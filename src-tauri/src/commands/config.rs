use crate::config::{
    self, load_llm_configs, AppConfig, LlmConfig,
};
use crate::llm::adapter::{ConnectionStatus, LlmAdapter};
use crate::llm::create_adapter;
use serde::{Deserialize, Serialize};
use tauri::State;

// ---------------------------------------------------------------------------
// IPC response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigResponse {
    pub screenshot_interval: u64,
    pub analysis_interval_secs: u64,
    pub output_dir: String,
    pub llm_provider: String,
    pub llm_model: String,
    pub llm_endpoint: String,
    pub auto_start: bool,

    // Screenshot image / capture settings
    pub jpeg_quality: u32,
    pub max_width: u32,
    pub include_cursor: bool,
    pub dedup_threshold: u32,
    pub retention_days: u32,
    pub capture_all_displays: bool,

    pub privacy: PrivacyResponse,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PrivacyResponse {
    pub blur_sensitive: bool,
    pub blocked_apps: Vec<String>,
    pub idle_pause_minutes: u64,
}

/// Get the current full application configuration.
#[tauri::command]
pub fn get_config() -> Result<ConfigResponse, String> {
    eprintln!("[CMD] get_config called");
    let app_config = AppConfig::load_or_default();

    Ok(ConfigResponse {
        screenshot_interval: app_config.screenshot.interval_secs,
        analysis_interval_secs: app_config.screenshot.analysis_interval_secs,
        output_dir: app_config
            .screenshot
            .output_dir
            .to_string_lossy()
            .to_string(),
        llm_provider: app_config.llm.provider,
        llm_model: app_config.llm.model,
        llm_endpoint: app_config.llm.base_url,
        auto_start: false,
        jpeg_quality: app_config.screenshot.jpeg_quality,
        max_width: app_config.screenshot.max_width,
        include_cursor: app_config.screenshot.include_cursor,
        dedup_threshold: app_config.screenshot.dedup_threshold,
        retention_days: app_config.screenshot.retention_days,
        capture_all_displays: app_config.screenshot.capture_all_displays,
        privacy: PrivacyResponse {
            blur_sensitive: app_config.privacy.blur_sensitive,
            blocked_apps: app_config.privacy.blocked_apps,
            idle_pause_minutes: app_config.privacy.idle_pause_minutes,
        },
    })
}

/// Update the full application configuration.
#[tauri::command]
pub fn update_config(
    screenshot_interval: Option<u64>,
    analysis_interval_secs: Option<u64>,
    output_dir: Option<String>,
    llm_provider: Option<String>,
    llm_model: Option<String>,
    llm_endpoint: Option<String>,
    jpeg_quality: Option<u32>,
    max_width: Option<u32>,
    include_cursor: Option<bool>,
    dedup_threshold: Option<u32>,
    retention_days: Option<u32>,
    capture_all_displays: Option<bool>,
    blur_sensitive: Option<bool>,
    blocked_apps: Option<Vec<String>>,
    idle_pause_minutes: Option<u64>,
) -> Result<(), String> {
    let mut changed = Vec::new();
    let mut config = AppConfig::load_or_default();

    if let Some(v) = screenshot_interval {
        config.screenshot.interval_secs = v;
        changed.push("screenshot_interval");
    }
    if let Some(v) = analysis_interval_secs {
        config.screenshot.analysis_interval_secs = v;
        changed.push("analysis_interval_secs");
    }
    if let Some(v) = output_dir {
        config.screenshot.output_dir = v.into();
        changed.push("output_dir");
    }
    if let Some(v) = jpeg_quality {
        config.screenshot.jpeg_quality = v;
        changed.push("jpeg_quality");
    }
    if let Some(v) = max_width {
        config.screenshot.max_width = v;
        changed.push("max_width");
    }
    if let Some(v) = include_cursor {
        config.screenshot.include_cursor = v;
        changed.push("include_cursor");
    }
    if let Some(v) = dedup_threshold {
        config.screenshot.dedup_threshold = v;
        changed.push("dedup_threshold");
    }
    if let Some(v) = retention_days {
        config.screenshot.retention_days = v;
        changed.push("retention_days");
    }
    if let Some(v) = capture_all_displays {
        config.screenshot.capture_all_displays = v;
        changed.push("capture_all_displays");
    }
    if let Some(v) = llm_provider {
        config.llm.provider = v;
        changed.push("llm_provider");
    }
    if let Some(v) = llm_model {
        config.llm.model = v;
        changed.push("llm_model");
    }
    if let Some(v) = llm_endpoint {
        config.llm.base_url = v;
        changed.push("llm_endpoint");
    }
    if let Some(v) = blur_sensitive {
        config.privacy.blur_sensitive = v;
        changed.push("blur_sensitive");
    }
    if let Some(v) = blocked_apps {
        config.privacy.blocked_apps = v;
        changed.push("blocked_apps");
    }
    if let Some(v) = idle_pause_minutes {
        config.privacy.idle_pause_minutes = v;
        changed.push("idle_pause_minutes");
    }

    eprintln!("[CMD] update_config: {} fields changed: {:?}", changed.len(), changed);

    config.save().map_err(|e| format!("Failed to save config: {e}"))?;
    Ok(())
}

/// Get all saved LLM configuration presets (from config file, not DB).
/// Note: the DB-backed `get_llm_configs` is in lib.rs — this is for file-based preset management.
#[tauri::command]
pub fn get_llm_config_presets() -> Result<Vec<LlmConfig>, String> {
    eprintln!("[CMD] get_llm_config_presets called");
    let configs = load_llm_configs().map_err(|e| format!("Failed to load LLM configs: {e}"))?;
    eprintln!("[CMD] get_llm_config_presets: returned {} configs", configs.len());
    Ok(configs)
}

/// Save an LLM configuration preset.
///
/// Writes to both the JSON config file (for `get_llm_config_presets`) and
/// the SQLite DB (for `list_llm_configs`), fixing the two-path desync bug.
#[tauri::command]
pub async fn save_llm_config(
    state: State<'_, crate::AppState>,
    config: LlmConfig,
) -> Result<(), String> {
    eprintln!(
        "[CMD] save_llm_config: name={}, provider={}, model={}, has_api_key={}",
        config.name,
        config.provider,
        config.model,
        config.api_key.is_some()
    );

    // 1. Save to JSON (for config-file-based reads)
    let mut configs = load_llm_configs().unwrap_or_default();
    let name = config.name.clone();
    if let Some(pos) = configs.iter().position(|c| c.name == name) {
        configs[pos] = config.clone();
    } else {
        configs.push(config.clone());
    }
    config::save_llm_configs(&configs).map_err(|e| format!("Failed to save JSON: {e}"))?;
    eprintln!("[CMD] save_llm_config: saved {} configs to JSON", configs.len());

    // 2. Also save to DB (for list_llm_configs / get_active_llm_config)
    let db_config = crate::db::models::LlmConfig {
        id: 0,
        name: config.name.clone(),
        provider: config.provider.clone(),
        base_url: config.base_url.clone(),
        model: config.model.clone(),
        api_key: config.api_key.clone(),
        max_tokens: config.max_tokens as i64,
        is_active: true, // 强制激活：新配置/更新配置保存后立即可用
        created_at: "".to_string(),
        use_batch_api: Some(config.use_batch_api),
    };
    // ── upsert 前打印完整数据 ──
    eprintln!("[CONFIG-DEBUG] upsert_llm_config: name={}, provider={}, model={}, is_active={}, has_api_key={}",
        db_config.name, db_config.provider, db_config.model, db_config.is_active, db_config.api_key.is_some());

    let db = state.db.clone();
    let db_config_clone = db_config.clone();
    tokio::task::spawn_blocking(move || {
        db.upsert_llm_config(&db_config_clone)
            .map_err(|e| format!("Failed to save to DB: {e}"))
    })
    .await
    .map_err(|e| e.to_string())??;

    // ── upsert 后立即验证 ──
    let db = state.db.clone();
    let active = tokio::task::spawn_blocking(move || {
        db.get_active_config().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    match active {
        Some(c) => eprintln!("[CONFIG-DEBUG] After save, get_active_config() returned: name={}, has_api_key={}", c.name, c.api_key.is_some()),
        None => eprintln!("[CONFIG-DEBUG] After save, get_active_config() returned NONE!!"),
    }
    eprintln!("[CMD] save_llm_config: saved to DB successfully");

    Ok(())
}

/// Delete an LLM configuration preset by name.
#[tauri::command]
pub async fn delete_llm_config(state: State<'_, crate::AppState>, name: String) -> Result<(), String> {
    // Delete from JSON
    let mut configs =
        load_llm_configs().map_err(|e| format!("Failed to load LLM configs: {e}"))?;
    configs.retain(|c| c.name != name);
    config::save_llm_configs(&configs).map_err(|e| format!("Failed to save LLM configs: {e}"))?;

    // Delete from DB
    let db = state.db.clone();
    let name_clone = name.clone();
    tokio::task::spawn_blocking(move || {
        db.delete_llm_config_by_name(&name_clone).map_err(|e| format!("Failed to delete from DB: {e}"))
    })
    .await
    .map_err(|e| e.to_string())??;

    eprintln!("[CMD] delete_llm_config: name={name}");
    Ok(())
}

/// Test the LLM connection with the given configuration.
#[tauri::command]
pub async fn test_llm_connection(config: LlmConfig) -> Result<ConnectionStatus, String> {
    eprintln!("[CMD] test_llm_connection: endpoint={}, model={}", config.base_url, config.model);
    let adapter: Box<dyn LlmAdapter> =
        create_adapter(&config).map_err(|e| format!("Failed to create adapter: {e}"))?;

    let result = adapter
        .test_connection()
        .await
        .map_err(|e| format!("Connection test failed: {e}"))?;
    eprintln!("[CMD] test_llm_connection: status=ok");
    Ok(result)
}

/// List available models for the given provider configuration.
#[tauri::command]
pub async fn list_llm_models(config: LlmConfig) -> Result<Vec<String>, String> {
    eprintln!("[CMD] list_llm_models: endpoint={}", config.base_url);
    let adapter: Box<dyn LlmAdapter> =
        create_adapter(&config).map_err(|e| format!("Failed to create adapter: {e}"))?;

    let models = adapter
        .list_models()
        .await
        .map_err(|e| format!("Failed to list models: {e}"))?;
    eprintln!("[CMD] list_llm_models: returned {} models", models.len());
    Ok(models)
}

/// Get the screenshot analysis prompt for the current locale.
#[tauri::command]
pub fn get_analysis_prompt() -> Result<String, String> {
    eprintln!("[CMD] get_analysis_prompt called");
    let config = AppConfig::load_or_default();
    Ok(config::get_analysis_prompt(&config.locale).to_string())
}

/// Get the batch analysis prompt for the current locale.
#[tauri::command]
pub fn get_batch_analysis_prompt() -> Result<String, String> {
    eprintln!("[CMD] get_batch_analysis_prompt called");
    let config = AppConfig::load_or_default();
    Ok(config::get_batch_analysis_prompt(&config.locale).to_string())
}

/// Set the user locale preference ("zh" or "en") and persist.
#[tauri::command]
pub fn set_locale(locale: String) -> Result<(), String> {
    eprintln!("[CMD] set_locale: locale={locale}");
    let mut config = AppConfig::load_or_default();
    config.set_locale(&locale);
    config.save().map_err(|e| format!("Failed to save config: {e}"))?;
    Ok(())
}
