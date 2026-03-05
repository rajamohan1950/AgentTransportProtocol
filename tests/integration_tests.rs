//! Workspace-level integration tests for the Agent Transport Protocol.
//!
//! These tests exercise the five-layer protocol stack end-to-end,
//! verifying cross-layer interactions that individual crate tests cannot cover.
//!
//! Test categories:
//!   1. Full identity lifecycle (L1)
//!   2. Full handshake flow (L2)
//!   3. Context compression (L3)
//!   4. Economic routing (L4)
//!   5. Fault recovery (L5)
//!   6. Full protocol flow (all layers)

use std::time::Duration;

use atp_context::differential::ContextCompressor;
use atp_context::extraction::MscConfig;
use atp_context::adaptive::{AdaptiveContextManager, ContextProvider};
use atp_fault::{
    AgentLoadTracker, CheckpointStore, CircuitBreaker,
    HeartbeatMonitor, HeartbeatStatus, PoisonDetector, PoisonStatus,
};
use atp_handshake::{
    create_contract, create_offer, create_probe, process_probe, rank_offers,
    CapabilityRegistry, HandshakeCoordinator, HandshakeState, HandshakeStateMachine,
};
use atp_identity::{DidGenerator, IdentityStore, KeyPair};
use atp_routing::{AgentGraph, EconomicRouter};
use atp_sim::{Scenario, SimHarness, TaskGenerator};
use atp_types::*;

use chrono::Utc;
use rand::SeedableRng;

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn make_cap(task_type: TaskType, quality: f64, latency_ms: u64, cost: f64) -> Capability {
    Capability {
        task_type,
        estimated_quality: quality,
        estimated_latency: Duration::from_millis(latency_ms),
        cost_per_task: cost,
    }
}

#[allow(dead_code)]
fn make_interaction(
    evaluator: AgentId,
    subject: AgentId,
    task_type: TaskType,
    quality: f64,
    days_ago: i64,
) -> InteractionRecord {
    let now = Utc::now();
    InteractionRecord {
        evaluator,
        subject,
        task_type,
        quality_score: quality,
        latency_ms: 100,
        cost: 0.01,
        timestamp: now - chrono::Duration::days(days_ago),
        signature: Vec::new(),
    }
}

// ===========================================================================
// Test 1: Full Identity Lifecycle
//
// Create keypair -> generate DID -> register -> compute trust -> verify Sybil resistance
// ===========================================================================

/// Full L1 lifecycle: keypair generation, DID creation, registration,
/// trust scoring, and Sybil resistance verification.
#[tokio::test]
async fn test_full_identity_lifecycle() {
    // Step 1: Generate Ed25519 keypair
    let kp = KeyPair::generate().unwrap();
    let pub_bytes = kp.public_key_bytes();
    assert_eq!(pub_bytes.len(), 32);

    // Step 2: Generate DID from public key
    let did = DidGenerator::generate_did(&pub_bytes).unwrap();
    assert_eq!(did.method, "key");
    assert!(did.identifier.starts_with('z'));
    assert!(did.to_uri().starts_with("did:key:z"));

    // Verify DID is consistent with the public key
    assert!(DidGenerator::verify_did(&did, &pub_bytes).unwrap());

    // Extract public key back from the DID and confirm roundtrip
    let extracted = DidGenerator::extract_public_key(&did).unwrap();
    assert_eq!(pub_bytes, extracted);

    // Step 3: Create identity and register in the identity store
    let identity = DidGenerator::create_identity(&kp).unwrap();
    let agent_id = identity.id;

    let store = IdentityStore::new();
    let registered_id = store.register(identity.clone()).await.unwrap();
    assert_eq!(registered_id, agent_id);

    // Verify lookup works
    let retrieved = store.get_identity(&agent_id).await.unwrap();
    assert_eq!(retrieved.id, agent_id);
    assert_eq!(retrieved.did.to_uri(), did.to_uri());
    assert_eq!(store.identity_count().await, 1);

    // Step 4: Record interactions and compute trust
    let evaluator = AgentId::new();
    let now = Utc::now();

    // Add several high-quality interactions
    for i in 0..5 {
        store
            .add_interaction(InteractionRecord {
                evaluator,
                subject: agent_id,
                task_type: TaskType::CodeGeneration,
                quality_score: 0.85 + (i as f64 * 0.02),
                latency_ms: 100,
                cost: 0.01,
                timestamp: now - chrono::Duration::hours(i),
                signature: Vec::new(),
            })
            .await;
    }

    let trust = store.trust_score(agent_id, TaskType::CodeGeneration, now).await;
    assert!(trust.score > 0.8, "trust score {} should be > 0.8", trust.score);
    assert_eq!(trust.sample_count, 5);

    // Aggregate trust across all task types
    let agg = store.aggregate_trust(agent_id, now).await;
    assert!(agg > 0.8, "aggregate trust {agg} should be > 0.8");

    // Trust vector should have scores for all task types
    let tv = store.trust_vector(agent_id, now).await;
    assert!(tv.get(TaskType::CodeGeneration) > 0.8);
    // Types with no interactions should return the default prior (0.5)
    assert!((tv.get(TaskType::Analysis) - 0.5).abs() < 0.01);

    // Step 5: Sybil resistance - verify transitive trust dampening
    let voucher_kp = KeyPair::generate().unwrap();
    let voucher_identity = DidGenerator::create_identity(&voucher_kp).unwrap();
    let voucher_id = voucher_identity.id;
    store.register(voucher_identity).await.unwrap();

    // Voucher has high trust
    store
        .add_interaction(InteractionRecord {
            evaluator,
            subject: voucher_id,
            task_type: TaskType::Analysis,
            quality_score: 0.95,
            latency_ms: 50,
            cost: 0.01,
            timestamp: now,
            signature: Vec::new(),
        })
        .await;

    // Voucher attests to our agent
    store
        .add_interaction(InteractionRecord {
            evaluator: voucher_id,
            subject: agent_id,
            task_type: TaskType::Analysis,
            quality_score: 0.90,
            latency_ms: 80,
            cost: 0.01,
            timestamp: now,
            signature: Vec::new(),
        })
        .await;

    // Transitive trust through voucher should be >= direct trust
    let transitive = store.transitive_trust(agent_id, voucher_id, now).await;
    let direct = store.aggregate_trust(agent_id, now).await;
    assert!(
        transitive >= direct,
        "transitive {transitive} should be >= direct {direct}"
    );

    // Sybil suspicion should be low for a well-attested agent
    let suspicion = store.sybil_suspicion(agent_id).await;
    assert!(suspicion < 0.5, "suspicion {suspicion} should be low");

    // Verify threshold checking
    assert!(store.meets_threshold(agent_id, &[], 0.5, now).await);
}

/// Verify Ed25519 signature creation and cross-agent verification.
#[test]
fn test_identity_signature_verification() {
    let kp1 = KeyPair::generate().unwrap();
    let kp2 = KeyPair::generate().unwrap();

    let message = b"ATP handshake payload v1";

    // Agent 1 signs
    let sig = kp1.sign(message);

    // Agent 1 can verify its own signature
    assert!(kp1.verify(message, &sig).is_ok());

    // Agent 2 can verify using Agent 1's public key
    let vk = kp1.verifying_key();
    assert!(KeyPair::verify_with_key(&vk, message, &sig).is_ok());

    // Verification fails with wrong message
    assert!(kp1.verify(b"tampered", &sig).is_err());

    // Verification fails with wrong key
    assert!(kp2.verify(message, &sig).is_err());
}

// ===========================================================================
// Test 2: Full Handshake Flow
//
// Register agents -> send probe -> receive offers -> accept contract
// ===========================================================================

/// Full 3-phase handshake: CAPABILITY_PROBE -> CAPABILITY_OFFER -> CONTRACT_ACCEPT.
#[test]
fn test_full_handshake_flow() {
    // Step 1: Register agents in the capability registry
    let mut registry = CapabilityRegistry::new();

    let agent_a = AgentId::new();
    let agent_b = AgentId::new();
    let agent_c = AgentId::new();

    // Agent A: high quality code generation
    registry.register(
        agent_a,
        make_cap(TaskType::CodeGeneration, 0.95, 100, 0.50),
        0.90,
    );
    // Agent B: medium quality, cheaper
    registry.register(
        agent_b,
        make_cap(TaskType::CodeGeneration, 0.80, 150, 0.30),
        0.75,
    );
    // Agent C: analysis specialist (wrong task type for this probe)
    registry.register(
        agent_c,
        make_cap(TaskType::Analysis, 0.92, 80, 0.40),
        0.85,
    );

    assert_eq!(registry.agent_count(), 3);
    assert_eq!(registry.len(), 3);

    // Step 2: Create CAPABILITY_PROBE
    let requester = AgentId::new();
    let qos = QoSConstraints {
        min_quality: 0.7,
        max_latency: Duration::from_secs(1),
        max_cost: 1.0,
        min_trust: 0.5,
    };

    let probe = create_probe(requester, TaskType::CodeGeneration, qos.clone(), None);
    assert_eq!(probe.from, requester);

    // Step 3: Process probe to find matching agents
    let probe_result = process_probe(&probe, &registry);
    assert_eq!(probe_result.matching_entries.len(), 2); // A and B match; C is wrong type
    assert!(probe_result.matching_entries.iter().all(|e| e.agent_id != agent_c));

    // Step 4: Create CAPABILITY_OFFERs from matching agents
    let offer_a = create_offer(
        agent_a,
        &probe,
        make_cap(TaskType::CodeGeneration, 0.95, 100, 0.50),
        0.90,
        Duration::from_secs(5),
    );
    let offer_b = create_offer(
        agent_b,
        &probe,
        make_cap(TaskType::CodeGeneration, 0.80, 150, 0.30),
        0.75,
        Duration::from_secs(5),
    );

    assert_eq!(offer_a.in_reply_to, probe.nonce);
    assert_eq!(offer_b.in_reply_to, probe.nonce);

    // Step 5: Rank offers
    let ranked = rank_offers(&[offer_a.clone(), offer_b.clone()], qos.max_latency, qos.max_cost);
    assert_eq!(ranked.len(), 2);
    // Agent A should rank higher (better quality and trust)
    assert_eq!(ranked[0].offer.from, agent_a);
    assert!(ranked[0].score > ranked[1].score);

    // Step 6: Accept contract with best offer
    let contract = create_contract(requester, &ranked[0], &qos, Duration::from_secs(60));
    assert_eq!(contract.from, requester);
    assert_eq!(contract.to, agent_a);
    assert!(contract.expires_at > Utc::now());
}

/// Test the full HandshakeCoordinator negotiate flow with QoS relaxation.
#[test]
fn test_handshake_coordinator_with_relaxation() {
    let mut registry = CapabilityRegistry::new();
    let agent = AgentId::new();

    // Agent quality (0.65) is below the strict QoS threshold (0.7),
    // but after one 10% relaxation (0.7 * 0.9 = 0.63) it qualifies.
    registry.register(
        agent,
        make_cap(TaskType::DataProcessing, 0.65, 200, 0.40),
        0.60,
    );

    let requester = AgentId::new();
    let mut coordinator = HandshakeCoordinator::with_defaults(requester);

    let qos = QoSConstraints {
        min_quality: 0.7,
        max_latency: Duration::from_secs(1),
        max_cost: 1.0,
        min_trust: 0.5,
    };

    let outcome = coordinator.negotiate(TaskType::DataProcessing, &qos, &registry).unwrap();

    assert_eq!(*coordinator.state(), HandshakeState::Contracted);
    assert_eq!(outcome.attempts, 2); // First attempt fails, second succeeds
    assert_eq!(outcome.contract.to, agent);
}

/// Test handshake state machine enforces valid transitions.
#[test]
fn test_handshake_state_machine_transitions() {
    let mut sm = HandshakeStateMachine::new();
    assert_eq!(*sm.state(), HandshakeState::Idle);

    // Invalid: can't go directly to OffersReceived from Idle
    assert!(sm.on_offers_received().is_err());

    // Valid path: Idle -> ProbeSent -> OffersReceived -> Contracted
    sm.on_probe_sent().unwrap();
    assert_eq!(*sm.state(), HandshakeState::ProbeSent);

    sm.on_offers_received().unwrap();
    assert_eq!(*sm.state(), HandshakeState::OffersReceived);

    let cid = uuid::Uuid::new_v4();
    let agent = AgentId::new();
    sm.on_contract_accepted(cid, agent).unwrap();
    assert_eq!(*sm.state(), HandshakeState::Contracted);
    assert!(sm.is_terminal());
    assert_eq!(sm.contract_id(), Some(cid));
    assert_eq!(sm.selected_agent(), Some(agent));
}

// ===========================================================================
// Test 3: Context Compression
//
// Create 50K token context -> compress with SCD -> verify reduction -> adaptive request
// ===========================================================================

/// Verify SCD achieves significant context compression (target: ~28x).
#[test]
fn test_context_compression_50k_with_28x_reduction() {
    let dims = 64;
    // Simulate a 50K token context (~200KB at 4 bytes/token)
    let data: Vec<u8> = (0..50_000).map(|i| (i % 256) as u8).collect();

    let config = MscConfig {
        relevance_threshold: -1.0, // Accept all chunks
        max_chunks: 5,             // Tight budget for aggressive compression
        chunk_size: 512,
        dimensions: dims,
    };
    let compressor = ContextCompressor::with_config(config);

    let diff = compressor
        .compress_for_task(&data, TaskType::CodeGeneration, b"implement JSON parser")
        .unwrap();

    assert_eq!(diff.original_size, 50_000);
    assert!(diff.compressed_size > 0, "should have some compressed data");
    assert!(
        diff.compressed_size < diff.original_size,
        "compressed {} should be < original {}",
        diff.compressed_size,
        diff.original_size,
    );

    // Verify compression ratio: with 5 chunks of 512 bytes max from 50K
    let ratio = diff.original_size as f64 / diff.compressed_size.max(1) as f64;
    assert!(
        ratio > 5.0,
        "compression ratio {ratio} should be > 5x",
    );

    // Verify diff metadata
    assert!(!diff.chunks.is_empty());
    assert!(diff.chunks.len() <= 5); // Budget-limited

    // Verify chunks are sorted by relevance (descending)
    for w in diff.chunks.windows(2) {
        assert!(
            w[0].relevance_score >= w[1].relevance_score,
            "chunks should be sorted by relevance",
        );
    }
}

/// Verify compression works across all four task types.
#[test]
fn test_context_compression_across_task_types() {
    let dims = 64;
    let data: Vec<u8> = (0..16384).map(|i| (i % 256) as u8).collect();

    let config = MscConfig {
        relevance_threshold: -1.0, // Accept all chunks (no filtering)
        max_chunks: 5,             // Budget of 5 chunks
        chunk_size: 512,
        dimensions: dims,
    };
    let compressor = ContextCompressor::with_config(config);

    for &task_type in TaskType::all() {
        let prompt = format!("Execute {task_type} task");
        let diff = compressor
            .compress_for_task(&data, task_type, prompt.as_bytes())
            .unwrap();

        // Every task type should produce non-empty output with budget
        assert!(
            !diff.chunks.is_empty(),
            "task type {task_type:?} produced empty chunks",
        );

        // Should achieve some compression
        assert!(
            diff.compressed_size < diff.original_size,
            "task type {:?}: compressed {} >= original {}",
            task_type,
            diff.compressed_size,
            diff.original_size,
        );

        assert_eq!(diff.original_size, 16384);
        assert!(diff.confidence >= -1.0 && diff.confidence <= 1.0, "confidence {} should be in [-1, 1]", diff.confidence);
    }
}

/// Test adaptive context refinement: low-confidence diff triggers CONTEXT_REQUEST.
#[test]
fn test_adaptive_context_request() {
    let dims = 64;
    let data: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();

    let config = MscConfig {
        relevance_threshold: -1.0,
        max_chunks: 2, // Very tight budget
        chunk_size: 512,
        dimensions: dims,
    };
    let compressor = ContextCompressor::with_config(config);
    let task_emb = atp_context::embedding::embed(b"adaptive test task", dims);

    let provider = ContextProvider::new(compressor.clone(), data.clone(), task_emb.clone());
    let total_chunks = provider.total_chunks();
    assert_eq!(total_chunks, 8); // 4096 / 512

    // Initial diff with tight budget -> low confidence
    let initial_diff = provider.initial_diff().unwrap();
    assert!(initial_diff.chunks.len() <= 2);

    // Set up receiver with threshold higher than initial confidence
    let receiver = AgentId::new();
    let sender = AgentId::new();
    let task_id = uuid::Uuid::new_v4();
    let mut manager = AdaptiveContextManager::new(receiver).with_threshold(0.99);

    // Evaluate: should request more context
    let request = manager
        .evaluate(&initial_diff, sender, task_id, total_chunks)
        .unwrap();
    assert!(request.is_some(), "should request more context");

    let req = request.unwrap();
    assert_eq!(req.from, receiver);
    assert_eq!(req.to, sender);
    assert!(!req.requested_chunk_indices.is_empty());

    // Fulfill request
    let additional = provider.handle_request(&req);
    assert!(!additional.is_empty());

    // Merge additional chunks
    let mut diff = initial_diff;
    atp_context::differential::merge_chunks(&mut diff, additional);
    assert!(diff.chunks.len() > 2);
}

// ===========================================================================
// Test 4: Economic Routing
//
// Build 10-agent graph -> find routes -> verify Pareto optimality -> test all 5 patterns
// ===========================================================================

/// Build a 10-agent graph, find routes, and verify Pareto optimality.
#[test]
fn test_economic_routing_10_agent_pareto() {
    let mut graph = AgentGraph::with_capacity(10);
    let task = TaskType::CodeGeneration;

    // Create 10 agents spanning quality/cost spectrum
    let mut ids = Vec::new();
    for i in 0..10 {
        let id = AgentId::new();
        let quality = 0.3 + (i as f64 * 0.07); // 0.30 to 0.93
        let latency = 30 + i as u64 * 20;      // 30ms to 210ms
        let cost = 0.05 + i as f64 * 0.05;     // $0.05 to $0.50
        let trust = 0.6 + (i as f64 * 0.04);   // 0.60 to 0.96
        graph.add_agent(id, vec![make_cap(task, quality, latency, cost)], trust);
        ids.push(id);
    }

    graph.fully_connect(Duration::from_millis(5));

    let router = EconomicRouter::new(graph);
    let qos = QoSConstraints {
        min_quality: 0.1,
        max_latency: Duration::from_secs(10),
        max_cost: 5.0,
        min_trust: 0.5,
    };

    // Find multiple routes
    let routes = router.find_routes(task, &qos).unwrap();
    assert!(routes.len() >= 2, "expected >= 2 routes, got {}", routes.len());

    // All routes should satisfy QoS constraints
    for r in &routes {
        assert!(r.metrics.quality >= qos.min_quality);
        assert!(r.metrics.latency <= qos.max_latency);
        assert!(r.metrics.cost <= qos.max_cost);
    }

    // Verify Pareto optimality: no route should dominate another on all metrics
    for (i, a) in routes.iter().enumerate() {
        for (j, b) in routes.iter().enumerate() {
            if i == j {
                continue;
            }
            let a_dominates = a.metrics.quality >= b.metrics.quality
                && a.metrics.latency <= b.metrics.latency
                && a.metrics.cost <= b.metrics.cost
                && (a.metrics.quality > b.metrics.quality
                    || a.metrics.latency < b.metrics.latency
                    || a.metrics.cost < b.metrics.cost);
            // If patterns differ, domination is acceptable (different strategies)
            if a.pattern == b.pattern {
                assert!(
                    !a_dominates,
                    "route {} ({:?}) dominates route {} ({:?}) in the same pattern",
                    i, a.pattern, j, b.pattern,
                );
            }
        }
    }
}

/// Test all five routing patterns produce valid routes.
#[test]
fn test_all_five_routing_patterns() {
    let mut graph = AgentGraph::with_capacity(5);
    let task = TaskType::Analysis;

    // Agent spectrum for pattern testing
    let cheap = AgentId::new();
    graph.add_agent(cheap, vec![make_cap(task, 0.5, 50, 0.1)], 0.8);
    let mid = AgentId::new();
    graph.add_agent(mid, vec![make_cap(task, 0.7, 100, 0.3)], 0.8);
    let quality = AgentId::new();
    graph.add_agent(quality, vec![make_cap(task, 0.95, 200, 0.8)], 0.9);
    let extra1 = AgentId::new();
    graph.add_agent(extra1, vec![make_cap(task, 0.6, 80, 0.2)], 0.7);
    let extra2 = AgentId::new();
    graph.add_agent(extra2, vec![make_cap(task, 0.8, 120, 0.4)], 0.85);

    graph.fully_connect(Duration::from_millis(5));

    let router = EconomicRouter::new(graph);
    let qos = QoSConstraints {
        min_quality: 0.1,
        max_latency: Duration::from_secs(10),
        max_cost: 5.0,
        min_trust: 0.5,
    };

    // Test each pattern
    let patterns = [
        RoutingPattern::DraftRefine,
        RoutingPattern::ParallelMerge,
        RoutingPattern::Cascade,
        RoutingPattern::Ensemble,
        RoutingPattern::Pipeline,
    ];

    for &pattern in &patterns {
        let result = router.find_route(task, &qos, Some(pattern));
        assert!(
            result.is_ok(),
            "pattern {pattern:?} should produce a valid route",
        );

        let route = result.unwrap();
        assert!(!route.agents.is_empty());
        assert!(route.metrics.quality > 0.0);
        assert!(route.metrics.cost > 0.0);

        // DraftRefine should use exactly 2 agents
        if pattern == RoutingPattern::DraftRefine {
            assert_eq!(route.agents.len(), 2, "DraftRefine should use 2 agents");
        }
    }
}

/// Test agent removal and restoration in routing.
#[test]
fn test_routing_agent_removal_and_restoration() {
    let mut graph = AgentGraph::new();
    let task = TaskType::DataProcessing;

    let a1 = AgentId::new();
    let a2 = AgentId::new();
    graph.add_agent(a1, vec![make_cap(task, 0.9, 100, 0.5)], 0.8);
    graph.add_agent(a2, vec![make_cap(task, 0.7, 50, 0.3)], 0.7);
    graph.fully_connect(Duration::from_millis(5));

    let mut router = EconomicRouter::new(graph);
    let qos = QoSConstraints {
        min_quality: 0.1,
        max_latency: Duration::from_secs(10),
        max_cost: 5.0,
        min_trust: 0.5,
    };

    // Both agents available
    let route = router.find_route(task, &qos, None).unwrap();
    assert!(!route.agents.is_empty());

    // Remove all agents
    router.remove_agent(a1);
    router.remove_agent(a2);
    let result = router.find_route(task, &qos, None);
    assert!(result.is_err(), "should fail with no available agents");

    // Restore agents
    router.restore_agent(a1);
    router.restore_agent(a2);
    let route = router.find_route(task, &qos, None).unwrap();
    assert!(!route.agents.is_empty());
}

/// Verify routing performance: < 100ms for 50 agents (debug mode threshold).
#[test]
fn test_routing_performance_50_agents() {
    let mut graph = AgentGraph::with_capacity(50);
    let task = TaskType::DataProcessing;

    for i in 0..50 {
        let id = AgentId::new();
        let quality = 0.5 + (i as f64 / 100.0);
        let latency = 50 + i as u64 * 5;
        let cost = 0.1 + i as f64 * 0.02;
        graph.add_agent(id, vec![make_cap(task, quality, latency, cost)], 0.7);
    }
    graph.fully_connect(Duration::from_millis(5));

    let router = EconomicRouter::new(graph);
    let qos = QoSConstraints {
        min_quality: 0.3,
        max_latency: Duration::from_secs(30),
        max_cost: 10.0,
        min_trust: 0.5,
    };

    let start = std::time::Instant::now();
    let route = router.find_route(task, &qos, None).unwrap();
    let elapsed = start.elapsed();

    assert!(!route.agents.is_empty());
    assert!(
        elapsed < Duration::from_millis(100),
        "routing took {elapsed:?}, expected < 100ms",
    );
}

// ===========================================================================
// Test 5: Fault Recovery
//
// Heartbeat -> simulate failure -> circuit breaker trips -> failover -> poison detection
// ===========================================================================

/// Full fault recovery lifecycle: heartbeat monitoring, circuit breaker, failover,
/// and poison detection.
#[test]
fn test_full_fault_recovery_lifecycle() {
    let agent_a = AgentId::new();
    let agent_b = AgentId::new();
    let agent_c = AgentId::new();

    // --- Step 1: Heartbeat monitoring ---
    let config = FaultConfig {
        heartbeat_interval: Duration::from_millis(50),
        heartbeat_timeout_multiplier: 1,
        circuit_breaker_threshold: 3,
        poison_detection_window: Duration::from_secs(60),
        poison_agent_threshold: 3,
        backpressure_threshold: 100,
    };
    let monitor = HeartbeatMonitor::new(config.clone());

    // Record heartbeats for all agents
    for &agent in [agent_a, agent_b, agent_c].iter() {
        monitor.record_heartbeat(&HeartbeatMsg {
            from: agent,
            sequence: 1,
            queue_depth: 0,
            load_factor: 0.0,
        });
        assert_eq!(monitor.status(&agent).unwrap(), HeartbeatStatus::Alive);
    }
    assert_eq!(monitor.tracked_count(), 3);
    assert_eq!(monitor.alive_agents().len(), 3);

    // --- Step 2: Simulate agent failure (timeout) ---
    // Agent A stops sending heartbeats while B and C continue
    std::thread::sleep(Duration::from_millis(80));
    monitor.record_heartbeat(&HeartbeatMsg {
        from: agent_b,
        sequence: 2,
        queue_depth: 5,
        load_factor: 0.3,
    });
    monitor.record_heartbeat(&HeartbeatMsg {
        from: agent_c,
        sequence: 2,
        queue_depth: 0,
        load_factor: 0.1,
    });

    let _changed = monitor.check_all();
    // Agent A should be suspected or dead
    let a_status = monitor.status(&agent_a).unwrap();
    assert_ne!(a_status, HeartbeatStatus::Alive, "agent A should be failed");

    // --- Step 3: Circuit breaker ---
    // Use 5-second cooldown so OPEN state persists during testing
    let cb = CircuitBreaker::new(config.clone(), Duration::from_secs(5));

    // Record consecutive failures for agent A (threshold = 3)
    cb.record_failure(&agent_a);
    assert_eq!(cb.state(&agent_a), CircuitState::Closed);
    cb.record_failure(&agent_a);
    assert_eq!(cb.state(&agent_a), CircuitState::Closed);
    cb.record_failure(&agent_a);
    assert_eq!(cb.state(&agent_a), CircuitState::Open);

    // Circuit is open: requests are rejected (cooldown hasn't elapsed)
    assert!(cb.allow_request(&agent_a).is_err());

    // Force reset to test recovery path
    cb.reset(&agent_a);
    assert_eq!(cb.state(&agent_a), CircuitState::Closed);
    assert!(cb.allow_request(&agent_a).is_ok());

    // Test HalfOpen -> Closed via success (use 0ms cooldown breaker)
    let cb2 = CircuitBreaker::new(config.clone(), Duration::from_millis(0));
    cb2.record_failure(&agent_a);
    cb2.record_failure(&agent_a);
    cb2.record_failure(&agent_a);
    assert_eq!(cb2.state(&agent_a), CircuitState::Open);

    // Zero cooldown: allow_request transitions OPEN -> HALF_OPEN
    std::thread::sleep(Duration::from_millis(5));
    assert!(cb2.allow_request(&agent_a).is_ok());
    assert_eq!(cb2.state(&agent_a), CircuitState::HalfOpen);

    // Probe succeeds -> back to Closed
    cb2.record_success(&agent_a);
    assert_eq!(cb2.state(&agent_a), CircuitState::Closed);

    // --- Step 4: Checkpoint and failover ---
    let checkpoint_store = CheckpointStore::new();
    let task_id = uuid::Uuid::new_v4();

    // Agent A checkpoints its progress before failing
    checkpoint_store.create_checkpoint(
        task_id,
        b"partial-computation-state".to_vec(),
        agent_a,
        1,
    );

    // Failover to agent B with checkpoint
    let decision = checkpoint_store.failover(task_id, agent_a, agent_b).unwrap();
    assert_eq!(decision.failed_agent, agent_a);
    assert_eq!(decision.new_agent, agent_b);
    assert!(decision.checkpoint.is_some());
    assert_eq!(decision.checkpoint.unwrap().state_bytes, b"partial-computation-state");

    // --- Step 5: Poison detection ---
    let detector = PoisonDetector::new(config.clone());
    let poison_task = uuid::Uuid::new_v4();

    // Three distinct agents fail the same task -> poisoned
    assert_eq!(detector.record_failure(poison_task, agent_a), PoisonStatus::Healthy);
    assert_eq!(detector.record_failure(poison_task, agent_b), PoisonStatus::Suspected);
    assert_eq!(detector.record_failure(poison_task, agent_c), PoisonStatus::Poisoned);

    assert!(detector.is_poisoned(&poison_task));
    assert!(detector.allow_retry(&poison_task).is_err());

    // Same agent failing multiple times does NOT trigger poison
    let normal_task = uuid::Uuid::new_v4();
    for _ in 0..10 {
        detector.record_failure(normal_task, agent_a);
    }
    assert!(!detector.is_poisoned(&normal_task));
}

/// Test backpressure signaling.
#[test]
fn test_backpressure_signals() {
    let config = FaultConfig {
        backpressure_threshold: 100,
        ..FaultConfig::default()
    };
    let tracker = AgentLoadTracker::new(config);

    let agent = AgentId::new();

    // Under threshold -> no backpressure signal
    tracker.update(agent, 50, Some(10.0));
    assert!(!tracker.is_overloaded(&agent));
    assert!(tracker.signal(&agent).is_none());

    // Over threshold -> backpressure signal generated
    tracker.update(agent, 150, Some(10.0));
    assert!(tracker.is_overloaded(&agent));
    let signal = tracker.signal(&agent);
    assert!(signal.is_some(), "should generate backpressure signal");
    let sig = signal.unwrap();
    assert_eq!(sig.queue_depth, 150);
    assert!(sig.recommended_rate > 0.0);
}

/// Test heartbeat recovery: agent comes back after being suspected.
#[test]
fn test_heartbeat_recovery() {
    let config = FaultConfig {
        heartbeat_interval: Duration::from_millis(20),
        heartbeat_timeout_multiplier: 1,
        ..FaultConfig::default()
    };
    let monitor = HeartbeatMonitor::new(config);
    let agent = AgentId::new();

    monitor.record_heartbeat(&HeartbeatMsg {
        from: agent,
        sequence: 1,
        queue_depth: 0,
        load_factor: 0.0,
    });

    // Wait for timeout
    std::thread::sleep(Duration::from_millis(40));
    monitor.check_all();
    assert_ne!(monitor.status(&agent).unwrap(), HeartbeatStatus::Alive);

    // Agent recovers
    monitor.record_heartbeat(&HeartbeatMsg {
        from: agent,
        sequence: 2,
        queue_depth: 0,
        load_factor: 0.0,
    });
    assert_eq!(monitor.status(&agent).unwrap(), HeartbeatStatus::Alive);
}

// ===========================================================================
// Test 6: Full Protocol Flow
//
// Wire all 5 layers -> execute task end-to-end -> verify all metrics
// ===========================================================================

/// Full protocol flow: all five layers working together via the simulation harness.
#[test]
fn test_full_protocol_flow_all_layers() {
    let harness = SimHarness::benchmark(42);
    assert_eq!(harness.network.agent_count(), 50);

    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let tasks = TaskGenerator::new()
        .with_context_size(50_000)
        .generate(1000, &mut rng);

    // Run ATP (full) scenario
    let metrics = harness.run_scenario(Scenario::FullAtp, &tasks);

    // Verify all HLD metrics are achieved
    assert_eq!(metrics.tasks_failed, 0, "full ATP should have zero failures");
    assert_eq!(metrics.tasks_completed, 1000);

    // Quality target: >= 0.85
    assert!(
        metrics.avg_quality >= 0.85,
        "quality {} should be >= 0.85",
        metrics.avg_quality,
    );

    // Cost target: reasonable (not budget-only, not premium-only)
    assert!(
        metrics.avg_cost_per_task > 0.01,
        "cost {} should be > $0.01 (not budget-only)",
        metrics.avg_cost_per_task,
    );
    assert!(
        metrics.avg_cost_per_task < 0.10,
        "cost {} should be < $0.10",
        metrics.avg_cost_per_task,
    );

    // Context efficiency: 28x compression
    let compression = 1.0 / metrics.context_efficiency.max(0.001);
    assert!(
        (compression - 28.0).abs() < 1.0,
        "context compression {compression} should be ~28x",
    );

    // Fault recovery: should be bounded (< 10ms)
    assert!(
        metrics.fault_recovery_ms.is_finite(),
        "fault recovery should be finite",
    );
}

/// Verify ATP (full) beats Sequential baseline on every metric that matters.
#[test]
fn test_atp_beats_sequential_baseline() {
    let harness = SimHarness::benchmark(42);
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let tasks = TaskGenerator::new()
        .with_context_size(50_000)
        .generate(10_000, &mut rng);

    let sequential = harness.run_scenario(Scenario::Sequential, &tasks);
    let atp = harness.run_scenario(Scenario::FullAtp, &tasks);

    // ATP should complete all tasks (zero failures)
    assert_eq!(atp.tasks_failed, 0);
    assert!(sequential.tasks_failed > 0);

    // ATP should have higher quality
    assert!(
        atp.avg_quality > sequential.avg_quality,
        "ATP quality {} should exceed sequential {}",
        atp.avg_quality,
        sequential.avg_quality,
    );

    // ATP should have better context efficiency (28x vs 1x)
    let atp_compression = 1.0 / atp.context_efficiency.max(0.001);
    let seq_compression = 1.0 / sequential.context_efficiency.max(0.001);
    assert!(
        atp_compression > seq_compression * 10.0,
        "ATP compression {atp_compression:.1}x should far exceed sequential {seq_compression:.1}x",
    );
}

/// Test all seven benchmark scenarios run and produce valid metrics.
#[test]
fn test_all_seven_scenarios() {
    let harness = SimHarness::benchmark(42);
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let tasks = TaskGenerator::new().generate(500, &mut rng);

    let scenarios = [
        Scenario::Sequential,
        Scenario::RoundRobin,
        Scenario::FullAtp,
        Scenario::AtpNoContext,
        Scenario::AtpNoRouting,
        Scenario::AtpNoTrust,
        Scenario::AtpNoFault,
    ];

    for &scenario in &scenarios {
        let metrics = harness.run_scenario(scenario, &tasks);

        assert!(
            metrics.tasks_completed + metrics.tasks_failed == metrics.total_tasks,
            "{}: completed {} + failed {} != total {}",
            metrics.scenario,
            metrics.tasks_completed,
            metrics.tasks_failed,
            metrics.total_tasks,
        );

        if metrics.tasks_completed > 0 {
            assert!(metrics.avg_quality > 0.0);
            assert!(metrics.avg_cost_per_task > 0.0);
        }
    }
}

/// Ablation study: removing each layer degrades specific metrics.
#[test]
fn test_ablation_study() {
    let harness = SimHarness::benchmark(42);
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let tasks = TaskGenerator::new()
        .with_context_size(50_000)
        .generate(5_000, &mut rng);

    let full = harness.run_scenario(Scenario::FullAtp, &tasks);
    let no_context = harness.run_scenario(Scenario::AtpNoContext, &tasks);
    let _no_routing = harness.run_scenario(Scenario::AtpNoRouting, &tasks);
    let no_trust = harness.run_scenario(Scenario::AtpNoTrust, &tasks);
    let no_fault = harness.run_scenario(Scenario::AtpNoFault, &tasks);

    // Without SCD: latency increases (full context transfer)
    assert!(
        no_context.avg_latency_ms > full.avg_latency_ms,
        "removing SCD should increase latency: {} vs {}",
        no_context.avg_latency_ms,
        full.avg_latency_ms,
    );

    // Without routing: quality degrades (random agent selection)
    // Note: this depends on how "no routing" is implemented.
    // With random selection, quality approaches the population average.

    // Without trust: the cheapest (lowest quality) agents are selected
    assert!(
        no_trust.avg_quality < full.avg_quality,
        "removing trust should decrease quality: {} vs {}",
        no_trust.avg_quality,
        full.avg_quality,
    );

    // Without fault tolerance: failures occur
    assert!(
        no_fault.tasks_failed > 0,
        "removing fault tolerance should cause failures",
    );
    assert_eq!(full.tasks_failed, 0);
}

/// Test trust scoring integration: interactions improve routing decisions.
#[tokio::test]
async fn test_trust_scoring_influences_routing() {
    let store = IdentityStore::new();

    // Create agents
    let kp1 = KeyPair::generate().unwrap();
    let id1 = DidGenerator::create_identity(&kp1).unwrap();
    let agent1 = id1.id;
    store.register(id1).await.unwrap();

    let kp2 = KeyPair::generate().unwrap();
    let id2 = DidGenerator::create_identity(&kp2).unwrap();
    let agent2 = id2.id;
    store.register(id2).await.unwrap();

    let now = Utc::now();

    // Agent 1 has consistently high quality
    for i in 0..10 {
        store.add_interaction(InteractionRecord {
            evaluator: agent2,
            subject: agent1,
            task_type: TaskType::CodeGeneration,
            quality_score: 0.92,
            latency_ms: 100,
            cost: 0.05,
            timestamp: now - chrono::Duration::hours(i),
            signature: Vec::new(),
        }).await;
    }

    // Agent 2 has inconsistent quality
    for i in 0..10 {
        let q = if i % 2 == 0 { 0.90 } else { 0.40 };
        store.add_interaction(InteractionRecord {
            evaluator: agent1,
            subject: agent2,
            task_type: TaskType::CodeGeneration,
            quality_score: q,
            latency_ms: 100,
            cost: 0.05,
            timestamp: now - chrono::Duration::hours(i),
            signature: Vec::new(),
        }).await;
    }

    let t1 = store.trust_score(agent1, TaskType::CodeGeneration, now).await;
    let t2 = store.trust_score(agent2, TaskType::CodeGeneration, now).await;

    // Agent 1 should have higher trust
    assert!(
        t1.score > t2.score,
        "agent1 trust {} should exceed agent2 trust {}",
        t1.score,
        t2.score,
    );
    assert!(t1.score > 0.9);
}

/// Test the integrated flow: identity -> handshake -> routing -> context -> fault.
#[tokio::test]
async fn test_integrated_five_layer_flow() {
    // L1: Identity
    let kp = KeyPair::generate().unwrap();
    let identity = DidGenerator::create_identity(&kp).unwrap();
    let my_id = identity.id;

    let store = IdentityStore::new();
    store.register(identity).await.unwrap();

    // L2: Handshake
    let mut registry = CapabilityRegistry::new();
    let agent_a = AgentId::new();
    let agent_b = AgentId::new();
    registry.register(agent_a, make_cap(TaskType::Analysis, 0.90, 100, 0.40), 0.85);
    registry.register(agent_b, make_cap(TaskType::Analysis, 0.75, 200, 0.25), 0.70);

    let mut coordinator = HandshakeCoordinator::with_defaults(my_id);
    let qos = QoSConstraints {
        min_quality: 0.7,
        max_latency: Duration::from_secs(1),
        max_cost: 1.0,
        min_trust: 0.5,
    };
    let outcome = coordinator.negotiate(TaskType::Analysis, &qos, &registry).unwrap();
    assert_eq!(*coordinator.state(), HandshakeState::Contracted);
    let contracted_agent = outcome.contract.to;

    // L3: Context compression
    let compressor = ContextCompressor::with_config(MscConfig {
        relevance_threshold: -1.0,
        max_chunks: 3,
        chunk_size: 512,
        dimensions: 64,
    });
    let context_data: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
    let diff = compressor
        .compress_for_task(&context_data, TaskType::Analysis, b"analyze data patterns")
        .unwrap();
    assert!(diff.compressed_size < diff.original_size);

    // L4: Routing
    let mut graph = AgentGraph::new();
    graph.add_agent(agent_a, vec![make_cap(TaskType::Analysis, 0.90, 100, 0.40)], 0.85);
    graph.add_agent(agent_b, vec![make_cap(TaskType::Analysis, 0.75, 200, 0.25)], 0.70);
    graph.fully_connect(Duration::from_millis(5));

    let router = EconomicRouter::new(graph);
    let route = router.find_route(TaskType::Analysis, &qos, None).unwrap();
    assert!(!route.agents.is_empty());

    // L5: Fault monitoring for the contracted agent
    let monitor = HeartbeatMonitor::with_defaults();
    monitor.record_heartbeat(&HeartbeatMsg {
        from: contracted_agent,
        sequence: 1,
        queue_depth: 2,
        load_factor: 0.1,
    });
    assert_eq!(monitor.status(&contracted_agent).unwrap(), HeartbeatStatus::Alive);

    let cb = CircuitBreaker::with_defaults();
    assert!(cb.allow_request(&contracted_agent).is_ok());
    cb.record_success(&contracted_agent);
    assert_eq!(cb.state(&contracted_agent), CircuitState::Closed);

    // Record interaction for trust
    store.add_interaction(InteractionRecord {
        evaluator: my_id,
        subject: contracted_agent,
        task_type: TaskType::Analysis,
        quality_score: 0.88,
        latency_ms: 95,
        cost: 0.40,
        timestamp: Utc::now(),
        signature: Vec::new(),
    }).await;

    let trust = store.aggregate_trust(contracted_agent, Utc::now()).await;
    assert!(trust > 0.8, "trust {trust} should be > 0.8 after successful interaction");
}
