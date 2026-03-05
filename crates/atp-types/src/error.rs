use thiserror::Error;

#[derive(Error, Debug)]
pub enum AtpError {
    #[error("identity error: {0}")]
    Identity(#[from] IdentityError),

    #[error("handshake error: {0}")]
    Handshake(#[from] HandshakeError),

    #[error("context error: {0}")]
    Context(#[from] ContextError),

    #[error("routing error: {0}")]
    Routing(#[from] RoutingError),

    #[error("fault error: {0}")]
    Fault(#[from] FaultError),

    #[error("transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Error, Debug)]
pub enum IdentityError {
    #[error("invalid DID format: {0}")]
    InvalidDid(String),

    #[error("signature verification failed")]
    SignatureVerification,

    #[error("unknown agent: {0}")]
    UnknownAgent(String),

    #[error("trust below threshold: {score} < {threshold}")]
    InsufficientTrust { score: f64, threshold: f64 },

    #[error("key generation failed: {0}")]
    KeyGeneration(String),
}

#[derive(Error, Debug)]
pub enum HandshakeError {
    #[error("handshake timeout after {0:?}")]
    Timeout(std::time::Duration),

    #[error("no capable agents found for task type {0}")]
    NoCapableAgents(String),

    #[error("contract negotiation failed: {0}")]
    NegotiationFailed(String),

    #[error("invalid handshake state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("offer expired")]
    OfferExpired,
}

#[derive(Error, Debug)]
pub enum ContextError {
    #[error("embedding dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    #[error("confidence below threshold: {0}")]
    LowConfidence(f64),

    #[error("context extraction failed: {0}")]
    ExtractionFailed(String),
}

#[derive(Error, Debug)]
pub enum RoutingError {
    #[error("no route satisfies constraints")]
    NoFeasibleRoute,

    #[error("negative cycle detected in routing graph")]
    NegativeCycle,

    #[error("route expired (TTL exceeded)")]
    RouteExpired,

    #[error("graph is empty")]
    EmptyGraph,
}

#[derive(Error, Debug)]
pub enum FaultError {
    #[error("heartbeat timeout for agent {0}")]
    HeartbeatTimeout(String),

    #[error("circuit open for agent {0}")]
    CircuitOpen(String),

    #[error("poison task detected: {0}")]
    PoisonTask(String),

    #[error("checkpoint restore failed: {0}")]
    CheckpointFailed(String),
}

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("gRPC error: {0}")]
    Grpc(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("connection refused: {0}")]
    ConnectionRefused(String),
}
