use serde::{Deserialize, Serialize};

// ── Screenshot Frame ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotFrame {
    pub id: String,
    pub captured_at: String,
    pub file_path: String,
    pub file_size: i64,
    pub width: i32,
    pub height: i32,
    pub phash: String,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub created_at: String,
}

// ── Activity Segment ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySegment {
    pub id: String,
    pub start_time: String,
    pub end_time: String,
    pub duration_secs: i64,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub llm_summary: Option<String>,
    pub category: String,
    pub user_label: Option<String>,
    pub confidence: f64,
    pub source_frame_ids: Option<String>,
    pub is_manual: bool,
    pub created_at: String,
    pub llm_cost: Option<f64>,
    pub llm_tokens: Option<i64>,
}

// ── Daily Summary ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySummary {
    pub id: String,
    pub date: String,
    pub total_seconds: i64,
    pub segment_count: i64,
    pub activity_breakdown: Option<String>,
    pub llm_summary: Option<String>,
    pub user_notes: Option<String>,
    pub report_html: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ── Period Summary ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodSummary {
    pub id: String,
    pub r#type: String,
    pub start_date: String,
    pub end_date: String,
    pub total_seconds: i64,
    pub daily_trend: Option<String>,
    pub activity_breakdown: Option<String>,
    pub llm_summary: Option<String>,
    pub user_notes: Option<String>,
    pub report_html: Option<String>,
    pub created_at: String,
}

// ── LLM Config ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub id: i64,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub max_tokens: i64,
    pub is_active: bool,
    pub created_at: String,
    /// Use batch API instead of real-time API (only for OpenAI-compatible providers).
    #[serde(default)]
    pub use_batch_api: Option<bool>,
}

// ── Privacy Rule ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyRule {
    pub id: i64,
    pub rule_type: String,
    pub pattern: String,
    pub is_active: bool,
    pub blur_rect: Option<String>,
    pub created_at: String,
}

// ── LLM Usage Log ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmUsageLog {
    pub id: i64,
    pub model: String,
    pub provider: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost: f64,
    pub created_at: String,
}

/// Aggregated usage summary returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub call_count: i64,
}

// ── Recording Status (frontend-facing) ──────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingStatus {
    pub is_recording: bool,
    pub is_paused: bool,
    pub segment_count: i64,
    pub total_seconds: i64,
}
