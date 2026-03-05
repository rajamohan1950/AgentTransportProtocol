//! # ATP SDK — Dead-Simple Agent Transport Protocol
//!
//! Just call and go. No setup, no config, no enums, no builders.
//!
//! ## Quick Start
//!
//! ```rust
//! // Print the full benchmark table — one line
//! atp_sdk::benchmark();
//!
//! // Find the best route for a coding task — one line
//! atp_sdk::route("coding");
//!
//! // Compress context — one line
//! atp_sdk::compress(b"lots of context data here...", "coding");
//!
//! // Create an agent, sign & verify — three lines
//! let agent = atp_sdk::agent();
//! let sig = agent.sign(b"hello");
//! assert!(agent.verify(b"hello", &sig));
//! ```
//!
//! ## Two Flavors of Every Operation
//!
//! | Verb (prints)         | Noun (returns)                     |
//! |-----------------------|------------------------------------|
//! | `benchmark()`         | `bench(10_000) -> BenchReport`     |
//! | `route("coding")`     | `find_route("coding") -> RouteResult` |
//! | `compress(d, "code")` | `shrink(d, "code") -> CompressResult`  |
//! | `sign(b"msg")`        | `agent() -> Agent`                 |
//! | `trust("coding")`     | `trust_score("coding") -> TrustInfo`   |

mod agent;
mod network;
mod parse;
mod report;
mod results;

pub use agent::{Agent, Signature};
pub use network::Network;
pub use parse::Quality;
pub use report::BenchReport;
pub use results::{CompressResult, RouteResult, TrustInfo};

// ═══════════════════════════════════════════════════════════════════
//  VERB FUNCTIONS — print and done (fire & forget)
// ═══════════════════════════════════════════════════════════════════

/// Print the full 7-scenario benchmark table. Zero setup.
///
/// ```rust
/// atp_sdk::benchmark();
/// ```
pub fn benchmark() {
    println!("{}", bench(10_000));
}

/// Print the best route for a task type.
///
/// Accepts: `"coding"`, `"analysis"`, `"writing"`, `"data"`.
///
/// ```rust
/// atp_sdk::route("coding");
/// // prints: Route: draft_refine via 2 agents (q=0.92, $0.0500, 45ms)
/// ```
pub fn route(skill: &str) {
    println!("{}", find_route(skill));
}

/// Print context compression results.
///
/// ```rust
/// atp_sdk::compress(b"big context data here...", "coding");
/// // prints: 28.3x compression (50000B → 1768B, 3 chunks)
/// ```
pub fn compress(data: &[u8], skill: &str) {
    println!("{}", shrink(data, skill));
}

/// Create a new agent, sign the message, and print it.
///
/// ```rust
/// atp_sdk::sign(b"hello world");
/// // prints: Agent(did:key:z6Mk...abc)
/// //         Signed: Sig(a3f2b1c9...)
/// //         Verified: true
/// ```
pub fn sign(message: &[u8]) {
    let a = agent();
    let sig = a.sign(message);
    let verified = a.verify(message, &sig);
    println!("{a}");
    println!("  Signed: {sig}");
    println!("  Verified: {verified}");
}

/// Print network trust information for a task type.
///
/// ```rust
/// atp_sdk::trust("coding");
/// // prints: Trust: 0.87 (n=42)
/// ```
pub fn trust(skill: &str) {
    println!("{}", trust_score(skill));
}

// ═══════════════════════════════════════════════════════════════════
//  NOUN FUNCTIONS — return typed results for programmatic use
// ═══════════════════════════════════════════════════════════════════

/// Run the benchmark and return the report. Specify task count.
///
/// ```rust
/// let report = atp_sdk::bench(10_000);
/// println!("{report}");
///
/// // Access specific scenarios:
/// let atp = report.atp().unwrap();
/// println!("ATP cost: ${:.4}", atp.avg_cost_per_task);
/// ```
pub fn bench(tasks: usize) -> BenchReport {
    network::global().benchmark(tasks)
}

/// Find the best route for a task type. Returns a `RouteResult`.
///
/// ```rust
/// let r = atp_sdk::find_route("coding");
/// println!("Quality: {:.2}", r.quality);
/// println!("Cost: ${:.4}", r.cost);
/// ```
pub fn find_route(skill: &str) -> RouteResult {
    network::global().route(skill)
}

/// Find the best route with a minimum quality constraint.
///
/// ```rust
/// let r = atp_sdk::find_route_with("coding", 0.9);
/// assert!(r.quality >= 0.9 || !r.is_ok());
/// ```
pub fn find_route_with(skill: &str, min_quality: f64) -> RouteResult {
    network::global().route_with_quality(skill, min_quality)
}

/// Compress context data. Returns a `CompressResult`.
///
/// ```rust
/// let c = atp_sdk::shrink(b"big context data...", "coding");
/// println!("Ratio: {:.0}x", c.ratio);
/// ```
pub fn shrink(data: &[u8], skill: &str) -> CompressResult {
    network::global().compress(data, skill)
}

/// Create a new agent with a fresh Ed25519 keypair and DID.
///
/// ```rust
/// let a = atp_sdk::agent();
/// let sig = a.sign(b"hello");
/// assert!(a.verify(b"hello", &sig));
/// println!("{a}");  // Agent(did:key:z6Mk...abc)
/// ```
pub fn agent() -> Agent {
    Agent::new()
}

/// Get trust information for a task type across the global network.
///
/// ```rust
/// let t = atp_sdk::trust_score("coding");
/// println!("Score: {:.2}", t.score);
/// ```
pub fn trust_score(skill: &str) -> TrustInfo {
    network::global().trust(skill)
}

// ═══════════════════════════════════════════════════════════════════
//  TESTS
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_liner_benchmark_runs() {
        let report = bench(100);
        assert!(!report.all().is_empty());
        let s = format!("{report}");
        assert!(s.contains("Sequential"));
        assert!(s.contains("ATP"));
    }

    #[test]
    fn route_all_skills() {
        for skill in &["coding", "analysis", "writing", "data"] {
            let r = find_route(skill);
            assert!(r.is_ok(), "failed for {skill}: {r}");
        }
    }

    #[test]
    fn route_returns_good_quality() {
        let r = find_route("coding");
        assert!(r.is_ok());
        assert!(r.quality > 0.0);
        assert!(r.cost > 0.0);
        assert!(r.agents > 0);
    }

    #[test]
    fn route_with_quality_constraint() {
        let r = find_route_with("coding", 0.5);
        assert!(r.is_ok());
    }

    #[test]
    fn compress_works() {
        // Use varied data so chunks have distinct embeddings
        let data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let c = shrink(&data, "coding");
        assert!(c.ratio >= 1.0);
        let s = format!("{c}");
        assert!(s.contains("compression"));
    }

    #[test]
    fn agent_sign_verify() {
        let a = agent();
        let sig = a.sign(b"hello ATP");
        assert!(a.verify(b"hello ATP", &sig));
        assert!(!a.verify(b"tampered", &sig));
    }

    #[test]
    fn agent_display_contains_did() {
        let a = agent();
        let s = format!("{a}");
        assert!(s.contains("did:key"), "got: {s}");
    }

    #[test]
    fn signature_display() {
        let a = agent();
        let sig = a.sign(b"test");
        let s = format!("{sig}");
        assert!(s.starts_with("Sig("));
    }

    #[test]
    fn report_has_scenarios() {
        let report = bench(100);
        assert!(report.atp().is_some());
        assert!(report.baseline().is_some());
        assert!(report.scenario("Round").is_some());
    }

    #[test]
    fn route_display_format() {
        let r = find_route("coding");
        let s = format!("{r}");
        assert!(s.starts_with("Route:"), "got: {s}");
    }

    #[test]
    fn trust_works() {
        let t = trust_score("coding");
        assert!(t.score >= 0.0);
        assert!(t.samples > 0);
        let s = format!("{t}");
        assert!(s.contains("Trust:"));
    }

    #[test]
    #[should_panic(expected = "Unknown task type")]
    fn bad_skill_panics() {
        find_route("quantum_teleportation");
    }

    #[test]
    fn network_display() {
        let net = Network::new();
        let s = format!("{net}");
        assert!(s.contains("50 agents"));
    }
}
