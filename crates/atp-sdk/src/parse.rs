use atp_types::TaskType;

/// Parse a user-friendly string into a TaskType.
///
/// Accepts many intuitive aliases:
/// - `"coding"`, `"code"`, `"codegen"`, `"code_generation"`, `"cg"` → CodeGeneration
/// - `"analysis"`, `"analyze"`, `"analyse"` → Analysis
/// - `"writing"`, `"creative"`, `"creative_writing"`, `"cw"` → CreativeWriting
/// - `"data"`, `"processing"`, `"data_processing"`, `"dp"` → DataProcessing
///
/// Case insensitive. Panics with a helpful message on bad input.
pub(crate) fn parse(s: &str) -> TaskType {
    match s.to_lowercase().trim() {
        "coding" | "code" | "codegen" | "code_generation" | "cg" => TaskType::CodeGeneration,
        "analysis" | "analyze" | "analyse" => TaskType::Analysis,
        "writing" | "creative" | "creative_writing" | "cw" => TaskType::CreativeWriting,
        "data" | "processing" | "data_processing" | "dp" => TaskType::DataProcessing,
        other => panic!(
            "Unknown task type: '{other}'. Try: coding, analysis, writing, or data"
        ),
    }
}

/// Minimum quality constraint for routing.
///
/// ```rust
/// let route = atp_sdk::find_route_with("coding", 0.9);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Quality(pub f64);
