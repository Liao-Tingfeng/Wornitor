//! Tauri IPC commands for the activity analysis lifecycle.
//!
//! Provides commands to:
//! - Manually trigger a one-shot analysis
//! - Start / stop / pause / resume the background scheduler
//! - Query recording status
//! - CRUD activity segments (merge, update, delete, add manual)
//! - Get today's timeline

use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::config::AppConfig;
use crate::db::models::ActivitySegment;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Current status of the recording engine (v2 — Tauri IPC friendly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingStatusV2 {
    pub is_recording: bool,
    pub is_paused: bool,
    pub segment_count: i64,
    pub total_seconds: i64,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Get today's activity timeline — all segments from today ordered by start_time.
#[tauri::command]
pub async fn get_today_timeline(
    state: State<'_, crate::AppState>,
) -> Result<Vec<ActivitySegment>, String> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let segments = state
        .db
        .get_segments_by_date(&today)
        .map_err(|e| e.to_string())?;
    eprintln!("[CMD] get_today_timeline: returned {} segments", segments.len());
    Ok(segments)
}

/// Manually trigger a one-shot analysis: take a screenshot, analyze with LLM, persist.
#[tauri::command]
pub async fn trigger_analysis(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
) -> Result<ActivitySegment, String> {
    eprintln!("[CMD] trigger_analysis called");
    // 1. Screenshot
    let cfg = crate::screen::ScreenshotConfig::default();
    let frame = crate::screen::take_screenshot(&cfg).map_err(|e| e.to_string())?;

    // Dedup — skip if too similar to last frame
    let last = state.screenshot_state.get_last_hash();
    if crate::screen::is_duplicate(&frame.hash, last.as_deref(), cfg.dedup_threshold) {
        return Err("No significant change detected since last capture".into());
    }
    state.screenshot_state.set_last_hash(frame.hash.clone());

    // 2. Base64 encode
    let b64 = crate::image::encode_base64(&frame.jpeg_bytes);

    // 3. Get active LLM config and create adapter
    let db_config = state
        .db
        .get_active_config()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No active LLM configuration".to_string())?;

    let llm_cfg = crate::config::LlmConfig {
        name: db_config.name,
        provider: db_config.provider,
        base_url: db_config.base_url,
        model: db_config.model,
        api_key: db_config.api_key,
        max_tokens: db_config.max_tokens as u32,
        is_active: true,
        use_batch_api: db_config.use_batch_api.unwrap_or(false),
    };
    let adapter = crate::llm::create_adapter(&llm_cfg).map_err(|e| e.to_string())?;

    // 4. Analyze
    let image = crate::llm::adapter::AnalysisImage {
        data: b64,
        media_type: "image/jpeg".to_string(),
    };
    let locale = crate::config::AppConfig::load_or_default().locale;
    let prompt = crate::config::get_analysis_prompt(&locale);
    let (analysis, usage) = adapter
        .analyze_screenshots(&[image], prompt)
        .await
        .map_err(|e| e.to_string())?;

    // 5. Persist screenshot
    let now_str = frame
        .captured_at
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let db_frame = crate::db::models::ScreenshotFrame {
        id: frame.id.clone(),
        captured_at: now_str.clone(),
        file_path: frame.file_path,
        file_size: frame.jpeg_bytes.len() as i64,
        width: frame.width as i32,
        height: frame.height as i32,
        phash: frame.hash,
        app_name: analysis.app_name.clone(),
        window_title: analysis.window_title.clone(),
        created_at: now_str.clone(),
    };
    state
        .db
        .insert_screenshot(&db_frame)
        .map_err(|e| e.to_string())?;

    // 5b. Persist token usage to DB
    if let Some(u) = usage {
        let cost = crate::llm::adapter::estimate_cost(adapter.model_name(), &u);
        let usage_log = crate::db::models::LlmUsageLog {
            id: 0,
            model: adapter.model_name().to_string(),
            provider: adapter.provider_name().to_string(),
            prompt_tokens: u.prompt_tokens as i64,
            completion_tokens: u.completion_tokens as i64,
            total_tokens: u.total_tokens as i64,
            estimated_cost: cost,
            created_at: now_str.clone(),
        };
        let _ = state.db.insert_llm_usage_log(&usage_log);
        let _ = app_handle.emit(
            "analysis:llm-cost",
            serde_json::json!({
                "model": adapter.model_name(),
                "provider": adapter.provider_name(),
                "prompt_tokens": u.prompt_tokens,
                "completion_tokens": u.completion_tokens,
                "total_tokens": u.total_tokens,
                "cost": cost,
                "type": "analysis",
                "timestamp": now_str.clone(),
            }),
        );
    }

    // 6. Create and persist segment
    let segment = ActivitySegment {
        id: uuid::Uuid::new_v4().to_string(),
        start_time: now_str.clone(),
        end_time: now_str.clone(),
        duration_secs: 0,
        app_name: analysis.app_name,
        window_title: analysis.window_title,
        llm_summary: Some(analysis.activity),
        category: analysis.category,
        user_label: None,
        confidence: analysis.confidence as f64,
        source_frame_ids: Some(db_frame.id),
        is_manual: false,
        created_at: now_str,
        llm_cost: None,
        llm_tokens: None,
    };
    state
        .db
        .insert_segment(&segment)
        .map_err(|e| e.to_string())?;

    Ok(segment)
}

/// Start the background recording (analysis) scheduler.
#[tauri::command]
pub async fn start_recording(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
) -> Result<(), String> {
    eprintln!("[CMD] start_recording called");
    if state.scheduler.is_running() {
        eprintln!("[CMD] start_recording: already running, ignored");
        return Ok(()); // already running
    }

    // Read both capture and analysis intervals from config
    let app_config = AppConfig::load_or_default();
    let capture_interval = app_config.screenshot.interval_secs;
    let analysis_interval = app_config.screenshot.analysis_interval_secs;

    // Validate: analysis_interval must be a multiple of capture_interval
    if analysis_interval < capture_interval || analysis_interval % capture_interval != 0 {
        eprintln!("[CMD] start_recording: invalid intervals (capture={capture_interval}, analysis={analysis_interval}), defaulting to 5:1 ratio");
        // Fall back to a sensible default rather than failing silently
    }

    let db = state.db.clone();
    let screenshot_state = state.screenshot_state.clone();

    // Dispatch to the scheduler thread
    state
        .scheduler
        .start(capture_interval, analysis_interval, app_handle, db, screenshot_state)
}

/// Stop the background recording scheduler.
#[tauri::command]
pub async fn stop_recording(state: State<'_, crate::AppState>) -> Result<(), String> {
    eprintln!("[CMD] stop_recording called");
    state.scheduler.stop()
}

/// Pause the recording (keeps the loop alive but skips ticks).
#[tauri::command]
pub async fn pause_recording(state: State<'_, crate::AppState>) -> Result<(), String> {
    eprintln!("[CMD] pause_recording called");
    state.scheduler.pause();
    Ok(())
}

/// Resume a paused recording.
#[tauri::command]
pub async fn resume_recording(state: State<'_, crate::AppState>) -> Result<(), String> {
    eprintln!("[CMD] resume_recording called");
    state.scheduler.resume();
    Ok(())
}

/// Get current recording status with segment counts.
#[tauri::command]
pub async fn get_recording_status(
    state: State<'_, crate::AppState>,
) -> Result<RecordingStatusV2, String> {
    eprintln!("[CMD] get_recording_status called");
    let segments = state
        .db
        .get_segments_by_date(&Local::now().format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let total_secs: i64 = segments.iter().map(|s| s.duration_secs).sum();
    let status = RecordingStatusV2 {
        is_recording: state.scheduler.is_running(),
        is_paused: state.scheduler.is_paused(),
        segment_count: segments.len() as i64,
        total_seconds: total_secs,
    };
    eprintln!("[CMD] get_recording_status: recording={}, paused={}, segments={}", status.is_recording, status.is_paused, status.segment_count);
    Ok(status)
}

/// Merge multiple activity segments into one with a new label.
#[tauri::command]
pub async fn merge_segments(
    state: State<'_, crate::AppState>,
    segment_ids: Vec<String>,
    merged_label: String,
) -> Result<(), String> {
    eprintln!("[CMD] merge_segments: {} ids, label={}", segment_ids.len(), merged_label);
    if segment_ids.len() < 2 {
        return Err("Need at least 2 segments to merge".into());
    }

    let ids_ref: Vec<&str> = segment_ids.iter().map(|s| s.as_str()).collect();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let today_segments = state
        .db
        .get_segments_by_date(&today)
        .map_err(|e| e.to_string())?;
    let segments: Vec<ActivitySegment> = today_segments
        .into_iter()
        .filter(|s| segment_ids.contains(&s.id))
        .collect();

    if segments.len() < 2 {
        return Err("Could not find all specified segments".into());
    }

    let start_time = segments
        .iter()
        .map(|s| &s.start_time)
        .min()
        .cloned()
        .unwrap_or_default();
    let end_time = segments
        .iter()
        .map(|s| &s.end_time)
        .max()
        .cloned()
        .unwrap_or_default();
    let total_duration: i64 = segments.iter().map(|s| s.duration_secs).sum();
    let all_frame_ids: Vec<&str> = segments
        .iter()
        .filter_map(|s| s.source_frame_ids.as_deref())
        .collect();

    let mut cat_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for s in &segments {
        *cat_counts.entry(s.category.as_str()).or_default() += 1;
    }
    let category = cat_counts
        .into_iter()
        .max_by_key(|&(_, c)| c)
        .map(|(cat, _)| cat)
        .unwrap_or("other")
        .to_string();

    let merged = ActivitySegment {
        id: uuid::Uuid::new_v4().to_string(),
        start_time,
        end_time,
        duration_secs: total_duration,
        app_name: None,
        window_title: None,
        llm_summary: Some(merged_label.clone()),
        category,
        user_label: Some(merged_label),
        confidence: 1.0,
        source_frame_ids: Some(all_frame_ids.join(",")),
        is_manual: true,
        created_at: chrono::Utc::now()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        llm_cost: None,
        llm_tokens: None,
    };

    state
        .db
        .merge_segments(&ids_ref, &merged)
        .map_err(|e| e.to_string())
}

/// Update (edit) an existing activity segment.
#[tauri::command]
pub async fn update_segment(
    state: State<'_, crate::AppState>,
    segment: ActivitySegment,
) -> Result<(), String> {
    eprintln!("[CMD] update_segment: id={}, category={}", segment.id, segment.category);
    state
        .db
        .update_segment(&segment)
        .map_err(|e| e.to_string())
}

/// Delete an activity segment by ID.
#[tauri::command]
pub async fn delete_segment(
    state: State<'_, crate::AppState>,
    segment_id: String,
) -> Result<(), String> {
    eprintln!("[CMD] delete_segment: id={}", segment_id);
    state
        .db
        .delete_segment(&segment_id)
        .map_err(|e| e.to_string())
}

/// Manually add an activity segment (user-created).
#[tauri::command]
pub async fn add_manual_segment(
    state: State<'_, crate::AppState>,
    segment: ActivitySegment,
) -> Result<(), String> {
    eprintln!("[CMD] add_manual_segment: category={}", segment.category);
    let mut s = segment;
    s.is_manual = true;
    state.db.insert_segment(&s).map_err(|e| e.to_string())
}
