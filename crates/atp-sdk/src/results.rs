use atp_types::{Route, TaskType};

// ── RouteResult ──────────────────────────────────────────────────────

/// The result of routing a task. Always printable.
///
/// ```text
/// Route: draft_refine via 2 agents (q=0.92, $0.0500, 45ms)
/// ```
#[derive(Debug, Clone)]
pub struct RouteResult {
    /// The task type that was routed.
    pub task: String,
    /// The routing pattern selected (e.g., "draft_refine", "cascade").
    pub pattern: String,
    /// Number of agents in the route.
    pub agents: usize,
    /// Expected quality score (0.0–1.0).
    pub quality: f64,
    /// Expected cost in USD.
    pub cost: f64,
    /// Expected latency in milliseconds.
    pub latency_ms: u64,
    ok: bool,
    error: Option<String>,
}

impl RouteResult {
    pub(crate) fn from_route(route: Route, task_type: TaskType) -> Self {
        Self {
            task: format!("{task_type}"),
            pattern: format!("{}", route.pattern),
            agents: route.agents.len(),
            quality: route.metrics.quality,
            cost: route.metrics.cost,
            latency_ms: route.metrics.latency.as_millis() as u64,
            ok: true,
            error: None,
        }
    }

    pub(crate) fn from_metrics(task_type: TaskType, metrics: &atp_sim::BenchMetrics) -> Self {
        Self {
            task: format!("{task_type}"),
            pattern: "full_atp".to_string(),
            agents: 1,
            quality: metrics.avg_quality,
            cost: metrics.avg_cost_per_task,
            latency_ms: metrics.avg_latency_ms as u64,
            ok: true,
            error: None,
        }
    }

    pub(crate) fn failed(task_type: TaskType, reason: &str) -> Self {
        Self {
            task: format!("{task_type}"),
            pattern: "none".to_string(),
            agents: 0,
            quality: 0.0,
            cost: 0.0,
            latency_ms: 0,
            ok: false,
            error: Some(reason.to_string()),
        }
    }

    /// Was the route found successfully?
    pub fn is_ok(&self) -> bool {
        self.ok
    }
}

impl std::fmt::Display for RouteResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ok {
            write!(
                f,
                "Route: {} via {} agent{} (q={:.2}, ${:.4}, {}ms)",
                self.pattern,
                self.agents,
                if self.agents == 1 { "" } else { "s" },
                self.quality,
                self.cost,
                self.latency_ms,
            )
        } else {
            write!(
                f,
                "Route: FAILED — {}",
                self.error.as_deref().unwrap_or("unknown")
            )
        }
    }
}

// ── CompressResult ───────────────────────────────────────────────────

/// The result of context compression. Always printable.
///
/// ```text
/// 28.3x compression (50000B → 1768B, 3 chunks)
/// ```
#[derive(Debug, Clone)]
pub struct CompressResult {
    /// Original data size in bytes.
    pub original_size: u64,
    /// Compressed size in bytes.
    pub compressed_size: u64,
    /// Compression ratio (original / compressed).
    pub ratio: f64,
    /// Number of context chunks retained.
    pub chunks: usize,
    /// Compression confidence score.
    pub confidence: f64,
}

impl std::fmt::Display for CompressResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:.1}x compression ({}B → {}B, {} chunk{})",
            self.ratio,
            self.original_size,
            self.compressed_size,
            self.chunks,
            if self.chunks == 1 { "" } else { "s" },
        )
    }
}

// ── TrustInfo ────────────────────────────────────────────────────────

/// Trust information for a task type. Always printable.
///
/// ```text
/// Trust: 0.87 (n=42)
/// ```
#[derive(Debug, Clone)]
pub struct TrustInfo {
    /// The trust score (0.0–1.0).
    pub score: f64,
    /// Number of interactions that contributed to this score.
    pub samples: u32,
}

impl TrustInfo {
    pub(crate) fn new(score: f64, samples: u32) -> Self {
        Self { score, samples }
    }
}

impl std::fmt::Display for TrustInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Trust: {:.2} (n={})", self.score, self.samples)
    }
}
