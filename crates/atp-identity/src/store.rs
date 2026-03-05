//! In-memory identity and interaction store.
//!
//! Provides a thread-safe store for [`AgentIdentity`] records and
//! [`InteractionRecord`] history, used by the trust engine and sybil guard.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use atp_types::{
    AgentId, AgentIdentity, IdentityError, InteractionRecord, TaskType, TrustScore,
    TrustVector,
};
use chrono::{DateTime, Utc};

use crate::sybil::SybilGuard;
use crate::trust::TrustEngine;

/// Thread-safe in-memory identity and interaction store.
#[derive(Clone)]
pub struct IdentityStore {
    inner: Arc<RwLock<StoreInner>>,
    trust_engine: Arc<TrustEngine>,
    sybil_guard: Arc<SybilGuard>,
}

struct StoreInner {
    /// Agent identities keyed by AgentId.
    identities: HashMap<AgentId, AgentIdentity>,
    /// All interaction records, keyed by subject AgentId.
    interactions: HashMap<AgentId, Vec<InteractionRecord>>,
}

impl IdentityStore {
    /// Create an empty store with default trust engine and sybil guard.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StoreInner {
                identities: HashMap::new(),
                interactions: HashMap::new(),
            })),
            trust_engine: Arc::new(TrustEngine::default()),
            sybil_guard: Arc::new(SybilGuard::default()),
        }
    }

    /// Create a store with custom trust engine and sybil guard.
    pub fn with_engines(trust_engine: TrustEngine, sybil_guard: SybilGuard) -> Self {
        Self {
            inner: Arc::new(RwLock::new(StoreInner {
                identities: HashMap::new(),
                interactions: HashMap::new(),
            })),
            trust_engine: Arc::new(trust_engine),
            sybil_guard: Arc::new(sybil_guard),
        }
    }

    // ── Identity management ──────────────────────────────────────────

    /// Register a new agent identity.
    pub async fn register(&self, identity: AgentIdentity) -> Result<AgentId, IdentityError> {
        let id = identity.id;
        let mut inner = self.inner.write().await;
        inner.identities.insert(id, identity);
        inner.interactions.entry(id).or_default();
        Ok(id)
    }

    /// Look up an identity by agent id.
    pub async fn get_identity(&self, id: &AgentId) -> Result<AgentIdentity, IdentityError> {
        let inner = self.inner.read().await;
        inner
            .identities
            .get(id)
            .cloned()
            .ok_or_else(|| IdentityError::UnknownAgent(id.to_string()))
    }

    /// List all registered agent ids.
    pub async fn list_agents(&self) -> Vec<AgentId> {
        let inner = self.inner.read().await;
        inner.identities.keys().copied().collect()
    }

    /// Remove an identity.
    pub async fn remove(&self, id: &AgentId) -> Result<AgentIdentity, IdentityError> {
        let mut inner = self.inner.write().await;
        let identity = inner
            .identities
            .remove(id)
            .ok_or_else(|| IdentityError::UnknownAgent(id.to_string()))?;
        inner.interactions.remove(id);
        Ok(identity)
    }

    /// Return the number of registered identities.
    pub async fn identity_count(&self) -> usize {
        let inner = self.inner.read().await;
        inner.identities.len()
    }

    // ── Interaction records ──────────────────────────────────────────

    /// Record a new interaction.
    pub async fn add_interaction(&self, record: InteractionRecord) {
        let subject = record.subject;
        let mut inner = self.inner.write().await;
        inner.interactions.entry(subject).or_default().push(record);
    }

    /// Get all interaction records for a given subject.
    pub async fn get_interactions(&self, subject: &AgentId) -> Vec<InteractionRecord> {
        let inner = self.inner.read().await;
        inner
            .interactions
            .get(subject)
            .cloned()
            .unwrap_or_default()
    }

    /// Get all interaction records in the store (across all subjects).
    pub async fn all_interactions(&self) -> Vec<InteractionRecord> {
        let inner = self.inner.read().await;
        inner
            .interactions
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect()
    }

    /// Return the total number of interaction records stored.
    pub async fn interaction_count(&self) -> usize {
        let inner = self.inner.read().await;
        inner.interactions.values().map(|v| v.len()).sum()
    }

    // ── Trust queries ────────────────────────────────────────────────

    /// Compute the trust score for an agent on a given task type.
    pub async fn trust_score(
        &self,
        subject: AgentId,
        task_type: TaskType,
        now: DateTime<Utc>,
    ) -> TrustScore {
        let inner = self.inner.read().await;
        let records = inner
            .interactions
            .get(&subject)
            .cloned()
            .unwrap_or_default();
        self.trust_engine
            .compute_trust_score(subject, task_type, &records, now)
    }

    /// Compute the full trust vector for an agent.
    pub async fn trust_vector(
        &self,
        subject: AgentId,
        now: DateTime<Utc>,
    ) -> TrustVector {
        let inner = self.inner.read().await;
        let records = inner
            .interactions
            .get(&subject)
            .cloned()
            .unwrap_or_default();
        self.trust_engine
            .compute_trust_vector(subject, &records, now)
    }

    /// Compute aggregate trust for an agent across all task types.
    pub async fn aggregate_trust(
        &self,
        subject: AgentId,
        now: DateTime<Utc>,
    ) -> f64 {
        let inner = self.inner.read().await;
        let records = inner
            .interactions
            .get(&subject)
            .cloned()
            .unwrap_or_default();
        self.trust_engine
            .compute_aggregate_trust(subject, &records, now)
    }

    /// Compute transitive trust for `subject` via `voucher`.
    pub async fn transitive_trust(
        &self,
        subject: AgentId,
        voucher: AgentId,
        now: DateTime<Utc>,
    ) -> f64 {
        let all_records = self.all_interactions().await;
        self.sybil_guard
            .transitive_trust(subject, voucher, &all_records, now)
    }

    /// Check whether an agent meets a trust threshold,
    /// optionally through a voucher chain.
    pub async fn meets_threshold(
        &self,
        subject: AgentId,
        chain: &[AgentId],
        threshold: f64,
        now: DateTime<Utc>,
    ) -> bool {
        let all_records = self.all_interactions().await;
        self.sybil_guard
            .meets_threshold(subject, chain, &all_records, threshold, now)
    }

    /// Compute the sybil suspicion score for an agent.
    pub async fn sybil_suspicion(&self, subject: AgentId) -> f64 {
        let all_records = self.all_interactions().await;
        self.sybil_guard.sybil_suspicion(subject, &all_records)
    }
}

impl Default for IdentityStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::did::DidGenerator;
    use crate::keypair::KeyPair;

    async fn make_store_with_agent() -> (IdentityStore, AgentId) {
        let store = IdentityStore::new();
        let kp = KeyPair::generate().unwrap();
        let identity = DidGenerator::create_identity(&kp).unwrap();
        let id = identity.id;
        store.register(identity).await.unwrap();
        (store, id)
    }

    #[tokio::test]
    async fn register_and_lookup() {
        let (store, id) = make_store_with_agent().await;
        let retrieved = store.get_identity(&id).await.unwrap();
        assert_eq!(retrieved.id, id);
    }

    #[tokio::test]
    async fn unknown_agent() {
        let store = IdentityStore::new();
        let unknown = AgentId::new();
        assert!(store.get_identity(&unknown).await.is_err());
    }

    #[tokio::test]
    async fn remove_agent() {
        let (store, id) = make_store_with_agent().await;
        assert_eq!(store.identity_count().await, 1);
        store.remove(&id).await.unwrap();
        assert_eq!(store.identity_count().await, 0);
    }

    #[tokio::test]
    async fn add_and_query_interactions() {
        let (store, id) = make_store_with_agent().await;
        let evaluator = AgentId::new();
        let now = Utc::now();

        let record = InteractionRecord {
            evaluator,
            subject: id,
            task_type: TaskType::CodeGeneration,
            quality_score: 0.85,
            latency_ms: 100,
            cost: 0.01,
            timestamp: now,
            signature: Vec::new(),
        };

        store.add_interaction(record).await;
        let interactions = store.get_interactions(&id).await;
        assert_eq!(interactions.len(), 1);
        assert!((interactions[0].quality_score - 0.85).abs() < 1e-9);
    }

    #[tokio::test]
    async fn trust_score_query() {
        let (store, id) = make_store_with_agent().await;
        let evaluator = AgentId::new();
        let now = Utc::now();

        let record = InteractionRecord {
            evaluator,
            subject: id,
            task_type: TaskType::Analysis,
            quality_score: 0.9,
            latency_ms: 50,
            cost: 0.005,
            timestamp: now,
            signature: Vec::new(),
        };

        store.add_interaction(record).await;

        let ts = store.trust_score(id, TaskType::Analysis, now).await;
        assert!((ts.score - 0.9).abs() < 1e-6);
        assert_eq!(ts.sample_count, 1);
    }

    #[tokio::test]
    async fn aggregate_trust_query() {
        let (store, id) = make_store_with_agent().await;
        let evaluator = AgentId::new();
        let now = Utc::now();

        store
            .add_interaction(InteractionRecord {
                evaluator,
                subject: id,
                task_type: TaskType::CodeGeneration,
                quality_score: 0.8,
                latency_ms: 100,
                cost: 0.01,
                timestamp: now,
                signature: Vec::new(),
            })
            .await;

        let agg = store.aggregate_trust(id, now).await;
        assert!((agg - 0.8).abs() < 1e-6);
    }

    #[tokio::test]
    async fn list_agents() {
        let store = IdentityStore::new();
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let id1 = DidGenerator::create_identity(&kp1).unwrap();
        let id2 = DidGenerator::create_identity(&kp2).unwrap();

        store.register(id1).await.unwrap();
        store.register(id2).await.unwrap();

        let agents = store.list_agents().await;
        assert_eq!(agents.len(), 2);
    }
}
