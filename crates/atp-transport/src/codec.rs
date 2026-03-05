//! Proto <-> domain type conversions for the ATP transport layer.
//!
//! Since neither `atp_types` nor `atp_proto` types are defined in this crate,
//! Rust's orphan rules prevent implementing `From`/`TryFrom` directly.
//! Instead, this module provides standalone conversion functions grouped by
//! type, following the naming convention `domain_to_proto` / `proto_to_domain`.

use std::time::Duration;

use uuid::Uuid;

use atp_proto::atp::v1 as proto;
use atp_types::{
    AgentId, BackpressureMsg, Capability, CapabilityOfferMsg, CapabilityProbeMsg,
    CircuitBreakMsg, CircuitState, ContractAcceptMsg, ContextChunk, ContextDiff,
    ContextEmbedding, ContextRequestMsg, HeartbeatMsg, InteractionProofMsg, QoSConstraints,
    Route, RouteMetrics, RoutingPattern, TaskResult as DomainTaskResult, TaskSubmitMsg, TaskType,
};

// ---------------------------------------------------------------------------
// Error type for fallible conversions
// ---------------------------------------------------------------------------

/// Errors that can occur during proto <-> domain conversion.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    #[error("invalid UUID: {0}")]
    InvalidUuid(String),

    #[error("invalid enum value: {field} = {value}")]
    InvalidEnum { field: &'static str, value: i32 },
}

impl From<CodecError> for tonic::Status {
    fn from(err: CodecError) -> Self {
        tonic::Status::invalid_argument(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// AgentId
// ---------------------------------------------------------------------------

pub fn agent_id_to_proto(id: AgentId) -> proto::AgentId {
    proto::AgentId {
        uuid: id.0.to_string(),
    }
}

pub fn proto_to_agent_id(pb: proto::AgentId) -> Result<AgentId, CodecError> {
    let uuid =
        Uuid::parse_str(&pb.uuid).map_err(|_| CodecError::InvalidUuid(pb.uuid.clone()))?;
    Ok(AgentId(uuid))
}

/// Helper to extract an AgentId from an `Option<proto::AgentId>`.
pub fn require_agent_id(
    opt: Option<proto::AgentId>,
    field: &'static str,
) -> Result<AgentId, CodecError> {
    opt.ok_or(CodecError::MissingField(field))
        .and_then(proto_to_agent_id)
}

// ---------------------------------------------------------------------------
// TaskType
// ---------------------------------------------------------------------------

pub fn task_type_to_proto(tt: TaskType) -> proto::TaskType {
    match tt {
        TaskType::CodeGeneration => proto::TaskType::CodeGeneration,
        TaskType::Analysis => proto::TaskType::Analysis,
        TaskType::CreativeWriting => proto::TaskType::CreativeWriting,
        TaskType::DataProcessing => proto::TaskType::DataProcessing,
    }
}

pub fn proto_to_task_type(value: i32) -> Result<TaskType, CodecError> {
    match proto::TaskType::try_from(value) {
        Ok(proto::TaskType::CodeGeneration) => Ok(TaskType::CodeGeneration),
        Ok(proto::TaskType::Analysis) => Ok(TaskType::Analysis),
        Ok(proto::TaskType::CreativeWriting) => Ok(TaskType::CreativeWriting),
        Ok(proto::TaskType::DataProcessing) => Ok(TaskType::DataProcessing),
        Ok(proto::TaskType::Unspecified) | Err(_) => Err(CodecError::InvalidEnum {
            field: "task_type",
            value,
        }),
    }
}

/// Convert a domain `TaskType` to the proto i32 representation.
pub fn task_type_to_i32(tt: TaskType) -> i32 {
    task_type_to_proto(tt) as i32
}

/// Convert a proto i32 to a domain `TaskType`.
pub fn task_type_from_i32(value: i32) -> Result<TaskType, CodecError> {
    proto_to_task_type(value)
}

// ---------------------------------------------------------------------------
// RoutingPattern
// ---------------------------------------------------------------------------

pub fn routing_pattern_to_proto(rp: RoutingPattern) -> proto::RoutingPattern {
    match rp {
        RoutingPattern::DraftRefine => proto::RoutingPattern::DraftRefine,
        RoutingPattern::ParallelMerge => proto::RoutingPattern::ParallelMerge,
        RoutingPattern::Cascade => proto::RoutingPattern::Cascade,
        RoutingPattern::Ensemble => proto::RoutingPattern::Ensemble,
        RoutingPattern::Pipeline => proto::RoutingPattern::Pipeline,
    }
}

pub fn proto_to_routing_pattern(value: i32) -> Result<RoutingPattern, CodecError> {
    match proto::RoutingPattern::try_from(value) {
        Ok(proto::RoutingPattern::DraftRefine) => Ok(RoutingPattern::DraftRefine),
        Ok(proto::RoutingPattern::ParallelMerge) => Ok(RoutingPattern::ParallelMerge),
        Ok(proto::RoutingPattern::Cascade) => Ok(RoutingPattern::Cascade),
        Ok(proto::RoutingPattern::Ensemble) => Ok(RoutingPattern::Ensemble),
        Ok(proto::RoutingPattern::Pipeline) => Ok(RoutingPattern::Pipeline),
        Ok(proto::RoutingPattern::Unspecified) | Err(_) => Err(CodecError::InvalidEnum {
            field: "routing_pattern",
            value,
        }),
    }
}

/// Convert a domain `RoutingPattern` to the proto i32 representation.
pub fn routing_pattern_to_i32(rp: RoutingPattern) -> i32 {
    routing_pattern_to_proto(rp) as i32
}

// ---------------------------------------------------------------------------
// CircuitState
// ---------------------------------------------------------------------------

pub fn circuit_state_to_proto(cs: CircuitState) -> proto::CircuitState {
    match cs {
        CircuitState::Closed => proto::CircuitState::Closed,
        CircuitState::Open => proto::CircuitState::Open,
        CircuitState::HalfOpen => proto::CircuitState::HalfOpen,
    }
}

pub fn proto_to_circuit_state(value: i32) -> Result<CircuitState, CodecError> {
    match proto::CircuitState::try_from(value) {
        Ok(proto::CircuitState::Closed) => Ok(CircuitState::Closed),
        Ok(proto::CircuitState::Open) => Ok(CircuitState::Open),
        Ok(proto::CircuitState::HalfOpen) => Ok(CircuitState::HalfOpen),
        Ok(proto::CircuitState::Unspecified) | Err(_) => Err(CodecError::InvalidEnum {
            field: "circuit_state",
            value,
        }),
    }
}

// ---------------------------------------------------------------------------
// Timestamp helpers
// ---------------------------------------------------------------------------

pub fn datetime_to_proto(dt: chrono::DateTime<chrono::Utc>) -> proto::Timestamp {
    proto::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

pub fn proto_to_datetime(ts: proto::Timestamp) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(ts.seconds, ts.nanos.max(0) as u32)
        .unwrap_or_else(chrono::Utc::now)
}

pub fn now_proto() -> proto::Timestamp {
    datetime_to_proto(chrono::Utc::now())
}

// ---------------------------------------------------------------------------
// QoSConstraints
// ---------------------------------------------------------------------------

pub fn qos_to_proto(qos: QoSConstraints) -> proto::QoSConstraints {
    proto::QoSConstraints {
        min_quality: qos.min_quality,
        max_latency_ms: qos.max_latency.as_millis() as u64,
        max_cost: qos.max_cost,
        min_trust: qos.min_trust,
    }
}

pub fn proto_to_qos(pb: proto::QoSConstraints) -> QoSConstraints {
    QoSConstraints {
        min_quality: pb.min_quality,
        max_latency: Duration::from_millis(pb.max_latency_ms),
        max_cost: pb.max_cost,
        min_trust: pb.min_trust,
    }
}

// ---------------------------------------------------------------------------
// Capability
// ---------------------------------------------------------------------------

pub fn capability_to_proto(cap: Capability) -> proto::Capability {
    proto::Capability {
        task_type: task_type_to_i32(cap.task_type),
        estimated_quality: cap.estimated_quality,
        estimated_latency_ms: cap.estimated_latency.as_millis() as u64,
        cost_per_task: cap.cost_per_task,
    }
}

pub fn proto_to_capability(pb: proto::Capability) -> Result<Capability, CodecError> {
    Ok(Capability {
        task_type: proto_to_task_type(pb.task_type)?,
        estimated_quality: pb.estimated_quality,
        estimated_latency: Duration::from_millis(pb.estimated_latency_ms),
        cost_per_task: pb.cost_per_task,
    })
}

// ---------------------------------------------------------------------------
// ContextEmbedding
// ---------------------------------------------------------------------------

pub fn embedding_to_proto(emb: ContextEmbedding) -> proto::Embedding {
    proto::Embedding {
        dimensions: emb.dimensions as u32,
        values: emb.values,
    }
}

pub fn proto_to_embedding(pb: proto::Embedding) -> ContextEmbedding {
    ContextEmbedding {
        dimensions: pb.dimensions as usize,
        values: pb.values,
    }
}

// ---------------------------------------------------------------------------
// ContextChunk
// ---------------------------------------------------------------------------

pub fn context_chunk_to_proto(chunk: ContextChunk) -> proto::ContextChunk {
    proto::ContextChunk {
        index: chunk.index,
        data: chunk.data,
        relevance_score: chunk.relevance_score,
    }
}

pub fn proto_to_context_chunk(pb: proto::ContextChunk) -> ContextChunk {
    ContextChunk {
        index: pb.index,
        data: pb.data,
        relevance_score: pb.relevance_score,
    }
}

// ---------------------------------------------------------------------------
// ContextDiff
// ---------------------------------------------------------------------------

pub fn context_diff_to_proto(diff: ContextDiff) -> proto::ContextDiff {
    proto::ContextDiff {
        base_hash: diff.base_hash.to_vec(),
        chunks: diff.chunks.into_iter().map(context_chunk_to_proto).collect(),
        confidence: diff.confidence,
        original_size: diff.original_size,
        compressed_size: diff.compressed_size,
    }
}

pub fn proto_to_context_diff(pb: proto::ContextDiff) -> ContextDiff {
    let mut base_hash = [0u8; 32];
    let len = pb.base_hash.len().min(32);
    base_hash[..len].copy_from_slice(&pb.base_hash[..len]);
    ContextDiff {
        base_hash,
        chunks: pb.chunks.into_iter().map(proto_to_context_chunk).collect(),
        confidence: pb.confidence,
        original_size: pb.original_size,
        compressed_size: pb.compressed_size,
    }
}

// ---------------------------------------------------------------------------
// RouteMetrics
// ---------------------------------------------------------------------------

pub fn route_metrics_to_proto(rm: RouteMetrics) -> proto::RouteMetrics {
    proto::RouteMetrics {
        quality: rm.quality,
        latency_ms: rm.latency.as_millis() as u64,
        cost: rm.cost,
    }
}

pub fn proto_to_route_metrics(pb: proto::RouteMetrics) -> RouteMetrics {
    RouteMetrics {
        quality: pb.quality,
        latency: Duration::from_millis(pb.latency_ms),
        cost: pb.cost,
    }
}

// ---------------------------------------------------------------------------
// Route
// ---------------------------------------------------------------------------

pub fn route_to_proto(route: Route) -> proto::Route {
    proto::Route {
        route_id: route.id.to_string(),
        pattern: routing_pattern_to_i32(route.pattern),
        agents: route.agents.into_iter().map(agent_id_to_proto).collect(),
        metrics: Some(route_metrics_to_proto(route.metrics)),
        computed_at: Some(datetime_to_proto(route.computed_at)),
        ttl_ms: route.ttl.as_millis() as u64,
    }
}

pub fn proto_to_route(pb: proto::Route) -> Result<Route, CodecError> {
    let id = Uuid::parse_str(&pb.route_id)
        .map_err(|_| CodecError::InvalidUuid(pb.route_id.clone()))?;
    let pattern = proto_to_routing_pattern(pb.pattern)?;
    let agents: Result<Vec<AgentId>, _> =
        pb.agents.into_iter().map(proto_to_agent_id).collect();
    let metrics = proto_to_route_metrics(
        pb.metrics.ok_or(CodecError::MissingField("metrics"))?,
    );
    let computed_at = pb
        .computed_at
        .map(proto_to_datetime)
        .unwrap_or_else(chrono::Utc::now);

    Ok(Route {
        id,
        pattern,
        agents: agents?,
        metrics,
        computed_at,
        ttl: Duration::from_millis(pb.ttl_ms),
    })
}

// ---------------------------------------------------------------------------
// CapabilityProbeMsg
// ---------------------------------------------------------------------------

pub fn probe_to_proto(msg: CapabilityProbeMsg) -> proto::CapabilityProbe {
    proto::CapabilityProbe {
        from: Some(agent_id_to_proto(msg.from)),
        task_type: task_type_to_i32(msg.task_type),
        qos: Some(qos_to_proto(msg.qos)),
        context_embedding: msg.context_embedding.map(embedding_to_proto),
        nonce: msg.nonce,
        timestamp: Some(datetime_to_proto(msg.timestamp)),
        signature: msg.signature,
    }
}

pub fn proto_to_probe(pb: proto::CapabilityProbe) -> Result<CapabilityProbeMsg, CodecError> {
    Ok(CapabilityProbeMsg {
        from: require_agent_id(pb.from, "from")?,
        task_type: proto_to_task_type(pb.task_type)?,
        qos: proto_to_qos(pb.qos.ok_or(CodecError::MissingField("qos"))?),
        context_embedding: pb.context_embedding.map(proto_to_embedding),
        nonce: pb.nonce,
        timestamp: pb
            .timestamp
            .map(proto_to_datetime)
            .unwrap_or_else(chrono::Utc::now),
        signature: pb.signature,
    })
}

// ---------------------------------------------------------------------------
// CapabilityOfferMsg
// ---------------------------------------------------------------------------

pub fn offer_to_proto(msg: CapabilityOfferMsg) -> proto::CapabilityOffer {
    proto::CapabilityOffer {
        from: Some(agent_id_to_proto(msg.from)),
        in_reply_to: msg.in_reply_to,
        capability: Some(capability_to_proto(msg.capability)),
        trust_score: msg.trust_score,
        trust_proof: msg.trust_proof,
        ttl_ms: msg.ttl.as_millis() as u64,
        timestamp: Some(datetime_to_proto(msg.timestamp)),
        signature: msg.signature,
    }
}

pub fn proto_to_offer(pb: proto::CapabilityOffer) -> Result<CapabilityOfferMsg, CodecError> {
    Ok(CapabilityOfferMsg {
        from: require_agent_id(pb.from, "from")?,
        in_reply_to: pb.in_reply_to,
        capability: proto_to_capability(
            pb.capability.ok_or(CodecError::MissingField("capability"))?,
        )?,
        trust_score: pb.trust_score,
        trust_proof: pb.trust_proof,
        ttl: Duration::from_millis(pb.ttl_ms),
        timestamp: pb
            .timestamp
            .map(proto_to_datetime)
            .unwrap_or_else(chrono::Utc::now),
        signature: pb.signature,
    })
}

// ---------------------------------------------------------------------------
// ContractAcceptMsg
// ---------------------------------------------------------------------------

pub fn contract_accept_to_proto(msg: ContractAcceptMsg) -> proto::ContractAccept {
    proto::ContractAccept {
        from: Some(agent_id_to_proto(msg.from)),
        to: Some(agent_id_to_proto(msg.to)),
        agreed_qos: Some(qos_to_proto(msg.agreed_qos)),
        context_plan: msg.context_plan,
        contract_id: msg.contract_id.to_string(),
        expires_ms: msg.expires_at.timestamp_millis() as u64,
        timestamp: Some(datetime_to_proto(msg.timestamp)),
        signature: msg.signature,
    }
}

pub fn proto_to_contract_accept(
    pb: proto::ContractAccept,
) -> Result<ContractAcceptMsg, CodecError> {
    let contract_id = Uuid::parse_str(&pb.contract_id)
        .map_err(|_| CodecError::InvalidUuid(pb.contract_id.clone()))?;
    let expires_at = chrono::DateTime::from_timestamp_millis(pb.expires_ms as i64)
        .unwrap_or_else(chrono::Utc::now);

    Ok(ContractAcceptMsg {
        from: require_agent_id(pb.from, "from")?,
        to: require_agent_id(pb.to, "to")?,
        agreed_qos: proto_to_qos(
            pb.agreed_qos.ok_or(CodecError::MissingField("agreed_qos"))?,
        ),
        context_plan: pb.context_plan,
        contract_id,
        expires_at,
        timestamp: pb
            .timestamp
            .map(proto_to_datetime)
            .unwrap_or_else(chrono::Utc::now),
        signature: pb.signature,
    })
}

// ---------------------------------------------------------------------------
// ContextRequestMsg
// ---------------------------------------------------------------------------

pub fn context_request_to_proto(msg: ContextRequestMsg) -> proto::ContextRequest {
    proto::ContextRequest {
        from: Some(agent_id_to_proto(msg.from)),
        to: Some(agent_id_to_proto(msg.to)),
        task_id: msg.task_id.to_string(),
        current_confidence: msg.current_confidence,
        requested_chunk_indices: msg.requested_chunk_indices,
        timestamp: Some(now_proto()),
    }
}

pub fn proto_to_context_request(
    pb: proto::ContextRequest,
) -> Result<ContextRequestMsg, CodecError> {
    let task_id = Uuid::parse_str(&pb.task_id)
        .map_err(|_| CodecError::InvalidUuid(pb.task_id.clone()))?;

    Ok(ContextRequestMsg {
        from: require_agent_id(pb.from, "from")?,
        to: require_agent_id(pb.to, "to")?,
        task_id,
        current_confidence: pb.current_confidence,
        requested_chunk_indices: pb.requested_chunk_indices,
    })
}

// ---------------------------------------------------------------------------
// TaskSubmitMsg
// ---------------------------------------------------------------------------

pub fn task_submit_to_proto(msg: TaskSubmitMsg) -> proto::TaskSubmit {
    proto::TaskSubmit {
        from: Some(agent_id_to_proto(msg.from)),
        to: Some(agent_id_to_proto(msg.to)),
        task_id: msg.task_id.to_string(),
        task_type: task_type_to_i32(msg.task_type),
        payload: msg.payload,
        context: Some(context_diff_to_proto(msg.context)),
        contract_id: msg.contract_id.to_string(),
        timestamp: Some(now_proto()),
    }
}

pub fn proto_to_task_submit(pb: proto::TaskSubmit) -> Result<TaskSubmitMsg, CodecError> {
    let task_id = Uuid::parse_str(&pb.task_id)
        .map_err(|_| CodecError::InvalidUuid(pb.task_id.clone()))?;
    let contract_id = Uuid::parse_str(&pb.contract_id)
        .map_err(|_| CodecError::InvalidUuid(pb.contract_id.clone()))?;
    let context = proto_to_context_diff(
        pb.context.ok_or(CodecError::MissingField("context"))?,
    );

    Ok(TaskSubmitMsg {
        from: require_agent_id(pb.from, "from")?,
        to: require_agent_id(pb.to, "to")?,
        task_id,
        task_type: proto_to_task_type(pb.task_type)?,
        payload: pb.payload,
        context,
        contract_id,
    })
}

// ---------------------------------------------------------------------------
// DomainTaskResult (atp_types::TaskResult)
// ---------------------------------------------------------------------------

pub fn task_result_to_proto(tr: DomainTaskResult) -> proto::TaskResult {
    proto::TaskResult {
        from: Some(agent_id_to_proto(tr.from)),
        task_id: tr.task_id.to_string(),
        quality_self_report: tr.quality_self_report,
        payload: tr.payload,
        elapsed_ms: tr.elapsed.as_millis() as u64,
        actual_cost: tr.actual_cost,
        timestamp: Some(now_proto()),
    }
}

pub fn proto_to_task_result(pb: proto::TaskResult) -> Result<DomainTaskResult, CodecError> {
    let task_id = Uuid::parse_str(&pb.task_id)
        .map_err(|_| CodecError::InvalidUuid(pb.task_id.clone()))?;

    Ok(DomainTaskResult {
        task_id,
        from: require_agent_id(pb.from, "from")?,
        quality_self_report: pb.quality_self_report,
        payload: pb.payload,
        elapsed: Duration::from_millis(pb.elapsed_ms),
        actual_cost: pb.actual_cost,
    })
}

// ---------------------------------------------------------------------------
// InteractionProofMsg
// ---------------------------------------------------------------------------

pub fn interaction_proof_to_proto(msg: InteractionProofMsg) -> proto::InteractionProof {
    proto::InteractionProof {
        evaluator: Some(agent_id_to_proto(msg.evaluator)),
        subject: Some(agent_id_to_proto(msg.subject)),
        task_id: msg.task_id.to_string(),
        task_type: task_type_to_i32(msg.task_type),
        quality_score: msg.quality_score,
        latency_ms: msg.latency_ms,
        cost: msg.cost,
        timestamp: Some(datetime_to_proto(msg.timestamp)),
        signature: msg.signature,
    }
}

pub fn proto_to_interaction_proof(
    pb: proto::InteractionProof,
) -> Result<InteractionProofMsg, CodecError> {
    let task_id = Uuid::parse_str(&pb.task_id)
        .map_err(|_| CodecError::InvalidUuid(pb.task_id.clone()))?;

    Ok(InteractionProofMsg {
        evaluator: require_agent_id(pb.evaluator, "evaluator")?,
        subject: require_agent_id(pb.subject, "subject")?,
        task_id,
        task_type: proto_to_task_type(pb.task_type)?,
        quality_score: pb.quality_score,
        latency_ms: pb.latency_ms,
        cost: pb.cost,
        timestamp: pb
            .timestamp
            .map(proto_to_datetime)
            .unwrap_or_else(chrono::Utc::now),
        signature: pb.signature,
    })
}

// ---------------------------------------------------------------------------
// HeartbeatMsg
// ---------------------------------------------------------------------------

pub fn heartbeat_to_proto(msg: HeartbeatMsg) -> proto::Heartbeat {
    proto::Heartbeat {
        from: Some(agent_id_to_proto(msg.from)),
        sequence: msg.sequence,
        queue_depth: msg.queue_depth,
        load_factor: msg.load_factor,
        timestamp: Some(now_proto()),
    }
}

pub fn proto_to_heartbeat(pb: proto::Heartbeat) -> Result<HeartbeatMsg, CodecError> {
    Ok(HeartbeatMsg {
        from: require_agent_id(pb.from, "from")?,
        sequence: pb.sequence,
        queue_depth: pb.queue_depth,
        load_factor: pb.load_factor,
    })
}

// ---------------------------------------------------------------------------
// BackpressureMsg
// ---------------------------------------------------------------------------

pub fn backpressure_to_proto(msg: BackpressureMsg) -> proto::Backpressure {
    proto::Backpressure {
        from: Some(agent_id_to_proto(msg.from)),
        queue_depth: msg.queue_depth,
        recommended_rate: msg.recommended_rate,
        estimated_drain_ms: msg.estimated_drain_ms,
        timestamp: Some(now_proto()),
    }
}

pub fn proto_to_backpressure(pb: proto::Backpressure) -> Result<BackpressureMsg, CodecError> {
    Ok(BackpressureMsg {
        from: require_agent_id(pb.from, "from")?,
        queue_depth: pb.queue_depth,
        recommended_rate: pb.recommended_rate,
        estimated_drain_ms: pb.estimated_drain_ms,
    })
}

// ---------------------------------------------------------------------------
// CircuitBreakMsg
// ---------------------------------------------------------------------------

pub fn circuit_break_to_proto(msg: CircuitBreakMsg) -> proto::CircuitBreak {
    proto::CircuitBreak {
        from: Some(agent_id_to_proto(msg.from)),
        target: Some(agent_id_to_proto(msg.target)),
        state: circuit_state_to_proto(msg.state) as i32,
        failure_count: msg.failure_count,
        timestamp: Some(now_proto()),
    }
}

pub fn proto_to_circuit_break(pb: proto::CircuitBreak) -> Result<CircuitBreakMsg, CodecError> {
    Ok(CircuitBreakMsg {
        from: require_agent_id(pb.from, "from")?,
        target: require_agent_id(pb.target, "target")?,
        state: proto_to_circuit_state(pb.state)?,
        failure_count: pb.failure_count,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_agent_id() {
        let id = AgentId::new();
        let pb = agent_id_to_proto(id);
        let back = proto_to_agent_id(pb).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn roundtrip_task_type() {
        for tt in TaskType::all() {
            let i = task_type_to_i32(*tt);
            let back = task_type_from_i32(i).unwrap();
            assert_eq!(*tt, back);
        }
    }

    #[test]
    fn invalid_task_type() {
        assert!(task_type_from_i32(0).is_err()); // Unspecified
        assert!(task_type_from_i32(99).is_err());
    }

    #[test]
    fn roundtrip_qos() {
        let qos = QoSConstraints::default();
        let pb = qos_to_proto(qos.clone());
        let back = proto_to_qos(pb);
        assert_eq!(qos.min_quality, back.min_quality);
        assert_eq!(qos.max_cost, back.max_cost);
        assert_eq!(qos.min_trust, back.min_trust);
    }

    #[test]
    fn roundtrip_capability() {
        let cap = Capability {
            task_type: TaskType::Analysis,
            estimated_quality: 0.9,
            estimated_latency: Duration::from_millis(250),
            cost_per_task: 0.5,
        };
        let pb = capability_to_proto(cap.clone());
        let back = proto_to_capability(pb).unwrap();
        assert_eq!(cap.task_type, back.task_type);
        assert!((cap.estimated_quality - back.estimated_quality).abs() < 1e-9);
    }

    #[test]
    fn roundtrip_context_diff() {
        let diff = ContextDiff {
            base_hash: [42u8; 32],
            chunks: vec![ContextChunk {
                index: 0,
                data: vec![1, 2, 3],
                relevance_score: 0.95,
            }],
            confidence: 0.8,
            original_size: 1000,
            compressed_size: 100,
        };
        let pb = context_diff_to_proto(diff.clone());
        let back = proto_to_context_diff(pb);
        assert_eq!(diff.base_hash, back.base_hash);
        assert_eq!(diff.chunks.len(), back.chunks.len());
        assert_eq!(diff.original_size, back.original_size);
    }

    #[test]
    fn roundtrip_routing_pattern() {
        let patterns = [
            RoutingPattern::DraftRefine,
            RoutingPattern::ParallelMerge,
            RoutingPattern::Cascade,
            RoutingPattern::Ensemble,
            RoutingPattern::Pipeline,
        ];
        for rp in patterns {
            let i = routing_pattern_to_i32(rp);
            let back = proto_to_routing_pattern(i).unwrap();
            assert_eq!(rp, back);
        }
    }

    #[test]
    fn roundtrip_circuit_state() {
        let states = [
            CircuitState::Closed,
            CircuitState::Open,
            CircuitState::HalfOpen,
        ];
        for cs in states {
            let i = circuit_state_to_proto(cs) as i32;
            let back = proto_to_circuit_state(i).unwrap();
            assert_eq!(cs, back);
        }
    }

    #[test]
    fn roundtrip_heartbeat_msg() {
        let msg = HeartbeatMsg {
            from: AgentId::new(),
            sequence: 42,
            queue_depth: 10,
            load_factor: 0.75,
        };
        let pb = heartbeat_to_proto(msg.clone());
        let back = proto_to_heartbeat(pb).unwrap();
        assert_eq!(msg.from, back.from);
        assert_eq!(msg.sequence, back.sequence);
        assert_eq!(msg.queue_depth, back.queue_depth);
    }

    #[test]
    fn roundtrip_backpressure_msg() {
        let msg = BackpressureMsg {
            from: AgentId::new(),
            queue_depth: 50,
            recommended_rate: 0.5,
            estimated_drain_ms: 2000,
        };
        let pb = backpressure_to_proto(msg.clone());
        let back = proto_to_backpressure(pb).unwrap();
        assert_eq!(msg.from, back.from);
        assert_eq!(msg.queue_depth, back.queue_depth);
        assert!((msg.recommended_rate - back.recommended_rate).abs() < 1e-9);
    }

    #[test]
    fn roundtrip_circuit_break_msg() {
        let msg = CircuitBreakMsg {
            from: AgentId::new(),
            target: AgentId::new(),
            state: CircuitState::Open,
            failure_count: 5,
        };
        let pb = circuit_break_to_proto(msg.clone());
        let back = proto_to_circuit_break(pb).unwrap();
        assert_eq!(msg.from, back.from);
        assert_eq!(msg.target, back.target);
        assert_eq!(msg.state, back.state);
        assert_eq!(msg.failure_count, back.failure_count);
    }

    #[test]
    fn roundtrip_task_result() {
        let tr = DomainTaskResult {
            task_id: Uuid::new_v4(),
            from: AgentId::new(),
            quality_self_report: 0.85,
            payload: vec![10, 20, 30],
            elapsed: Duration::from_millis(500),
            actual_cost: 0.3,
        };
        let pb = task_result_to_proto(tr.clone());
        let back = proto_to_task_result(pb).unwrap();
        assert_eq!(tr.task_id, back.task_id);
        assert_eq!(tr.from, back.from);
        assert!((tr.quality_self_report - back.quality_self_report).abs() < 1e-9);
        assert_eq!(tr.elapsed, back.elapsed);
    }

    #[test]
    fn missing_field_produces_error() {
        let pb = proto::CapabilityProbe {
            from: None, // missing required field
            task_type: 1,
            qos: Some(proto::QoSConstraints::default()),
            context_embedding: None,
            nonce: 1,
            timestamp: None,
            signature: vec![],
        };
        let result = proto_to_probe(pb);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_uuid_produces_error() {
        let pb = proto::AgentId {
            uuid: "not-a-uuid".to_string(),
        };
        assert!(proto_to_agent_id(pb).is_err());
    }

    #[test]
    fn codec_error_to_status() {
        let err = CodecError::MissingField("test_field");
        let status: tonic::Status = err.into();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(status.message().contains("test_field"));
    }

    #[test]
    fn roundtrip_probe_msg() {
        let msg = CapabilityProbeMsg {
            from: AgentId::new(),
            task_type: TaskType::CodeGeneration,
            qos: QoSConstraints::default(),
            context_embedding: Some(ContextEmbedding::new(vec![1.0, 2.0, 3.0])),
            nonce: 99,
            timestamp: chrono::Utc::now(),
            signature: vec![1, 2, 3, 4],
        };
        let pb = probe_to_proto(msg.clone());
        let back = proto_to_probe(pb).unwrap();
        assert_eq!(msg.from, back.from);
        assert_eq!(msg.task_type, back.task_type);
        assert_eq!(msg.nonce, back.nonce);
        assert!(back.context_embedding.is_some());
        assert_eq!(
            msg.context_embedding.unwrap().values,
            back.context_embedding.unwrap().values,
        );
    }

    #[test]
    fn roundtrip_interaction_proof() {
        let msg = InteractionProofMsg {
            evaluator: AgentId::new(),
            subject: AgentId::new(),
            task_id: Uuid::new_v4(),
            task_type: TaskType::DataProcessing,
            quality_score: 0.92,
            latency_ms: 150,
            cost: 0.2,
            timestamp: chrono::Utc::now(),
            signature: vec![5, 6, 7],
        };
        let pb = interaction_proof_to_proto(msg.clone());
        let back = proto_to_interaction_proof(pb).unwrap();
        assert_eq!(msg.evaluator, back.evaluator);
        assert_eq!(msg.subject, back.subject);
        assert_eq!(msg.task_id, back.task_id);
        assert_eq!(msg.task_type, back.task_type);
    }
}
