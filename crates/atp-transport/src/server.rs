//! Tonic gRPC server implementing the `AtpService` trait.
//!
//! The [`AtpServer`] struct holds shared state and protocol-layer handlers,
//! delegating each RPC to the appropriate handler. It converts between proto
//! wire types and domain types using the conversions in [`crate::codec`].

use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::info;
use uuid::Uuid;

use atp_proto::atp::v1 as proto;
use atp_proto::atp::v1::atp_service_server::AtpService;
use atp_types::{
    AgentId, BackpressureMsg, CapabilityOfferMsg, CapabilityProbeMsg, CircuitBreakMsg,
    ContractAcceptMsg, ContextRequestMsg, HeartbeatMsg, InteractionProofMsg, TaskSubmitMsg,
    TaskResult as DomainTaskResult,
};

use crate::codec;

// ---------------------------------------------------------------------------
// Handler traits -- allow protocol layers to plug in
// ---------------------------------------------------------------------------

/// Handler for Layer 2 capability handshake operations.
#[async_trait::async_trait]
pub trait HandshakeHandler: Send + Sync + 'static {
    async fn handle_probe(
        &self,
        probe: CapabilityProbeMsg,
    ) -> Result<CapabilityOfferMsg, Status>;

    async fn handle_accept_contract(
        &self,
        accept: ContractAcceptMsg,
    ) -> Result<(String, bool), Status>;
}

/// Handler for task lifecycle operations.
#[async_trait::async_trait]
pub trait TaskHandler: Send + Sync + 'static {
    async fn handle_submit_task(
        &self,
        submit: TaskSubmitMsg,
    ) -> Result<(String, bool, String), Status>;

    async fn handle_stream_results(
        &self,
        task_id: Uuid,
    ) -> Result<Vec<DomainTaskResult>, Status>;
}

/// Handler for Layer 3 context operations.
#[async_trait::async_trait]
pub trait ContextHandler: Send + Sync + 'static {
    async fn handle_context_request(
        &self,
        req: ContextRequestMsg,
    ) -> Result<(String, Vec<atp_types::ContextChunk>), Status>;
}

/// Handler for Layer 4 routing operations.
#[async_trait::async_trait]
pub trait RoutingHandler: Send + Sync + 'static {
    async fn handle_route_query(
        &self,
        task_type: atp_types::TaskType,
        qos: atp_types::QoSConstraints,
        preferred_pattern: Option<atp_types::RoutingPattern>,
        max_routes: u32,
    ) -> Result<Vec<atp_types::Route>, Status>;
}

/// Handler for Layer 5 fault tolerance operations.
#[async_trait::async_trait]
pub trait FaultHandler: Send + Sync + 'static {
    async fn handle_heartbeat(&self, msg: HeartbeatMsg) -> Result<u64, Status>;
    async fn handle_backpressure(&self, msg: BackpressureMsg) -> Result<bool, Status>;
    async fn handle_circuit_break(&self, msg: CircuitBreakMsg) -> Result<bool, Status>;
}

/// Handler for Layer 1 trust/identity operations.
#[async_trait::async_trait]
pub trait TrustHandler: Send + Sync + 'static {
    async fn handle_interaction_proof(
        &self,
        proof: InteractionProofMsg,
    ) -> Result<bool, Status>;
}

// ---------------------------------------------------------------------------
// Default (stub) handlers -- return reasonable defaults
// ---------------------------------------------------------------------------

/// Default handler that logs and returns stub responses.
/// Used when protocol layers are not yet wired in.
#[derive(Debug, Clone)]
pub struct DefaultHandler;

#[async_trait::async_trait]
impl HandshakeHandler for DefaultHandler {
    async fn handle_probe(
        &self,
        probe: CapabilityProbeMsg,
    ) -> Result<CapabilityOfferMsg, Status> {
        info!(from = %probe.from, task_type = %probe.task_type, "probe received (default handler)");
        Ok(CapabilityOfferMsg {
            from: AgentId::new(),
            in_reply_to: probe.nonce,
            capability: atp_types::Capability {
                task_type: probe.task_type,
                estimated_quality: 0.8,
                estimated_latency: std::time::Duration::from_millis(100),
                cost_per_task: 0.1,
            },
            trust_score: 0.75,
            trust_proof: vec![],
            ttl: std::time::Duration::from_secs(60),
            timestamp: chrono::Utc::now(),
            signature: vec![],
        })
    }

    async fn handle_accept_contract(
        &self,
        accept: ContractAcceptMsg,
    ) -> Result<(String, bool), Status> {
        info!(contract_id = %accept.contract_id, "contract accept received (default handler)");
        Ok((accept.contract_id.to_string(), true))
    }
}

#[async_trait::async_trait]
impl TaskHandler for DefaultHandler {
    async fn handle_submit_task(
        &self,
        submit: TaskSubmitMsg,
    ) -> Result<(String, bool, String), Status> {
        info!(task_id = %submit.task_id, "task submit received (default handler)");
        Ok((submit.task_id.to_string(), true, String::new()))
    }

    async fn handle_stream_results(
        &self,
        task_id: Uuid,
    ) -> Result<Vec<DomainTaskResult>, Status> {
        info!(task_id = %task_id, "stream results requested (default handler)");
        Ok(vec![])
    }
}

#[async_trait::async_trait]
impl ContextHandler for DefaultHandler {
    async fn handle_context_request(
        &self,
        req: ContextRequestMsg,
    ) -> Result<(String, Vec<atp_types::ContextChunk>), Status> {
        info!(task_id = %req.task_id, "context request received (default handler)");
        Ok((req.task_id.to_string(), vec![]))
    }
}

#[async_trait::async_trait]
impl RoutingHandler for DefaultHandler {
    async fn handle_route_query(
        &self,
        task_type: atp_types::TaskType,
        _qos: atp_types::QoSConstraints,
        _preferred_pattern: Option<atp_types::RoutingPattern>,
        _max_routes: u32,
    ) -> Result<Vec<atp_types::Route>, Status> {
        info!(task_type = %task_type, "route query received (default handler)");
        Ok(vec![])
    }
}

#[async_trait::async_trait]
impl FaultHandler for DefaultHandler {
    async fn handle_heartbeat(&self, msg: HeartbeatMsg) -> Result<u64, Status> {
        Ok(msg.sequence)
    }

    async fn handle_backpressure(&self, _msg: BackpressureMsg) -> Result<bool, Status> {
        Ok(true)
    }

    async fn handle_circuit_break(&self, _msg: CircuitBreakMsg) -> Result<bool, Status> {
        Ok(true)
    }
}

#[async_trait::async_trait]
impl TrustHandler for DefaultHandler {
    async fn handle_interaction_proof(
        &self,
        proof: InteractionProofMsg,
    ) -> Result<bool, Status> {
        info!(
            evaluator = %proof.evaluator,
            subject = %proof.subject,
            "interaction proof received (default handler)"
        );
        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// AtpServer
// ---------------------------------------------------------------------------

/// The gRPC server implementing all ATP service RPCs.
///
/// Each protocol layer is represented by a handler trait object. By default,
/// stub handlers are used that log and return reasonable defaults. Wire in
/// real handlers via [`AtpServerBuilder`].
pub struct AtpServer {
    handshake: Arc<dyn HandshakeHandler>,
    task: Arc<dyn TaskHandler>,
    context: Arc<dyn ContextHandler>,
    routing: Arc<dyn RoutingHandler>,
    fault: Arc<dyn FaultHandler>,
    trust: Arc<dyn TrustHandler>,
}

impl AtpServer {
    /// Create a new `AtpServer` with all default (stub) handlers.
    pub fn new() -> Self {
        let default = Arc::new(DefaultHandler);
        Self {
            handshake: default.clone(),
            task: default.clone(),
            context: default.clone(),
            routing: default.clone(),
            fault: default.clone(),
            trust: default,
        }
    }

    /// Return a builder for configuring handlers.
    pub fn builder() -> AtpServerBuilder {
        AtpServerBuilder::new()
    }

    /// Wrap this server in the tonic-generated `AtpServiceServer`.
    pub fn into_service(self) -> proto::atp_service_server::AtpServiceServer<Self> {
        proto::atp_service_server::AtpServiceServer::new(self)
    }
}

impl Default for AtpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for configuring [`AtpServer`] with custom handlers.
pub struct AtpServerBuilder {
    handshake: Option<Arc<dyn HandshakeHandler>>,
    task: Option<Arc<dyn TaskHandler>>,
    context: Option<Arc<dyn ContextHandler>>,
    routing: Option<Arc<dyn RoutingHandler>>,
    fault: Option<Arc<dyn FaultHandler>>,
    trust: Option<Arc<dyn TrustHandler>>,
}

impl AtpServerBuilder {
    pub fn new() -> Self {
        Self {
            handshake: None,
            task: None,
            context: None,
            routing: None,
            fault: None,
            trust: None,
        }
    }

    pub fn handshake(mut self, h: impl HandshakeHandler) -> Self {
        self.handshake = Some(Arc::new(h));
        self
    }

    pub fn task(mut self, h: impl TaskHandler) -> Self {
        self.task = Some(Arc::new(h));
        self
    }

    pub fn context(mut self, h: impl ContextHandler) -> Self {
        self.context = Some(Arc::new(h));
        self
    }

    pub fn routing(mut self, h: impl RoutingHandler) -> Self {
        self.routing = Some(Arc::new(h));
        self
    }

    pub fn fault(mut self, h: impl FaultHandler) -> Self {
        self.fault = Some(Arc::new(h));
        self
    }

    pub fn trust(mut self, h: impl TrustHandler) -> Self {
        self.trust = Some(Arc::new(h));
        self
    }

    pub fn build(self) -> AtpServer {
        let default = Arc::new(DefaultHandler);
        AtpServer {
            handshake: self.handshake.unwrap_or_else(|| default.clone()),
            task: self.task.unwrap_or_else(|| default.clone()),
            context: self.context.unwrap_or_else(|| default.clone()),
            routing: self.routing.unwrap_or_else(|| default.clone()),
            fault: self.fault.unwrap_or_else(|| default.clone()),
            trust: self.trust.unwrap_or_else(|| default.clone()),
        }
    }
}

impl Default for AtpServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// AtpService trait implementation
// ---------------------------------------------------------------------------

type StreamResultsStream =
    Pin<Box<dyn tokio_stream::Stream<Item = Result<proto::TaskResult, Status>> + Send>>;

#[async_trait::async_trait]
impl AtpService for AtpServer {
    // -- Layer 2: Capability Handshake --

    async fn probe(
        &self,
        request: Request<proto::CapabilityProbe>,
    ) -> Result<Response<proto::CapabilityOffer>, Status> {
        let pb = request.into_inner();
        let probe_msg = codec::proto_to_probe(pb)?;

        let offer_msg = self.handshake.handle_probe(probe_msg).await?;
        let pb_offer = codec::offer_to_proto(offer_msg);
        Ok(Response::new(pb_offer))
    }

    async fn accept_contract(
        &self,
        request: Request<proto::ContractAccept>,
    ) -> Result<Response<proto::ContractAck>, Status> {
        let pb = request.into_inner();
        let accept_msg = codec::proto_to_contract_accept(pb)?;

        let (contract_id, accepted) = self.handshake.handle_accept_contract(accept_msg).await?;
        Ok(Response::new(proto::ContractAck {
            contract_id,
            accepted,
        }))
    }

    // -- Task lifecycle --

    async fn submit_task(
        &self,
        request: Request<proto::TaskSubmit>,
    ) -> Result<Response<proto::TaskAck>, Status> {
        let pb = request.into_inner();
        let submit_msg = codec::proto_to_task_submit(pb)?;

        let (task_id, accepted, reason) = self.task.handle_submit_task(submit_msg).await?;
        Ok(Response::new(proto::TaskAck {
            task_id,
            accepted,
            reason,
        }))
    }

    type StreamResultsStream = StreamResultsStream;

    async fn stream_results(
        &self,
        request: Request<proto::TaskQuery>,
    ) -> Result<Response<Self::StreamResultsStream>, Status> {
        let pb = request.into_inner();
        let task_id = Uuid::parse_str(&pb.task_id)
            .map_err(|_| Status::invalid_argument(format!("invalid task_id: {}", pb.task_id)))?;

        let results = self.task.handle_stream_results(task_id).await?;

        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            for result in results {
                let pb_result = codec::task_result_to_proto(result);
                if tx.send(Ok(pb_result)).await.is_err() {
                    break;
                }
            }
        });

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::StreamResultsStream))
    }

    // -- Layer 3: Context --

    async fn request_context(
        &self,
        request: Request<proto::ContextRequest>,
    ) -> Result<Response<proto::ContextResponse>, Status> {
        let pb = request.into_inner();
        let ctx_msg = codec::proto_to_context_request(pb)?;

        let (task_id, chunks) = self.context.handle_context_request(ctx_msg).await?;
        Ok(Response::new(proto::ContextResponse {
            task_id,
            chunks: chunks.into_iter().map(codec::context_chunk_to_proto).collect(),
        }))
    }

    // -- Layer 4: Routing --

    async fn query_route(
        &self,
        request: Request<proto::RouteQuery>,
    ) -> Result<Response<proto::RouteResponse>, Status> {
        let pb = request.into_inner();
        let task_type = codec::proto_to_task_type(pb.task_type)?;
        let qos = codec::proto_to_qos(
            pb.qos.ok_or_else(|| Status::invalid_argument("missing qos field"))?,
        );
        let preferred = codec::proto_to_routing_pattern(pb.preferred_pattern).ok();

        let routes = self
            .routing
            .handle_route_query(task_type, qos, preferred, pb.max_routes)
            .await?;

        Ok(Response::new(proto::RouteResponse {
            routes: routes.into_iter().map(codec::route_to_proto).collect(),
        }))
    }

    // -- Layer 5: Fault tolerance --

    async fn send_heartbeat(
        &self,
        request: Request<proto::Heartbeat>,
    ) -> Result<Response<proto::HeartbeatAck>, Status> {
        let pb = request.into_inner();
        let hb_msg = codec::proto_to_heartbeat(pb)?;

        let sequence = self.fault.handle_heartbeat(hb_msg).await?;
        Ok(Response::new(proto::HeartbeatAck { sequence }))
    }

    async fn report_backpressure(
        &self,
        request: Request<proto::Backpressure>,
    ) -> Result<Response<proto::BackpressureAck>, Status> {
        let pb = request.into_inner();
        let bp_msg = codec::proto_to_backpressure(pb)?;

        let acknowledged = self.fault.handle_backpressure(bp_msg).await?;
        Ok(Response::new(proto::BackpressureAck { acknowledged }))
    }

    async fn report_circuit_break(
        &self,
        request: Request<proto::CircuitBreak>,
    ) -> Result<Response<proto::CircuitBreakAck>, Status> {
        let pb = request.into_inner();
        let cb_msg = codec::proto_to_circuit_break(pb)?;

        let acknowledged = self.fault.handle_circuit_break(cb_msg).await?;
        Ok(Response::new(proto::CircuitBreakAck { acknowledged }))
    }

    // -- Layer 1: Trust --

    async fn submit_interaction_proof(
        &self,
        request: Request<proto::InteractionProof>,
    ) -> Result<Response<proto::ProofAck>, Status> {
        let pb = request.into_inner();
        let proof_msg = codec::proto_to_interaction_proof(pb)?;

        let accepted = self.trust.handle_interaction_proof(proof_msg).await?;
        Ok(Response::new(proto::ProofAck { accepted }))
    }
}

// ---------------------------------------------------------------------------
// Convenience: start the server
// ---------------------------------------------------------------------------

/// Start the ATP gRPC server on the given address.
///
/// This is a convenience function that creates a tonic server and serves
/// the `AtpServer` on the specified socket address.
pub async fn serve(
    server: AtpServer,
    addr: std::net::SocketAddr,
) -> Result<(), tonic::transport::Error> {
    info!(%addr, "starting ATP gRPC server");
    tonic::transport::Server::builder()
        .add_service(server.into_service())
        .serve(addr)
        .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_server_builds() {
        let _server = AtpServer::new();
    }

    #[test]
    fn builder_with_defaults() {
        let _server = AtpServer::builder().build();
    }

    #[test]
    fn builder_with_custom_handler() {
        let _server = AtpServer::builder()
            .trust(DefaultHandler)
            .fault(DefaultHandler)
            .build();
    }

    #[test]
    fn into_service_compiles() {
        let server = AtpServer::new();
        let _svc = server.into_service();
    }

    #[tokio::test]
    async fn probe_default_handler() {
        let server = AtpServer::new();
        let probe = proto::CapabilityProbe {
            from: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            task_type: proto::TaskType::Analysis as i32,
            qos: Some(proto::QoSConstraints {
                min_quality: 0.5,
                max_latency_ms: 1000,
                max_cost: 1.0,
                min_trust: 0.3,
            }),
            context_embedding: None,
            nonce: 42,
            timestamp: Some(codec::now_proto()),
            signature: vec![],
        };

        let response = server.probe(Request::new(probe)).await;
        assert!(response.is_ok());
        let offer = response.unwrap().into_inner();
        assert_eq!(offer.in_reply_to, 42);
    }

    #[tokio::test]
    async fn heartbeat_default_handler() {
        let server = AtpServer::new();
        let hb = proto::Heartbeat {
            from: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            sequence: 7,
            queue_depth: 3,
            load_factor: 0.5,
            timestamp: Some(codec::now_proto()),
        };

        let response = server.send_heartbeat(Request::new(hb)).await;
        assert!(response.is_ok());
        assert_eq!(response.unwrap().into_inner().sequence, 7);
    }

    #[tokio::test]
    async fn submit_task_default_handler() {
        let server = AtpServer::new();
        let task_id = Uuid::new_v4();
        let submit = proto::TaskSubmit {
            from: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            to: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            task_id: task_id.to_string(),
            task_type: proto::TaskType::CodeGeneration as i32,
            payload: vec![1, 2, 3],
            context: Some(proto::ContextDiff {
                base_hash: vec![0u8; 32],
                chunks: vec![],
                confidence: 0.9,
                original_size: 100,
                compressed_size: 10,
            }),
            contract_id: Uuid::new_v4().to_string(),
            timestamp: Some(codec::now_proto()),
        };

        let response = server.submit_task(Request::new(submit)).await;
        assert!(response.is_ok());
        let ack = response.unwrap().into_inner();
        assert!(ack.accepted);
        assert_eq!(ack.task_id, task_id.to_string());
    }

    #[tokio::test]
    async fn missing_from_field_returns_invalid_argument() {
        let server = AtpServer::new();
        let probe = proto::CapabilityProbe {
            from: None,
            task_type: proto::TaskType::Analysis as i32,
            qos: Some(proto::QoSConstraints::default()),
            context_embedding: None,
            nonce: 1,
            timestamp: None,
            signature: vec![],
        };

        let response = server.probe(Request::new(probe)).await;
        assert!(response.is_err());
        let status = response.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn accept_contract_default_handler() {
        let server = AtpServer::new();
        let contract_id = Uuid::new_v4();
        let accept = proto::ContractAccept {
            from: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            to: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            agreed_qos: Some(proto::QoSConstraints {
                min_quality: 0.7,
                max_latency_ms: 500,
                max_cost: 0.5,
                min_trust: 0.5,
            }),
            context_plan: "full".to_string(),
            contract_id: contract_id.to_string(),
            expires_ms: chrono::Utc::now().timestamp_millis() as u64 + 60_000,
            timestamp: Some(codec::now_proto()),
            signature: vec![],
        };

        let response = server.accept_contract(Request::new(accept)).await;
        assert!(response.is_ok());
        let ack = response.unwrap().into_inner();
        assert!(ack.accepted);
        assert_eq!(ack.contract_id, contract_id.to_string());
    }

    #[tokio::test]
    async fn backpressure_default_handler() {
        let server = AtpServer::new();
        let bp = proto::Backpressure {
            from: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            queue_depth: 50,
            recommended_rate: 0.5,
            estimated_drain_ms: 2000,
            timestamp: Some(codec::now_proto()),
        };

        let response = server.report_backpressure(Request::new(bp)).await;
        assert!(response.is_ok());
        assert!(response.unwrap().into_inner().acknowledged);
    }

    #[tokio::test]
    async fn circuit_break_default_handler() {
        let server = AtpServer::new();
        let cb = proto::CircuitBreak {
            from: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            target: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            state: proto::CircuitState::Open as i32,
            failure_count: 5,
            timestamp: Some(codec::now_proto()),
        };

        let response = server.report_circuit_break(Request::new(cb)).await;
        assert!(response.is_ok());
        assert!(response.unwrap().into_inner().acknowledged);
    }

    #[tokio::test]
    async fn interaction_proof_default_handler() {
        let server = AtpServer::new();
        let proof = proto::InteractionProof {
            evaluator: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            subject: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            task_id: Uuid::new_v4().to_string(),
            task_type: proto::TaskType::Analysis as i32,
            quality_score: 0.9,
            latency_ms: 100,
            cost: 0.2,
            timestamp: Some(codec::now_proto()),
            signature: vec![1, 2, 3],
        };

        let response = server.submit_interaction_proof(Request::new(proof)).await;
        assert!(response.is_ok());
        assert!(response.unwrap().into_inner().accepted);
    }

    #[tokio::test]
    async fn stream_results_default_handler() {
        let server = AtpServer::new();
        let query = proto::TaskQuery {
            task_id: Uuid::new_v4().to_string(),
        };

        let response = server.stream_results(Request::new(query)).await;
        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn context_request_default_handler() {
        let server = AtpServer::new();
        let req = proto::ContextRequest {
            from: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            to: Some(proto::AgentId {
                uuid: Uuid::new_v4().to_string(),
            }),
            task_id: Uuid::new_v4().to_string(),
            current_confidence: 0.6,
            requested_chunk_indices: vec![0, 1, 2],
            timestamp: Some(codec::now_proto()),
        };

        let response = server.request_context(Request::new(req)).await;
        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn query_route_default_handler() {
        let server = AtpServer::new();
        let query = proto::RouteQuery {
            task_type: proto::TaskType::CodeGeneration as i32,
            qos: Some(proto::QoSConstraints {
                min_quality: 0.5,
                max_latency_ms: 1000,
                max_cost: 1.0,
                min_trust: 0.3,
            }),
            preferred_pattern: proto::RoutingPattern::DraftRefine as i32,
            max_routes: 5,
        };

        let response = server.query_route(Request::new(query)).await;
        assert!(response.is_ok());
    }
}
