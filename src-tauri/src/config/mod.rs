use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------

/// Default prompt for screenshot activity analysis (single image).
pub const SCREENSHOT_ANALYSIS_PROMPT: &str = r#"You are a work activity analyzer. Given a screenshot of the user's screen, identify:
1. What application is active
2. What window/tab title
3. What the user is doing (be specific)
4. Activity category (one of: dev, meeting, communication, design, documentation, browsing, other)

Respond ONLY with valid JSON (no markdown, no code blocks, no extra text):
{
  "activity": "short description",
  "app_name": "app name",
  "window_title": "window title",
  "category": "category",
  "confidence": 0.0-1.0
}

请用中文回复，使用专业简洁的表述。"#;

/// Screenshot analysis prompt — English (same as SCREENSHOT_ANALYSIS_PROMPT, kept for bilingual support).
pub const SCREENSHOT_ANALYSIS_PROMPT_EN: &str = r#"You are a work activity analyzer. Given a screenshot of the user's screen, identify:
1. What application is active
2. What window/tab title
3. What the user is doing (be specific)
4. Activity category (one of: dev, meeting, communication, design, documentation, browsing, other)

Respond ONLY with valid JSON (no markdown, no code blocks, no extra text):
{
  "activity": "short description",
  "app_name": "app name",
  "window_title": "window title",
  "category": "category",
  "confidence": 0.0-1.0
}

Please respond in English with professional and concise wording."#;

/// Default prompt for batch screenshot analysis (multiple images in a time window).
pub const BATCH_ANALYSIS_PROMPT: &str = r#"You are a work activity analyzer. Below are multiple screenshots captured over a period of time showing the user's screen activity.

Please analyze ALL screenshots together and produce a SINGLE consolidated analysis that best represents what the user was doing during this period.

Respond ONLY with valid JSON (no markdown, no code blocks, no extra text):
{
  "activity": "summary of what the user was doing during this period",
  "app_name": "the most used application",
  "window_title": "the most relevant window title",
  "category": "activity category (one of: dev, meeting, communication, design, documentation, browsing, other)",
  "confidence": 0.0-1.0
}

请用中文回复，使用专业简洁的表述。"#;

/// Batch analysis prompt — English.
pub const BATCH_ANALYSIS_PROMPT_EN: &str = r#"You are a work activity analyzer. Below are multiple screenshots captured over a period of time showing the user's screen activity.

Please analyze ALL screenshots together and produce a SINGLE consolidated analysis that best represents what the user was doing during this period.

Respond ONLY with valid JSON (no markdown, no code blocks, no extra text):
{
  "activity": "summary of what the user was doing during this period",
  "app_name": "the most used application",
  "window_title": "the most relevant window title",
  "category": "activity category (one of: dev, meeting, communication, design, documentation, browsing, other)",
  "confidence": 0.0-1.0
}

Please respond in English with professional and concise wording."#;

/// Default prompt for daily work summary generation.
pub const DAILY_SUMMARY_PROMPT: &str = r#"你是一个专业的工作报告生成器。根据用户今天的活动记录，生成一份简洁的日报。

请按以下结构输出（Markdown 格式）：

# XXXX年X月X日 工作日报

## 今日概览
- 总工作时间：X小时X分钟
- 活动片段数：X个
- 主要工作分类：分类1（X分钟）、分类2（X分钟）

## 时间线
（按时间顺序列出主要活动，每项包含时间段、时长、内容摘要）

## 重点成果
（列出当日最重要的2-3项工作成果）

## 备注
（任何值得记录的模式、中断或观察）

要求：
- 字数控制在 300 字以内
- 数据为空时输出"当天没有记录到活动。"
- 请用中文回复，使用专业简洁的表述"#;

/// Daily summary prompt — English.
pub const DAILY_SUMMARY_PROMPT_EN: &str = r#"You are a professional work report generator. Based on today's activity records, generate a concise daily summary.

Output in Markdown with the following structure:

# Daily Work Summary - YYYY-MM-DD

## Overview
- Total working time: Xh Xmin
- Number of activity segments: X
- Main categories: Category 1 (Xmin), Category 2 (Xmin)

## Timeline
(List main activities chronologically, each with time range, duration, and description)

## Key Accomplishments
(2-3 most important achievements)

## Notes
(Any notable patterns, interruptions, or observations)

Requirements:
- Keep within 300 words
- For empty data, output "No activity was recorded for this day."
- Please respond in English with professional and concise wording."#;

/// 周报总结 prompt — 中文
pub const WEEKLY_SUMMARY_PROMPT_ZH: &str = r#"你是一个专业的工作报告生成器。根据用户本周的活动记录，生成一份简洁的周报。

请按以下结构输出（Markdown 格式）：

# XXXX年X月X日 - XXXX年X月X日 工作周报

## 本周概览
- 总工作时间：X小时X分钟
- 工作天数：X天
- 日均工作时间：X小时X分钟
- 主要工作分类：分类1（X分钟）、分类2（X分钟）

## 时间线
（按天列出主要活动，每天包含时间段、时长、内容摘要）

## 重点成果
（列出本周最重要的3-5项工作成果）

## 分类统计
（按活动分类汇总时间，如 开发: X小时, 会议: X小时）

## 本周趋势与观察
（分析本周工作效率、模式变化、中断情况等）

## 备注
（任何值得记录的观察或建议）

要求：
- 字数控制在 500 字以内
- 数据为空时输出"本周没有记录到活动。"
- 请用中文回复，使用专业简洁的表述"#;

/// Weekly summary prompt — English
pub const WEEKLY_SUMMARY_PROMPT_EN: &str = r#"You are a professional work report generator. Based on this week's activity records, generate a concise weekly summary.

Output in Markdown with the following structure:

# Weekly Work Summary - YYYY-MM-DD to YYYY-MM-DD

## Overview
- Total working time: Xh Xmin
- Working days: X
- Daily average: Xh Xmin
- Main categories: Category 1 (Xmin), Category 2 (Xmin)

## Timeline
(List main activities by day, each with time range, duration, and description)

## Key Accomplishments
(3-5 most important achievements this week)

## Category Breakdown
(Time aggregated by activity category, e.g., Development: Xh, Meetings: Xh)

## Trends & Observations
(Productivity patterns, changes, interruptions, etc.)

## Notes
(Any notable observations or suggestions)

Requirements:
- Keep within 500 words
- For empty data, output "No activity was recorded for this week."
- Please respond in English with professional and concise wording."#;

/// 月报总结 prompt — 中文
pub const MONTHLY_SUMMARY_PROMPT_ZH: &str = r#"你是一个专业的工作报告生成器。根据用户本月的活动记录，生成一份简洁的月报。

请按以下结构输出（Markdown 格式）：

# XXXX年X月 工作月报

## 月度概览
- 总工作时间：X小时X分钟
- 工作天数：X天
- 日均工作时间：X小时X分钟
- 主要工作分类：分类1（X分钟）、分类2（X分钟）

## 重点项目
（列出本月最重要的3-5个重点项目，每项包含投入时间、关键进展、成果）

## 分类统计
（按活动分类汇总时间，如 开发: X小时, 会议: X小时, 设计: X小时）

## 月度趋势
（分析本月工作效率变化、各周对比、分类占比变化趋势）

## 总结与建议
（月度整体评估，下月改进建议）

要求：
- 字数控制在 800 字以内
- 数据为空时输出"本月没有记录到活动。"
- 请用中文回复，使用专业简洁的表述"#;

/// Monthly summary prompt — English
pub const MONTHLY_SUMMARY_PROMPT_EN: &str = r#"You are a professional work report generator. Based on this month's activity records, generate a concise monthly summary.

Output in Markdown with the following structure:

# Monthly Work Summary - YYYY-MM

## Overview
- Total working time: Xh Xmin
- Working days: X
- Daily average: Xh Xmin
- Main categories: Category 1 (Xmin), Category 2 (Xmin)

## Major Projects
(3-5 key projects this month, each with time invested, key progress, and outcomes)

## Category Breakdown
(Time aggregated by activity category, e.g., Development: Xh, Meetings: Xh, Design: Xh)

## Monthly Trends
(Productivity changes across weeks, category proportion shifts, weekly comparisons)

## Summary & Recommendations
(Overall monthly assessment, suggestions for the next month)

Requirements:
- Keep within 800 words
- For empty data, output "No activity was recorded for this month."
- Please respond in English with professional and concise wording."#;

// ---------------------------------------------------------------------------
// Prompt selection helpers (bilingual)
// ---------------------------------------------------------------------------

/// Select screenshot analysis prompt based on locale.
pub fn get_analysis_prompt(locale: &str) -> &'static str {
    if locale == "en" {
        SCREENSHOT_ANALYSIS_PROMPT_EN
    } else {
        SCREENSHOT_ANALYSIS_PROMPT
    }
}

/// Select batch analysis prompt based on locale.
pub fn get_batch_analysis_prompt(locale: &str) -> &'static str {
    if locale == "en" {
        BATCH_ANALYSIS_PROMPT_EN
    } else {
        BATCH_ANALYSIS_PROMPT
    }
}

/// Select daily summary prompt based on locale.
pub fn get_daily_summary_prompt(locale: &str) -> &'static str {
    if locale == "en" {
        DAILY_SUMMARY_PROMPT_EN
    } else {
        DAILY_SUMMARY_PROMPT
    }
}

/// Select weekly summary prompt based on locale.
pub fn get_weekly_summary_prompt(locale: &str) -> &'static str {
    if locale == "en" {
        WEEKLY_SUMMARY_PROMPT_EN
    } else {
        WEEKLY_SUMMARY_PROMPT_ZH
    }
}

/// Select monthly summary prompt based on locale.
pub fn get_monthly_summary_prompt(locale: &str) -> &'static str {
    if locale == "en" {
        MONTHLY_SUMMARY_PROMPT_EN
    } else {
        MONTHLY_SUMMARY_PROMPT_ZH
    }
}

// ---------------------------------------------------------------------------
// Config structures
// ---------------------------------------------------------------------------

/// Screenshot capture settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotConfig {
    /// Interval between captures (seconds).
    pub interval_secs: u64,
    /// Interval between LLM batch analyses (seconds).
    /// Must be a multiple of interval_secs.
    pub analysis_interval_secs: u64,
    /// Output directory for screenshot files.
    pub output_dir: PathBuf,
    /// Maximum number of screenshots to retain.
    pub max_files: u32,
    /// JPEG quality (1-100).
    pub jpeg_quality: u32,
    /// Maximum image width (in pixels).
    pub max_width: u32,
    /// Whether to include the cursor in screenshots.
    pub include_cursor: bool,
    /// Deduplication threshold (0-100, lower = more aggressive dedup).
    pub dedup_threshold: u32,
    /// Number of days to retain screenshots.
    pub retention_days: u32,
    /// Capture all connected displays and stitch them together.
    /// When false (default), only the main display is captured.
    #[serde(default)]
    pub capture_all_displays: bool,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            interval_secs: 30,
            analysis_interval_secs: 300,
            output_dir: PathBuf::from(""),
            max_files: 5000,
            jpeg_quality: 85,
            max_width: 1200,
            include_cursor: true,
            dedup_threshold: 5,
            retention_days: 30,
            capture_all_displays: false,
        }
    }
}

/// Privacy controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    /// Blur sensitive regions before analysis.
    pub blur_sensitive: bool,
    /// Do not capture when these app names are active.
    pub blocked_apps: Vec<String>,
    /// Automatically pause capture after idle minutes.
    pub idle_pause_minutes: u64,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            blur_sensitive: true,
            blocked_apps: vec![
                "LoginWindow".to_string(),
                "iTerm2".to_string(), // dev terminals may have credentials
                "Terminal".to_string(),
                "Keychain Access".to_string(),
            ],
            idle_pause_minutes: 5,
        }
    }
}

/// LLM provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Human-friendly name for this config preset.
    pub name: String,
    /// Provider type: "openai" | "ollama" | "custom"
    pub provider: String,
    /// API base URL (e.g. "https://api.openai.com/v1" or "http://localhost:11434").
    pub base_url: String,
    /// Model identifier (e.g. "gpt-4o", "qwen2.5:7b").
    pub model: String,
    /// API key for the LLM provider.
    pub api_key: Option<String>,
    /// Maximum output tokens.
    pub max_tokens: u32,
    /// Whether this config is the active/default one.
    pub is_active: bool,
    /// Use batch API instead of real-time API (only for OpenAI-compatible providers).
    /// When true, the scheduler submits batch jobs and polls for results.
    /// Default: false (real-time API).
    #[serde(default)]
    pub use_batch_api: bool,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            provider: "ollama".to_string(),
            base_url: "http://localhost:11434".to_string(),
            model: "qwen2.5:7b".to_string(),
            api_key: None,
            max_tokens: 4096,
            is_active: true,
            use_batch_api: false,
        }
    }
}

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub screenshot: ScreenshotConfig,
    pub llm: LlmConfig,
    pub privacy: PrivacyConfig,
    /// User locale preference ("zh" or "en")
    #[serde(default = "default_locale")]
    pub locale: String,
    /// Path where this config was loaded from / saved to.
    #[serde(skip)]
    pub config_path: Option<PathBuf>,
}

fn default_locale() -> String {
    "zh".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            screenshot: ScreenshotConfig::default(),
            llm: LlmConfig::default(),
            privacy: PrivacyConfig::default(),
            locale: default_locale(),
            config_path: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Config file not found: {0}")]
    NotFound(PathBuf),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Validation error: {0}")]
    Validation(String),
}

// ---------------------------------------------------------------------------
// Config paths
// ---------------------------------------------------------------------------

/// Default config directory for the Wornitor app.
fn config_dir() -> Result<PathBuf, ConfigError> {
    let dir = dirs_or_default();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Config file path.
fn config_path() -> Result<PathBuf, ConfigError> {
    Ok(config_dir()?.join("config.json"))
}

/// LLM configs file path (multi-preset support).
fn llm_configs_path() -> Result<PathBuf, ConfigError> {
    Ok(config_dir()?.join("llm_configs.json"))
}

/// Determine config directory: XDG_CONFIG_HOME or ~/.config/wornitor on Linux,
/// ~/Library/Application Support/wornitor on macOS.
fn dirs_or_default() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("wornitor")
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(xdg).join("wornitor")
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".config").join("wornitor")
        }
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| "C:\\Users\\Public".to_string());
        PathBuf::from(appdata).join("Wornitor")
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".wornitor")
    }
}

// ---------------------------------------------------------------------------
// Config loading / saving
// ---------------------------------------------------------------------------

impl AppConfig {
    /// Load the full application config from the default path.
    /// Returns `Err(ConfigError::NotFound)` if no config file exists.
    pub fn load() -> Result<Self, ConfigError> {
        let path = config_path()?;

        if !path.exists() {
            return Err(ConfigError::NotFound(path));
        }

        let content = fs::read_to_string(&path)?;
        let mut config: AppConfig = serde_json::from_str(&content)?;
        config.config_path = Some(path);

        Ok(config)
    }

    /// Save the full application config to the default path.
    /// API key is serialized directly into the JSON file.
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = self
            .config_path
            .clone()
            .or_else(|| config_path().ok())
            .ok_or_else(|| {
                ConfigError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not determine config path",
                ))
            })?;

        // Ensure the directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // api_key is serialized normally via serde
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;

        Ok(())
    }

    /// Load the config, or return a default if no file exists.
    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }

    /// Set the user locale and persist.
    pub fn set_locale(&mut self, locale: &str) {
        self.locale = locale.to_string();
    }
}

// ---------------------------------------------------------------------------
// Multi-preset LLM config management
// ---------------------------------------------------------------------------

/// Load all saved LLM config presets.
pub fn load_llm_configs() -> Result<Vec<LlmConfig>, ConfigError> {
    let path = llm_configs_path()?;

    eprintln!(
        "[CONFIG] Loading from {:?} (exists: {})",
        path,
        path.exists()
    );

    if !path.exists() {
        eprintln!("[CONFIG] No config file, returning default");
        return Ok(vec![LlmConfig::default()]);
    }

    let content = fs::read_to_string(&path)?;
    let configs: Vec<LlmConfig> = serde_json::from_str(&content)?;

    eprintln!("[CONFIG] Loaded {} configs from JSON", configs.len());
    for c in &configs {
        eprintln!(
            "[CONFIG]   Config: name={}, has_api_key={}",
            c.name,
            c.api_key.is_some()
        );
    }

    Ok(configs)
}

/// Save all LLM config presets (API keys are serialized directly into JSON).
pub fn save_llm_configs(configs: &[LlmConfig]) -> Result<(), ConfigError> {
    let path = llm_configs_path()?;

    eprintln!("[CONFIG] Writing {} configs to {:?}", configs.len(), path);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(configs)?;
    eprintln!("[CONFIG] Written {} bytes to {:?}", content.len(), path);
    fs::write(&path, &content)?;

    Ok(())
}
