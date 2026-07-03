use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("Request timeout: {0}")]
    Timeout(String),

    #[error("Rate limited, retry after {0}s")]
    RateLimited(u64),

    #[error("API error ({status}): {message}")]
    ApiError { status: u16, message: String },

    #[error("Network error: {0}")]
    Network(String),

    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Token limit exceeded")]
    TokenOverflow,
}

impl LlmError {
    /// Convert a `reqwest::Error` into `LlmError::Network` with detailed diagnostics.
    ///
    /// This logs the root cause (timeout vs connect vs status vs other) so we
    /// can distinguish issues like DNS failure, TCP timeout, TLS handshake, etc.
    pub fn from_reqwest_error(e: reqwest::Error) -> Self {
        let detail = if e.is_timeout() {
            format!("Request timed out (connect={}, total={})", e.is_connect(), e.is_request())
        } else if e.is_connect() {
            format!("Connection failed (DNS/TCP/TLS): {e}")
        } else if e.is_status() {
            format!("HTTP error status: {}", e.status().map(|s| s.as_u16()).unwrap_or(0))
        } else if e.is_request() {
            format!("Request error (before response): {e}")
        } else {
            format!("Unexpected reqwest error: {e}")
        };

        eprintln!("[LLM] Network error: {detail}");
        LlmError::Network(detail)
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Token usage information from an LLM API response.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Estimate the cost (in CNY) for a given model and token usage.
pub fn estimate_cost(model: &str, usage: &UsageInfo) -> f64 {
    // Prices: ¥ per token (approximate, may need adjustments)
    let (prompt_price, completion_price): (f64, f64) = match model {
        // Kimi k2.5
        "kimi-k2.5" => (0.000_012, 0.000_012),
        // GPT-4o
        "gpt-4o" => (0.000_005, 0.000_015),
        // GPT-4o-mini
        "gpt-4o-mini" => (0.000_000_15, 0.000_000_60),
        // DeepSeek-V3
        "deepseek-chat" | "deepseek-v3" => (0.000_002, 0.000_008),
        // Claude 3.5 Sonnet
        "claude-3.5-sonnet" => (0.000_003, 0.000_015),
        // Default fallback
        _ => (0.000_010, 0.000_030),
    };

    usage.prompt_tokens as f64 * prompt_price + usage.completion_tokens as f64 * completion_price
}

/// An image to send for analysis (base64-encoded JPEG).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisImage {
    pub data: String,
    pub media_type: String,
}

/// Structured result from screenshot analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub activity: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub category: String,
    pub confidence: f32,
}

/// Context passed to the report-generation prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportContext {
    pub date_range: (String, String),
    pub segments: Vec<ActivitySegmentSummary>,
}

/// One activity segment within a report context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySegmentSummary {
    pub start_time: String,
    pub end_time: String,
    pub duration_secs: i64,
    pub app_name: Option<String>,
    pub category: String,
    pub summary: String,
}

/// Result from a connection test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub success: bool,
    pub message: String,
    pub model: Option<String>,
}

// ---------------------------------------------------------------------------
// LLM adapter trait
// ---------------------------------------------------------------------------

/// Common interface that every LLM provider must implement.
#[async_trait::async_trait]
pub trait LlmAdapter: Send + Sync {
    /// Send a batch of screenshot images for activity analysis.
    /// Returns the analysis result along with token usage (if available).
    async fn analyze_screenshots(
        &self,
        images: &[AnalysisImage],
        prompt: &str,
    ) -> Result<(AnalysisResult, Option<UsageInfo>), LlmError>;

    /// Generate a work report from activity segments.
    /// Returns the report text along with token usage (if available).
    async fn generate_report(
        &self,
        context: &ReportContext,
        prompt: &str,
    ) -> Result<(String, Option<UsageInfo>), LlmError>;

    /// Test whether the provider is reachable with the current config.
    async fn test_connection(&self) -> Result<ConnectionStatus, LlmError>;

    /// List available model identifiers.
    async fn list_models(&self) -> Result<Vec<String>, LlmError>;

    /// Return the model name (e.g. "kimi-k2.5", "gpt-4o").
    fn model_name(&self) -> &str;

    /// Return the provider name (e.g. "openai", "ollama").
    fn provider_name(&self) -> &str;
}
