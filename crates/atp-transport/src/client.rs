//! Tonic gRPC client wrapper for the ATP service.
//!
//! The [`AtpClient`] struct wraps the tonic-generated
//! [`AtpServiceClient`](atp_proto::atp::v1::atp_service_client::AtpServiceClient)
//! and provides typed methods that accept and return domain types from
//! `atp_types`, handling all proto conversions internally.

use tonic::transport::Channel;
use uuid::Uuid;

use atp_proto::atp::v1 as proto;
use atp_proto::atp::v1::atp_service_client::AtpServiceClient;
use atp_types::{
    BackpressureMsg, CapabilityOfferMsg, CapabilityProbeMsg, CircuitBreakMsg,
    ContractAcceptMsg, ContextChunk, ContextRequestMsg, HeartbeatMsg, InteractionProofMsg,
    QoSConstraints, Route, RoutingPattern, TaskResult as DomainTaskResult, TaskSubmitMsg, TaskType,
};

use crate::codec::{self, CodecError};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from the ATP client.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    #[error("gRPC status: {0}")]
    Status(#[from] tonic::Status),

    #[error("codec error: {0}")]
    Codec(#[from] CodecError),
}

// ---------------------------------------------------------------------------
// AtpClient
// ---------------------------------------------------------------------------

/// High-level ATP gRPC client.
///
/// Wraps the generated tonic client and provides typed methods that take
/// domain types and return domain types, performing proto conversion
/// transparently.
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use atp_transport::client::AtpClient;
///
/// let client = AtpClient::connect("http://[::1]:50051").await?;
/// // Use typed methods...
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct AtpClient {
    inner: AtpServiceClient<Channel>,
}

impl AtpClient {
    /// Connect to an ATP gRPC server at the given URI.
    pub async fn connect(dst: &str) -> Result<Self, ClientError> {
        let client = AtpServiceClient::connect(dst.to_string()).await?;
        Ok(Self { inner: client })
    }

    /// Create a client from an existing tonic `Channel`.
    pub fn from_channel(channel: Channel) -> Self {
        Self {
            inner: AtpServiceClient::new(channel),
        }
    }

    /// Return a reference to the underlying tonic client for advanced use.
    pub fn inner(&self) -> &AtpServiceClient<Channel> {
        &self.inner
    }

    /// Return a mutable reference to the underlying tonic client.
    pub fn inner_mut(&mut self) -> &mut AtpServiceClient<Channel> {
        &mut self.inner
    }

    // -----------------------------------------------------------------------
    // Layer 2: Capability Handshake
    // -----------------------------------------------------------------------

    /// Send a capability probe and receive an offer.
    pub async fn probe(
        &mut self,
        probe: CapabilityProbeMsg,
    ) -> Result<CapabilityOfferMsg, ClientError> {
        let pb_probe = codec::probe_to_proto(probe);
        let response = self.inner.probe(pb_probe).await?;
        let offer = codec::proto_to_offer(response.into_inner())?;
        Ok(offer)
    }

    /// Accept a contract and receive acknowledgement.
    pub async fn accept_contract(
        &mut self,
        accept: ContractAcceptMsg,
    ) -> Result<(String, bool), ClientError> {
        let pb_accept = codec::contract_accept_to_proto(accept);
        let response = self.inner.accept_contract(pb_accept).await?;
        let ack = response.into_inner();
        Ok((ack.contract_id, ack.accepted))
    }

    // -----------------------------------------------------------------------
    // Task lifecycle
    // -----------------------------------------------------------------------

    /// Submit a task and receive acknowledgement.
    pub async fn submit_task(
        &mut self,
        submit: TaskSubmitMsg,
    ) -> Result<(String, bool, String), ClientError> {
        let pb_submit = codec::task_submit_to_proto(submit);
        let response = self.inner.submit_task(pb_submit).await?;
        let ack = response.into_inner();
        Ok((ack.task_id, ack.accepted, ack.reason))
    }

    /// Stream results for a task, collecting all into a Vec.
    pub async fn stream_results(
        &mut self,
        task_id: Uuid,
    ) -> Result<Vec<DomainTaskResult>, ClientError> {
        let pb_query = proto::TaskQuery {
            task_id: task_id.to_string(),
        };
        let response = self.inner.stream_results(pb_query).await?;
        let mut stream = response.into_inner();

        let mut results = Vec::new();
        while let Some(pb_result) = stream.message().await? {
            let result = codec::proto_to_task_result(pb_result)?;
            results.push(result);
        }
        Ok(results)
    }

    /// Stream results for a task, returning the raw tonic streaming response.
    /// Use this for large result sets where you want to process results
    /// incrementally.
    pub async fn stream_results_raw(
        &mut self,
        task_id: Uuid,
    ) -> Result<tonic::Streaming<proto::TaskResult>, ClientError> {
        let pb_query = proto::TaskQuery {
            task_id: task_id.to_string(),
        };
        let response = self.inner.stream_results(pb_query).await?;
        Ok(response.into_inner())
    }

    // -----------------------------------------------------------------------
    // Layer 3: Context
    // -----------------------------------------------------------------------

    /// Request context chunks from a peer.
    pub async fn request_context(
        &mut self,
        req: ContextRequestMsg,
    ) -> Result<(Uuid, Vec<ContextChunk>), ClientError> {
        let pb_req = codec::context_request_to_proto(req);
        let response = self.inner.request_context(pb_req).await?;
        let pb_resp = response.into_inner();
        let task_id = Uuid::parse_str(&pb_resp.task_id)
            .map_err(|_| CodecError::InvalidUuid(pb_resp.task_id.clone()))?;
        let chunks: Vec<ContextChunk> = pb_resp
            .chunks
            .into_iter()
            .map(codec::proto_to_context_chunk)
            .collect();
        Ok((task_id, chunks))
    }

    // -----------------------------------------------------------------------
    // Layer 4: Routing
    // -----------------------------------------------------------------------

    /// Query routes for a task type with QoS constraints.
    pub async fn query_route(
        &mut self,
        task_type: TaskType,
        qos: QoSConstraints,
        preferred_pattern: Option<RoutingPattern>,
        max_routes: u32,
    ) -> Result<Vec<Route>, ClientError> {
        let pb_query = proto::RouteQuery {
            task_type: codec::task_type_to_i32(task_type),
            qos: Some(codec::qos_to_proto(qos)),
            preferred_pattern: preferred_pattern
                .map(codec::routing_pattern_to_i32)
                .unwrap_or(0),
            max_routes,
        };
        let response = self.inner.query_route(pb_query).await?;
        let pb_resp = response.into_inner();
        let routes: Result<Vec<Route>, _> = pb_resp
            .routes
            .into_iter()
            .map(codec::proto_to_route)
            .collect();
        Ok(routes?)
    }

    // -----------------------------------------------------------------------
    // Layer 5: Fault tolerance
    // -----------------------------------------------------------------------

    /// Send a heartbeat and receive the acknowledged sequence number.
    pub async fn send_heartbeat(
        &mut self,
        msg: HeartbeatMsg,
    ) -> Result<u64, ClientError> {
        let pb_hb = codec::heartbeat_to_proto(msg);
        let response = self.inner.send_heartbeat(pb_hb).await?;
        Ok(response.into_inner().sequence)
    }

    /// Report backpressure to a peer.
    pub async fn report_backpressure(
        &mut self,
        msg: BackpressureMsg,
    ) -> Result<bool, ClientError> {
        let pb_bp = codec::backpressure_to_proto(msg);
        let response = self.inner.report_backpressure(pb_bp).await?;
        Ok(response.into_inner().acknowledged)
    }

    /// Report a circuit break event.
    pub async fn report_circuit_break(
        &mut self,
        msg: CircuitBreakMsg,
    ) -> Result<bool, ClientError> {
        let pb_cb = codec::circuit_break_to_proto(msg);
        let response = self.inner.report_circuit_break(pb_cb).await?;
        Ok(response.into_inner().acknowledged)
    }

    // -----------------------------------------------------------------------
    // Layer 1: Trust
    // -----------------------------------------------------------------------

    /// Submit an interaction proof for trust scoring.
    pub async fn submit_interaction_proof(
        &mut self,
        proof: InteractionProofMsg,
    ) -> Result<bool, ClientError> {
        let pb_proof = codec::interaction_proof_to_proto(proof);
        let response = self.inner.submit_interaction_proof(pb_proof).await?;
        Ok(response.into_inner().accepted)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_error_display() {
        let err = ClientError::Codec(CodecError::MissingField("test"));
        assert!(err.to_string().contains("test"));
    }

    #[test]
    fn from_channel_compiles() {
        // Just verify the API compiles - we cannot actually connect in a unit test
        // without a running server.
        fn _assert_from_channel(ch: Channel) {
            let _client = AtpClient::from_channel(ch);
        }
    }
}
