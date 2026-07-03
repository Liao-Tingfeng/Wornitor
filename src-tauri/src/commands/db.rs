//! Tauri IPC commands for direct database queries used by the frontend.
//!
//! These are thin wrappers that let the frontend call DB methods via IPC.
//! All commands use spawn_blocking to avoid blocking the async runtime.

use tauri::State;

use crate::db::models::*;

#[tauri::command]
pub async fn get_segments_by_date(
    state: State<'_, crate::AppState>,
    date: String,
) -> Result<Vec<ActivitySegment>, String> {
    eprintln!("[CMD] get_segments_by_date: date={date}");
    let db = state.db.clone();
    let segments = tokio::task::spawn_blocking(move || {
        db.get_segments_by_date(&date).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!(
        "[CMD] get_segments_by_date: returned {} segments",
        segments.len()
    );
    Ok(segments)
}

#[tauri::command]
pub async fn get_segments_in_range(
    state: State<'_, crate::AppState>,
    from: String,
    to: String,
) -> Result<Vec<ActivitySegment>, String> {
    eprintln!("[CMD] get_segments_in_range: {from}~{to}");
    let db = state.db.clone();
    let segments = tokio::task::spawn_blocking(move || {
        db.get_segments_in_range(&from, &to).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!(
        "[CMD] get_segments_in_range: returned {} segments",
        segments.len()
    );
    Ok(segments)
}

#[tauri::command]
pub async fn get_screenshot(
    state: State<'_, crate::AppState>,
    id: String,
) -> Result<Option<ScreenshotFrame>, String> {
    eprintln!("[CMD] get_screenshot: id={id}");
    let db = state.db.clone();
    let result = tokio::task::spawn_blocking(move || {
        db.get_screenshot(&id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!("[CMD] get_screenshot: found={}", result.is_some());
    Ok(result)
}

#[tauri::command]
pub async fn get_screenshots_in_range(
    state: State<'_, crate::AppState>,
    from: String,
    to: String,
) -> Result<Vec<ScreenshotFrame>, String> {
    eprintln!("[CMD] get_screenshots_in_range: {from}~{to}");
    let db = state.db.clone();
    let frames = tokio::task::spawn_blocking(move || {
        db.get_screenshots_in_range(&from, &to).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!(
        "[CMD] get_screenshots_in_range: returned {} frames",
        frames.len()
    );
    Ok(frames)
}

#[tauri::command]
pub async fn get_screenshots_by_ids(
    state: State<'_, crate::AppState>,
    ids: Vec<String>,
) -> Result<Vec<ScreenshotFrame>, String> {
    eprintln!("[CMD] get_screenshots_by_ids: {} ids", ids.len());
    let db = state.db.clone();
    let frames = tokio::task::spawn_blocking(move || {
        db.get_screenshots_by_ids(&ids).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!(
        "[CMD] get_screenshots_by_ids: returned {} frames",
        frames.len()
    );
    Ok(frames)
}

#[tauri::command]
pub async fn get_recent_screenshots(
    state: State<'_, crate::AppState>,
    limit: i64,
) -> Result<Vec<ScreenshotFrame>, String> {
    eprintln!("[CMD] get_recent_screenshots: limit={limit}");
    let db = state.db.clone();
    let frames = tokio::task::spawn_blocking(move || {
        db.get_recent_screenshots(limit).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!(
        "[CMD] get_recent_screenshots: returned {} frames",
        frames.len()
    );
    Ok(frames)
}

#[tauri::command]
pub async fn get_daily_summaries_in_range(
    state: State<'_, crate::AppState>,
    from: String,
    to: String,
) -> Result<Vec<DailySummary>, String> {
    eprintln!("[CMD] get_daily_summaries_in_range: {from}~{to}");
    let db = state.db.clone();
    let summaries = tokio::task::spawn_blocking(move || {
        db.get_daily_summaries_in_range(&from, &to).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!(
        "[CMD] get_daily_summaries_in_range: returned {} summaries",
        summaries.len()
    );
    Ok(summaries)
}

#[tauri::command]
pub async fn get_period_summaries_by_type(
    state: State<'_, crate::AppState>,
    r#type: String,
) -> Result<Vec<PeriodSummary>, String> {
    eprintln!("[CMD] get_period_summaries_by_type: type={}", r#type);
    let db = state.db.clone();
    let summaries = tokio::task::spawn_blocking(move || {
        db.get_period_summaries_by_type(&r#type).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!(
        "[CMD] get_period_summaries_by_type: returned {} summaries",
        summaries.len()
    );
    Ok(summaries)
}

#[tauri::command]
pub async fn list_llm_configs(state: State<'_, crate::AppState>) -> Result<Vec<LlmConfig>, String> {
    eprintln!("[CMD] list_llm_configs called");
    let db = state.db.clone();
    let configs = tokio::task::spawn_blocking(move || {
        db.get_llm_configs().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!("[CMD] list_llm_configs: returned {} configs", configs.len());
    for c in &configs {
        eprintln!(
            "[CMD]   Config: id={}, name={}, has_api_key={}",
            c.id,
            c.name,
            c.api_key.is_some()
        );
    }
    Ok(configs)
}

#[tauri::command]
pub async fn get_active_llm_config(
    state: State<'_, crate::AppState>,
) -> Result<Option<LlmConfig>, String> {
    eprintln!("[CMD] get_active_llm_config called");
    let db = state.db.clone();
    let config = tokio::task::spawn_blocking(move || {
        db.get_active_config().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!("[CMD] get_active_llm_config: found={}", config.is_some());
    Ok(config)
}

#[tauri::command]
pub async fn get_active_privacy_rules(
    state: State<'_, crate::AppState>,
    rule_type: String,
) -> Result<Vec<PrivacyRule>, String> {
    eprintln!("[CMD] get_active_privacy_rules: type={rule_type}");
    let db = state.db.clone();
    let rules = tokio::task::spawn_blocking(move || {
        db.get_active_rules(&rule_type).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!(
        "[CMD] get_active_privacy_rules: returned {} rules",
        rules.len()
    );
    Ok(rules)
}

#[tauri::command]
pub async fn get_category_breakdown(
    state: State<'_, crate::AppState>,
    from: String,
    to: String,
) -> Result<Vec<(String, i64)>, String> {
    eprintln!("[CMD] get_category_breakdown: {from}~{to}");
    let db = state.db.clone();
    let breakdown = tokio::task::spawn_blocking(move || {
        db.get_category_breakdown(&from, &to).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!(
        "[CMD] get_category_breakdown: returned {} categories",
        breakdown.len()
    );
    Ok(breakdown)
}

// ═══════════════════════════════════════════════════════════════
//  LLM Usage / Cost queries
// ═══════════════════════════════════════════════════════════════

/// Get aggregated LLM usage summary for a specific date (YYYY-MM-DD).
#[tauri::command]
pub async fn get_daily_usage(
    state: State<'_, crate::AppState>,
    date: String,
) -> Result<UsageSummary, String> {
    eprintln!("[CMD] get_daily_usage: date={date}");
    let db = state.db.clone();
    let summary = tokio::task::spawn_blocking(move || {
        db.get_daily_usage_summary(&date).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!(
        "[CMD] get_daily_usage: tokens={}, cost=¥{:.4}, calls={}",
        summary.total_tokens, summary.total_cost, summary.call_count,
    );
    Ok(summary)
}

/// Get aggregated LLM usage summary for a specific month.
#[tauri::command]
pub async fn get_monthly_usage(
    state: State<'_, crate::AppState>,
    year: i32,
    month: u32,
) -> Result<UsageSummary, String> {
    eprintln!("[CMD] get_monthly_usage: {year}-{month}");
    let db = state.db.clone();
    let summary = tokio::task::spawn_blocking(move || {
        db.get_monthly_usage_summary(year, month).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!(
        "[CMD] get_monthly_usage: tokens={}, cost=¥{:.4}, calls={}",
        summary.total_tokens, summary.total_cost, summary.call_count,
    );
    Ok(summary)
}

#[tauri::command]
pub async fn get_daily_trend(
    state: State<'_, crate::AppState>,
    from: String,
    to: String,
) -> Result<Vec<(String, i64)>, String> {
    eprintln!("[CMD] get_daily_trend: {from}~{to}");
    let db = state.db.clone();
    let trend = tokio::task::spawn_blocking(move || {
        db.get_daily_trend(&from, &to).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!("[CMD] get_daily_trend: returned {} days", trend.len());
    Ok(trend)
}

#[tauri::command]
pub async fn clean_old_data(
    state: State<'_, crate::AppState>,
    retention_days: i64,
) -> Result<i64, String> {
    eprintln!("[CMD] clean_old_data: retention_days={retention_days}");
    let db = state.db.clone();
    let deleted = tokio::task::spawn_blocking(move || {
        db.clean_old_data(retention_days).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    eprintln!("[CMD] clean_old_data: deleted {deleted} rows");
    Ok(deleted)
}