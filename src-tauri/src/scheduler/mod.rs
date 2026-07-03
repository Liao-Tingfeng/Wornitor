//! Activity analysis scheduler and background task infrastructure.
//!
//! Three independent subsystems coexist here:
//!
//! 1. **`Scheduler`** — the analysis loop: screenshot → dedup → batch → LLM → DB → events.
//!    Screenshot frequency and LLM analysis frequency are decoupled (batch mode).
//! 2. **`TaskHandle` / `spawn_interval`** — generic interval-based loop used by
//!    the screenshot capture module (preserved for backward compatibility).
//!
//! 3. **Batch polling** — when `use_batch_api` is enabled, a background poll loop
//!    checks pending batch status and processes completed results.
//!
//! Dual-loop architecture for batch API mode:
//!   - `run_loop`: capture → dedup → enqueue → submit batch → continue (non-blocking)
//!   - `poll_loop`: (spawned separately) poll pending batches → fetch results → persist

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::Emitter;
use uuid::Uuid;

use crate::db::models::ActivitySegment;
use crate::llm::adapter::{AnalysisImage, UsageInfo};
use crate::llm::create_adapter;
use crate::llm::kimi_batch::{
    BatchCompletionResult, BatchInputItem, KimiBatchAdapter,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum images per batch — prevents exceeding LLM context window / token limit.
/// Rough estimate: each base64 JPEG ~180KB ≈ ~70K tokens, leaving room for prompt + response.
const MAX_IMAGES_PER_BATCH: usize = 10;

// ---------------------------------------------------------------------------
// Logger macro — unified [SCHEDULER] prefix
// ---------------------------------------------------------------------------
macro_rules! log_sched {
    ($($arg:tt)*) => {
        eprintln!("[SCHEDULER] {}", format_args!($($arg)*))
    };
}

// ---------------------------------------------------------------------------
// TaskHandle & spawn_interval (preserved for screenshot loop backward compat)
// ---------------------------------------------------------------------------

/// A handle to a running background task.
#[derive(Debug, Clone)]
pub struct TaskHandle {
    cancelled: Arc<AtomicBool>,
}

impl TaskHandle {
    /// Request that the task stop at its next iteration.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Check whether a cancel request has been made.
    #[allow(dead_code)]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Spawn a background task that runs `f` every `period`.
pub fn spawn_interval<F>(period: std::time::Duration, f: F) -> TaskHandle
where
    F: Fn() + Send + 'static,
{
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = cancelled.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);
        interval.tick().await; // skip immediate first tick

        loop {
            interval.tick().await;
            if cancelled_clone.load(Ordering::Acquire) {
                break;
            }
            f();
        }
    });

    TaskHandle { cancelled }
}

// ---------------------------------------------------------------------------
// Scheduler – the analysis loop (batch mode)
// ---------------------------------------------------------------------------

/// The analysis scheduler shared via `AppState`.
///
/// All mutable state is behind atomics / a mutex, so `&self` methods suffice
/// and the struct is `Send + Sync`.
pub struct Scheduler {
    is_paused: AtomicBool,
    is_running: AtomicBool,
    join_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

/// A pending screenshot frame stored in the batch buffer.
#[derive(Clone)]
struct PendingFrame {
    frame: crate::screen::ScreenshotFrame,
    // Pre-computed base64 data so we don't re-encode at batch time.
    // Lazily computed — `None` means not yet encoded.
    b64_cache: Option<String>,
}

/// A batch that has been submitted to the batch API and is waiting for completion.
struct PendingBatch {
    batch_id: String,
    submitted_at: chrono::NaiveDateTime,
    adapter_config: crate::config::LlmConfig,
    /// Original frames that were submitted — needed for persistence when results come back.
    frames: Vec<PendingFrame>,
    /// The prompt used for this batch (must match what the LLM was given).
    prompt: String,
    /// The model name used at submission time.
    model: String,
}

impl Scheduler {
    pub fn new() -> Self {
        log_sched!("Scheduler created");
        Self {
            is_paused: AtomicBool::new(false),
            is_running: AtomicBool::new(false),
            join_handle: Mutex::new(None),
        }
    }

    /// Start the analysis loop and (if batch mode) the poll loop in background tasks.
    ///
    /// `capture_interval_secs` — how often to take a screenshot.
    /// `analysis_interval_secs` — how often to batch-analyze pending screenshots.
    ///   Must be a multiple of `capture_interval_secs`.
    ///
    /// When the active LLM config has `use_batch_api=true`, a second `poll_loop`
    /// is spawned to check pending batch status and process completed results.
    ///
    /// The task captures the atomic flags **by clone** (Arc<AtomicBool>), so it
    /// can safely check pause/running from its own thread without borrowing the
    /// Scheduler struct.
    pub fn start(
        &self,
        capture_interval_secs: u64,
        analysis_interval_secs: u64,
        app_handle: tauri::AppHandle,
        db: crate::db::Database,
        screenshot_state: crate::screen::ScreenshotState,
    ) -> Result<(), String> {
        let mut guard = self.join_handle.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            log_sched!("Start requested but already running");
            return Err("Scheduler is already running".into());
        }
        log_sched!("Starting scheduler: capture={capture_interval_secs}s, analysis={analysis_interval_secs}s");

        // Validate that analysis_interval is a multiple of capture_interval
        if analysis_interval_secs < capture_interval_secs
            || analysis_interval_secs % capture_interval_secs != 0
        {
            log_sched!("WARNING: analysis_interval ({analysis_interval_secs}s) is not a multiple of capture_interval ({capture_interval_secs}s). Rounding up.");
        }

        self.is_running.store(true, Ordering::Release);
        self.is_paused.store(false, Ordering::Release);

        // Clone the atomic flags so the spawned task can check them without
        // borrowing the Scheduler struct itself.
        let run_flag = Arc::new(AtomicBool::new(true));
        let pause_flag = Arc::new(AtomicBool::new(false));

        let run_flag_for_loop = run_flag.clone();
        let pause_flag_for_loop = pause_flag.clone();
        let run_flag_for_poll = run_flag.clone();
        let pause_flag_for_poll = pause_flag.clone();
        let app_handle_for_poll = app_handle.clone();
        let db_for_poll = db.clone();

        // Shared pending batches list — written by run_loop, read by poll_loop
        let pending_batches: Arc<Mutex<Vec<PendingBatch>>> = Arc::new(Mutex::new(Vec::new()));
        let pending_batches_for_poll = pending_batches.clone();
        let pending_batches_for_loop = pending_batches.clone();

        let handle = tokio::spawn(async move {
            Scheduler::run_loop(
                capture_interval_secs,
                analysis_interval_secs,
                app_handle,
                db,
                screenshot_state,
                run_flag_for_loop,
                pause_flag_for_loop,
                pending_batches_for_loop,
            )
            .await;
        });

        // Spawn the poll loop only if we detect a provider that supports batch API
        let poll_handle = tokio::spawn(async move {
            // Check if the active config uses batch API
            let use_batch = Self::should_use_batch_api(&db_for_poll).await;
            if use_batch {
                log_sched!("Batch API mode detected — starting poll loop");
                Scheduler::poll_loop(
                    pending_batches_for_poll,
                    db_for_poll,
                    app_handle_for_poll,
                    run_flag_for_poll,
                    pause_flag_for_poll,
                )
                .await;
            } else {
                log_sched!("Real-time API mode — poll loop not started");
                // Keep the task alive until shutdown but do nothing
                while run_flag_for_poll.load(Ordering::Acquire) {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        });

        *guard = Some(handle);
        // Note: we don't track poll_handle in join_handle — it's a subsidiary task
        // that will naturally exit when run_flag becomes false.
        // We use a detached tokio::spawn (owned by the runtime).
        // If stop() is called, the main handle is aborted, and poll_handle
        // should check the flag and exit on its own.
        //
        // To ensure clean shutdown of both, we store only the main handle.
        // The poll loop reads the same shared flags and will exit within 5s.
        // We intentionally leak poll_handle (detached) — when the runtime drops,
        // both tasks are cancelled.
        std::mem::forget(poll_handle);
        Ok(())
    }

    /// Check whether the active database config enables batch API mode.
    async fn should_use_batch_api(db: &crate::db::Database) -> bool {
        match db.get_active_config() {
            Ok(Some(cfg)) => {
                let provider = cfg.provider.to_lowercase();
                let use_batch = cfg.use_batch_api.unwrap_or(false);
                // Batch API only supported for OpenAI-compatible providers
                if use_batch && (provider == "openai" || provider == "custom") {
                    true
                } else if use_batch {
                    log_sched!(
                        "WARNING: use_batch_api=true but provider='{}' doesn't support batch API. Falling back to real-time.",
                        provider
                    );
                    false
                } else {
                    false
                }
            }
            Ok(None) => false,
            Err(e) => {
                log_sched!("Failed to check batch API config: {e}");
                false
            }
        }
    }

    pub fn pause(&self) {
        self.is_paused.store(true, Ordering::Release);
        log_sched!("Paused");
    }

    pub fn resume(&self) {
        self.is_paused.store(false, Ordering::Release);
        log_sched!("Resumed");
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused.load(Ordering::Acquire)
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Acquire)
    }

    pub fn stop(&self) -> Result<(), String> {
        self.is_running.store(false, Ordering::Release);
        let handle = self.join_handle.lock().map_err(|e| e.to_string())?.take();
        if let Some(h) = handle {
            h.abort();
        }
        Ok(())
    }

    // ── Internal: the async loop (self-contained, no &self borrowing) ──

    async fn run_loop(
        capture_interval_secs: u64,
        analysis_interval_secs: u64,
        app_handle: tauri::AppHandle,
        db: crate::db::Database,
        screenshot_state: crate::screen::ScreenshotState,
        is_running: Arc<AtomicBool>,
        is_paused: Arc<AtomicBool>,
        pending_batches: Arc<Mutex<Vec<PendingBatch>>>,
    ) {
        let capture_duration = std::time::Duration::from_secs(capture_interval_secs);
        let analysis_duration = std::time::Duration::from_secs(analysis_interval_secs);

        let mut pending_frames: Vec<PendingFrame> = Vec::new();
        let mut last_analysis = std::time::Instant::now();
        let mut last_activity: Option<std::time::Instant> = None;
        let mut last_category: Option<String> = None;
        let mut current_segment_id: Option<String> = None;
        let mut tick: u64 = 0;

        log_sched!(
            "Run loop started: capture={capture_interval_secs}s, analysis={analysis_interval_secs}s, max_batch={MAX_IMAGES_PER_BATCH}"
        );

        while is_running.load(Ordering::Acquire) {
            tick += 1;

            // Run each tick wrapped in a panic-catching helper that resets
            // mutable state on panic to avoid corrupted invariants.
            let tick_ok = run_safe_tick(
                tick,
                capture_duration,
                analysis_duration,
                &mut pending_frames,
                &mut last_analysis,
                &mut last_activity,
                &mut last_category,
                &mut current_segment_id,
                &app_handle,
                &db,
                &screenshot_state,
                &is_paused,
                &pending_batches,
            )
            .await;

            if !tick_ok {
                // Panic was caught inside run_safe_tick; state has been reset.
                // Sleep briefly to avoid tight panic loop.
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }

        log_sched!("Run loop exited");
    }

    /// Real-time analysis path: create adapter, call LLM, persist results.
    async fn handle_realtime_analyze(
        db: &crate::db::Database,
        app_handle: &tauri::AppHandle,
        batch: Vec<PendingFrame>,
        prompt: &str,
        capture_duration: std::time::Duration,
        last_category: &mut Option<String>,
        current_segment_id: &mut Option<String>,
    ) {
        // Build AnalysisImage list from the batch
        let images: Vec<AnalysisImage> = batch
            .iter()
            .map(|pf| {
                let data = pf.b64_cache.clone().unwrap_or_default();
                AnalysisImage {
                    data,
                    media_type: "image/jpeg".to_string(),
                }
            })
            .collect();

        // Create LLM adapter from active DB config
        let adapter = match Self::create_llm_adapter(db, app_handle).await {
            Some(a) => a,
            None => return,
        };

        log_sched!(
            "Sending batch to real-time LLM: {} images, prompt_len={}",
            images.len(),
            prompt.len()
        );

        let llm_result =
            Self::call_llm_with_retry(&*adapter, &images, prompt, app_handle, capture_duration)
                .await;

        let (analysis, usage) = match llm_result {
            Some((a, u)) => (a, u),
            None => return,
        };

        // Persist token usage
        Self::persist_usage(db, app_handle, usage.as_ref(), &batch, &*adapter);

        log_sched!(
            "LLM batch result: category={}, activity={}, confidence={}",
            analysis.category,
            analysis.activity,
            analysis.confidence,
        );

        // Extract cost info for segment association
        let (cost, total_tokens) = usage
            .as_ref()
            .map(|u| {
                let cost = crate::llm::adapter::estimate_cost(&adapter.model_name(), u);
                (cost, u.total_tokens as i64)
            })
            .unwrap_or((0.0, 0));

        // Persist frames and segment
        Self::persist_analysis(
            db,
            app_handle,
            &batch,
            &analysis,
            capture_duration,
            last_category,
            current_segment_id,
            cost,
            total_tokens,
        )
        .await;
    }

    /// Batch API submit path: submit to batch API, store in pending_batches for poll loop.
    async fn handle_batch_submit(
        llm_config: &crate::config::LlmConfig,
        batch: Vec<PendingFrame>,
        prompt: &str,
        pending_batches: &Arc<Mutex<Vec<PendingBatch>>>,
        app_handle: &tauri::AppHandle,
    ) {
        // Build BatchInputItem list from the batch
        let items: Vec<BatchInputItem> = batch
            .iter()
            .map(|pf| {
                let b64 = pf.b64_cache.clone().unwrap_or_default();
                KimiBatchAdapter::build_analysis_item(
                    &pf.frame.id,
                    &llm_config.model,
                    &b64,
                    prompt,
                    llm_config.max_tokens,
                )
            })
            .collect();

        let adapter = KimiBatchAdapter::new(llm_config.clone());

        log_sched!(
            "Submitting batch to batch API: {} items, model={}",
            items.len(),
            llm_config.model
        );

        match adapter.submit_batch(items).await {
            Ok(batch_id) => {
                log_sched!("Batch submitted successfully: id={}", batch_id);
                let now = chrono::Local::now().naive_local();
                let mut guard = pending_batches.lock().unwrap();
                guard.push(PendingBatch {
                    batch_id,
                    submitted_at: now,
                    adapter_config: llm_config.clone(),
                    frames: batch,
                    prompt: prompt.to_string(),
                    model: llm_config.model.clone(),
                });
                let _ = app_handle.emit(
                    "analysis:batch-submitted",
                    serde_json::json!({
                        "batch_id": guard.last().map(|b| b.batch_id.clone()),
                        "pending_count": guard.len(),
                        "timestamp": now.format("%Y-%m-%d %H:%M:%S").to_string(),
                    }),
                );
            }
            Err(e) => {
                log_sched!("Batch submission failed: {e}");
                let _ = app_handle.emit(
                    "analysis:error",
                    serde_json::json!({"error": format!("Batch submission failed: {e}")}),
                );
            }
        }
    }

    /// Create an LLM adapter from the active DB config.
    /// Returns `None` if no config is available (emits an error event).
    async fn create_llm_adapter(
        db: &crate::db::Database,
        app_handle: &tauri::AppHandle,
    ) -> Option<Box<dyn crate::llm::adapter::LlmAdapter>> {
        let db_config = match db.get_active_config() {
            Ok(Some(c)) => c,
            Ok(None) => {
                log_sched!("No active LLM config, skipping batch");
                let _ = app_handle.emit(
                    "analysis:error",
                    serde_json::json!({"error": "No active LLM config"}),
                );
                return None;
            }
            Err(e) => {
                log_sched!("DB error fetching config: {e}");
                return None;
            }
        };

        log_sched!(
            "Using LLM config: name={}, provider={}, model={}",
            db_config.name,
            db_config.provider,
            db_config.model
        );

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

        match create_adapter(&llm_cfg) {
            Ok(a) => Some(a),
            Err(e) => {
                log_sched!("LLM adapter creation failed: {e}");
                None
            }
        }
    }

    /// Call LLM `analyze_screenshots` with up to 2 retries on transient errors.
    /// Returns `(AnalysisResult, Option<UsageInfo>)` on success.
    async fn call_llm_with_retry(
        adapter: &dyn crate::llm::adapter::LlmAdapter,
        images: &[AnalysisImage],
        prompt: &str,
        app_handle: &tauri::AppHandle,
        _capture_duration: std::time::Duration,
    ) -> Option<(crate::llm::adapter::AnalysisResult, Option<UsageInfo>)> {
        let mut retries = 0;
        loop {
            let result = adapter.analyze_screenshots(images, prompt).await;
            match result {
                Ok((a, usage)) => {
                    log_sched!(
                        "LLM result: category={}, activity={}, confidence={}",
                        a.category,
                        a.activity,
                        a.confidence
                    );
                    return Some((a, usage));
                }
                Err(e) => {
                    let is_transient = matches!(
                        &e,
                        crate::llm::adapter::LlmError::Network(_)
                            | crate::llm::adapter::LlmError::Timeout(_)
                    );
                    if is_transient && retries < 2 {
                        retries += 1;
                        let backoff = 5;
                        log_sched!(
                            "LLM call failed (attempt #{retries}), retrying in {backoff}s: {e}"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                        continue;
                    }
                    log_sched!("LLM analysis failed after {retries} retries: {e}");
                    let _ = app_handle.emit(
                        "analysis:error",
                        serde_json::json!({"error": format!("LLM analysis failed: {e}")}),
                    );
                    return None;
                }
            }
        }
    }

    // ── Persistence helpers ────────────────────────────────────────────────

    /// Persist token usage to DB and emit cost event.
    fn persist_usage(
        db: &crate::db::Database,
        app_handle: &tauri::AppHandle,
        usage: Option<&UsageInfo>,
        batch: &[PendingFrame],
        adapter: &dyn crate::llm::adapter::LlmAdapter,
    ) {
        let Some(u) = usage else { return };
        let now_str = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let model_name = adapter.model_name().to_string();
        let provider_name = adapter.provider_name().to_string();
        let cost = crate::llm::adapter::estimate_cost(&model_name, u);
        let usage_log = crate::db::models::LlmUsageLog {
            id: 0,
            model: model_name.clone(),
            provider: provider_name.clone(),
            prompt_tokens: u.prompt_tokens as i64,
            completion_tokens: u.completion_tokens as i64,
            total_tokens: u.total_tokens as i64,
            estimated_cost: cost,
            created_at: now_str.clone(),
        };
        if let Err(e) = db.insert_llm_usage_log(&usage_log) {
            log_sched!("Failed to insert LLM usage log: {e}");
        } else {
            log_sched!("LLM usage logged: tokens={}, cost=¥{:.6}", u.total_tokens, cost);
            let _ = app_handle.emit(
                "analysis:llm-cost",
                serde_json::json!({
                    "model": model_name,
                    "provider": provider_name,
                    "prompt_tokens": u.prompt_tokens,
                    "completion_tokens": u.completion_tokens,
                    "total_tokens": u.total_tokens,
                    "cost": cost,
                    "type": "analysis",
                    "batch_size": batch.len(),
                    "timestamp": now_str,
                }),
            );
        }
    }

    /// Persist token usage from a batch API result (no LlmAdapter available).
    fn persist_usage_batch(
        db: &crate::db::Database,
        app_handle: &tauri::AppHandle,
        usage: Option<&UsageInfo>,
        batch: &[PendingFrame],
        model_name: &str,
    ) {
        let Some(u) = usage else { return };
        let now_str = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let provider_name = "kimi-batch";
        let cost = crate::llm::adapter::estimate_cost(model_name, u);
        let usage_log = crate::db::models::LlmUsageLog {
            id: 0,
            model: model_name.to_string(),
            provider: provider_name.to_string(),
            prompt_tokens: u.prompt_tokens as i64,
            completion_tokens: u.completion_tokens as i64,
            total_tokens: u.total_tokens as i64,
            estimated_cost: cost,
            created_at: now_str.clone(),
        };
        if let Err(e) = db.insert_llm_usage_log(&usage_log) {
            log_sched!("Failed to insert LLM usage log: {e}");
        } else {
            log_sched!("Batch LLM usage logged: tokens={}, cost=¥{:.6}", u.total_tokens, cost);
            let _ = app_handle.emit(
                "analysis:llm-cost",
                serde_json::json!({
                    "model": model_name,
                    "provider": provider_name,
                    "prompt_tokens": u.prompt_tokens,
                    "completion_tokens": u.completion_tokens,
                    "total_tokens": u.total_tokens,
                    "cost": cost,
                    "type": "analysis",
                    "batch_size": batch.len(),
                    "timestamp": now_str,
                }),
            );
        }
    }

    /// Persist screenshot frames and create/extend activity segment.
    async fn persist_analysis(
        db: &crate::db::Database,
        app_handle: &tauri::AppHandle,
        batch: &[PendingFrame],
        analysis: &crate::llm::adapter::AnalysisResult,
        capture_duration: std::time::Duration,
        last_category: &mut Option<String>,
        current_segment_id: &mut Option<String>,
        llm_cost: f64,
        llm_tokens: i64,
    ) {
        let base_dir = crate::dirs_db_path()
            .map(std::path::PathBuf::from)
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));

        // Collect frame IDs for the segment
        let mut frame_ids: Vec<String> = Vec::new();

        for pf in batch {
            let now_str = pf.frame.captured_at.format("%Y-%m-%d %H:%M:%S").to_string();
            let disk_path = crate::screen::save_to_disk(&pf.frame, &base_dir).ok();

            let db_frame = crate::db::models::ScreenshotFrame {
                id: pf.frame.id.clone(),
                captured_at: now_str,
                file_path: disk_path.unwrap_or_default(),
                file_size: pf.frame.jpeg_bytes.len() as i64,
                width: pf.frame.width as i32,
                height: pf.frame.height as i32,
                phash: pf.frame.hash.clone(),
                app_name: analysis.app_name.clone(),
                window_title: analysis.window_title.clone(),
                created_at: pf.frame.captured_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            if let Err(e) = db.insert_screenshot(&db_frame) {
                log_sched!("DB insert screenshot failed: {e}");
            } else {
                log_sched!("Screenshot saved to DB: id={}", pf.frame.id);
            }
            frame_ids.push(pf.frame.id.clone());
        }

        // Calculate batch span
        let capture_secs = capture_duration.as_secs();
        let start_ts = batch
            .first()
            .map(|pf| pf.frame.captured_at.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();
        let end_ts = batch
            .last()
            .map(|pf| pf.frame.captured_at.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();
        let batch_duration_secs = (batch.len() as u64) * capture_secs;

        let same_category = last_category.as_deref() == Some(&analysis.category);

        if same_category {
            // Extend the existing segment
            if let Some(ref seg_id) = current_segment_id {
                if let Ok(Some(mut seg)) = db.get_segment_by_id(seg_id) {
                    seg.end_time = end_ts.clone();
                    seg.duration_secs += batch_duration_secs as i64;
                    if let Some(ref summary) = seg.llm_summary {
                        seg.llm_summary = Some(format!("{summary}; {}", analysis.activity));
                    } else {
                        seg.llm_summary = Some(analysis.activity.clone());
                    }
                    let existing = seg.source_frame_ids.unwrap_or_default();
                    let new_ids = frame_ids.join(",");
                    seg.source_frame_ids = Some(if existing.is_empty() {
                        new_ids
                    } else {
                        format!("{existing},{new_ids}")
                    });
                    let _ = db.update_segment(&seg);
                    // Update cost on the segment (accumulate)
                    if llm_cost > 0.0 || llm_tokens > 0 {
                        let new_cost = seg.llm_cost.unwrap_or(0.0) + llm_cost;
                        let new_tokens = seg.llm_tokens.unwrap_or(0) + llm_tokens;
                        let _ = db.update_segment_cost(&seg.id, new_cost, new_tokens);
                        seg.llm_cost = Some(new_cost);
                        seg.llm_tokens = Some(new_tokens);
                    }
                    log_sched!(
                        "Extended segment id={}, duration_secs={}",
                        seg.id,
                        seg.duration_secs
                    );
                    let _ = app_handle.emit("analysis:update-segment", &seg);
                }
            }
        } else {
            // Create a new segment
            let segment = ActivitySegment {
                id: Uuid::new_v4().to_string(),
                start_time: start_ts,
                end_time: end_ts.clone(),
                duration_secs: batch_duration_secs as i64,
                app_name: analysis.app_name.clone(),
                window_title: analysis.window_title.clone(),
                llm_summary: Some(analysis.activity.clone()),
                category: analysis.category.clone(),
                user_label: None,
                confidence: analysis.confidence as f64,
                source_frame_ids: Some(frame_ids.join(",")),
                is_manual: false,
                created_at: end_ts.clone(),
                llm_cost: if llm_cost > 0.0 { Some(llm_cost) } else { None },
                llm_tokens: if llm_tokens > 0 { Some(llm_tokens) } else { None },
            };
            let seg_id = segment.id.clone();
            if let Err(e) = db.insert_segment(&segment) {
                log_sched!("DB insert segment failed: {e}");
                return;
            }
            log_sched!(
                "Segment saved to DB: id={}, category={}, duration={}s",
                seg_id,
                segment.category,
                segment.duration_secs
            );
            *current_segment_id = Some(seg_id);
            let _ = app_handle.emit("analysis:new-segment", &segment);
        }

        *last_category = Some(analysis.category.clone());

        // Emit status event
        let _ = app_handle.emit(
            "analysis:status",
            serde_json::json!({
                "status": "ok",
                "activity": analysis.activity,
                "category": analysis.category,
                "confidence": analysis.confidence,
                "timestamp": end_ts.clone(),
                "batch_size": batch.len(),
            }),
        );
    }

    // ── Batch poll loop ────────────────────────────────────────────────────

    /// Background loop: poll pending batches and process completed results.
    /// Each iteration's sync work is wrapped in catch_unwind to recover from panics.
    async fn poll_loop(
        pending_batches: Arc<Mutex<Vec<PendingBatch>>>,
        db: crate::db::Database,
        app_handle: tauri::AppHandle,
        is_running: Arc<AtomicBool>,
        is_paused: Arc<AtomicBool>,
    ) {
        log_sched!("Batch poll loop started");
        while is_running.load(Ordering::Acquire) {
            if is_paused.load(Ordering::Acquire) {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }

            // Snapshot current pending batch IDs (sync, can panic on lock poison)
            let batch_snapshot: Vec<(String, crate::config::LlmConfig)> =
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let guard = pending_batches.lock().unwrap();
                    guard
                        .iter()
                        .map(|b| (b.batch_id.clone(), b.adapter_config.clone()))
                        .collect::<Vec<_>>()
                })) {
                    Ok(snap) => snap,
                    Err(e) => {
                        let msg = format!("Lock panic in poll loop: {:?}", 
                            e.downcast_ref::<&str>().map(|s| s.to_string())
                                .or_else(|| e.downcast_ref::<String>().cloned())
                                .unwrap_or_else(|| "unknown".to_string()));
                        log_sched!("{msg}, recovering...");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                };

            if batch_snapshot.is_empty() {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }

            for (batch_id, cfg) in &batch_snapshot {
                log_sched!("Polling batch: id={}", batch_id);
                let adapter = KimiBatchAdapter::new(cfg.clone());

                match adapter.retrieve_batch(batch_id).await {
                    Ok(status) => {
                        log_sched!(
                            "Batch status: id={}, status={}, counts={:?}",
                            batch_id,
                            status.status,
                            status.request_counts,
                        );

                        match status.status.as_str() {
                            "completed" => {
                                log_sched!("Batch completed: id={}", batch_id);
                                let _ = app_handle.emit(
                                    "analysis:batch-completed",
                                    serde_json::json!({
                                        "batch_id": batch_id,
                                        "status": "completed",
                                    }),
                                );

                                if let Some(ref output_file) = status.output_file {
                                    match adapter.fetch_results(output_file).await {
                                        Ok(results) => {
                                            // process_batch_results has async parts but its critical
                                // sync section (lock) is already protected inside.
                                // We wrap only the sync entry into the function.
                                let p = pending_batches.clone();
                                let d = db.clone();
                                let a = app_handle.clone();
                                let bid = batch_id.clone();
                                Self::process_batch_results(
                                    &bid,
                                    results,
                                    &p,
                                    &d,
                                    &a,
                                )
                                .await;
                                        }
                                        Err(e) => {
                                            log_sched!(
                                                "Failed to fetch batch results for {}: {e}",
                                                batch_id
                                            );
                                        }
                                    }
                                } else {
                                    log_sched!("Batch {} completed but no output_file", batch_id);
                                }
                            }
                            "failed" | "cancelled" => {
                                log_sched!("Batch {} ended with status={}", batch_id, status.status);
                                let _ = app_handle.emit(
                                    "analysis:batch-failed",
                                    serde_json::json!({
                                        "batch_id": batch_id,
                                        "status": status.status,
                                        "errors": status.errors,
                                    }),
                                );
                                // Remove from pending list
                                if let Err(panic_err) = std::panic::catch_unwind(
                                    std::panic::AssertUnwindSafe(|| {
                                        let mut guard = pending_batches.lock().unwrap();
                                        guard.retain(|b| b.batch_id != *batch_id);
                                    }),
                                ) {
                                    let msg = extract_panic_msg(panic_err);
                                    log_sched!("PANIC removing failed batch {}: {msg}", batch_id);
                                }
                            }
                            "validating" | "in_progress" => {
                                log_sched!("Batch {} still in progress (status={})", batch_id, status.status);
                            }
                            other => {
                                log_sched!("Batch {} unknown status: {}", batch_id, other);
                            }
                        }
                    }
                    Err(e) => {
                        log_sched!("Failed to poll batch {}: {e}", batch_id);
                    }
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
        log_sched!("Batch poll loop exited");
    }

    /// Process completed batch results: parse each result, persist frames + segments.
    async fn process_batch_results(
        batch_id: &str,
        results: Vec<BatchCompletionResult>,
        pending_batches: &Arc<Mutex<Vec<PendingBatch>>>,
        db: &crate::db::Database,
        app_handle: &tauri::AppHandle,
    ) {
        // Find and remove the pending batch entry
        let pending = {
            let mut guard = pending_batches.lock().unwrap();
            let idx = guard.iter().position(|b| b.batch_id == batch_id);
            match idx {
                Some(i) => Some(guard.remove(i)),
                None => {
                    log_sched!("process_batch_results: batch {} not found in pending list", batch_id);
                    return;
                }
            }
        };

        let Some(pending) = pending else { return };
        log_sched!(
            "Processing {} results for batch {} ({} frames)",
            results.len(),
            batch_id,
            pending.frames.len()
        );

        // Track segment state across batch results (in a real batch, results[i] corresponds
        // to pending.frames[i], but we consolidate into one segment for simplicity).
        let mut last_category: Option<String> = None;
        let mut current_segment_id: Option<String> = None;

        // Accumulate all analysis results with their frames
        let mut frame_analyses: Vec<(PendingFrame, Option<crate::llm::adapter::AnalysisResult>, Option<crate::llm::adapter::UsageInfo>)> = Vec::new();

        for result in &results {
            if let Some(ref err) = result.error {
                log_sched!(
                    "Batch item {} failed: {}: {}",
                    result.custom_id,
                    err.code,
                    err.message
                );
                continue;
            }

            // Find the matching frame by custom_id
            let frame_idx = pending.frames.iter().position(|f| f.frame.id == result.custom_id);
            let frame = match frame_idx {
                Some(i) => pending.frames[i].clone(),
                None => {
                    log_sched!("No matching frame for custom_id={}", result.custom_id);
                    continue;
                }
            };

            // Parse the analysis from the response
            if let Some(choice) = result.response.body.choices.first() {
                match KimiBatchAdapter::parse_analysis_from_choice(choice) {
                    Ok(analysis) => {
                        let usage = KimiBatchAdapter::extract_usage(&result.response.body);
                        frame_analyses.push((frame, Some(analysis), usage));
                    }
                    Err(e) => {
                        log_sched!(
                            "Failed to parse analysis for custom_id={}: {e}",
                            result.custom_id
                        );
                        frame_analyses.push((frame, None, None));
                    }
                }
            } else {
                log_sched!(
                    "No choices in response for custom_id={}",
                    result.custom_id
                );
                frame_analyses.push((frame, None, None));
            }
        }

        if frame_analyses.is_empty() {
            log_sched!("No valid results in batch {}, skipping persistence", batch_id);
            return;
        }

        // Use the first successful analysis as the consolidated result.
        // Find index first, then extract by index to avoid borrow conflicts.
        let first_good_idx = frame_analyses.iter().position(|(_, a, _)| a.is_some());
        let first_good_idx = match first_good_idx {
            Some(i) => i,
            None => {
                log_sched!("No successful analysis in batch {}", batch_id);
                return;
            }
        };

        let (analysis, batch_cost, batch_tokens) = {
            let (_, ref a, ref usage) = frame_analyses[first_good_idx];
            // Log usage from the first successful result as representative
            if let Some(ref u) = usage {
                Self::persist_usage_batch(db, app_handle, Some(u), &pending.frames, &pending.model);
                let cost = crate::llm::adapter::estimate_cost(&pending.model, u);
                (a.clone().unwrap(), cost, u.total_tokens as i64)
            } else {
                (a.clone().unwrap(), 0.0, 0)
            }
        };

        // Build a combined batch for persistence (use all frames with the consolidated analysis)
        let batch_frames: Vec<PendingFrame> = frame_analyses
            .into_iter()
            .map(|(f, _, _)| f)
            .collect();

        // Calculate actual capture duration from frame timestamps
        let actual_duration = if batch_frames.len() >= 2 {
            let first = batch_frames.first().unwrap().frame.captured_at;
            let last = batch_frames.last().unwrap().frame.captured_at;
            let diff = last - first;
            std::time::Duration::from_secs(diff.num_seconds().max(30) as u64)
        } else {
            std::time::Duration::from_secs(30)
        };

        Self::persist_analysis(
            db,
            app_handle,
            &batch_frames,
            &analysis,
            actual_duration,
            &mut last_category,
            &mut current_segment_id,
            batch_cost,
            batch_tokens,
        )
        .await;

        log_sched!(
            "Batch {} processed: {} frames persisted, segment_id={:?}",
            batch_id,
            batch_frames.len(),
            current_segment_id,
        );
    }
}

// ── Helper functions ─────────────────────────────────────────────────────

/// Run one tick of the analysis loop with panic recovery.
///
/// Returns `true` if the tick completed normally, `false` if a panic was caught.
/// Mutable state (`pending_frames`, etc.) is reset to safe defaults on panic.
#[allow(clippy::too_many_arguments)]
async fn run_safe_tick(
    tick: u64,
    capture_duration: std::time::Duration,
    analysis_duration: std::time::Duration,
    pending_frames: &mut Vec<PendingFrame>,
    last_analysis: &mut std::time::Instant,
    last_activity: &mut Option<std::time::Instant>,
    last_category: &mut Option<String>,
    current_segment_id: &mut Option<String>,
    app_handle: &tauri::AppHandle,
    db: &crate::db::Database,
    screenshot_state: &crate::screen::ScreenshotState,
    is_paused: &Arc<AtomicBool>,
    pending_batches: &Arc<Mutex<Vec<PendingBatch>>>,
) -> bool {
    // Use a wrapper that runs the sync panic-prone parts under catch_unwind,
    // and the async parts normally. We separate the tick into phases where
    // phase boundaries are async .await points.
    //
    // The approach: build the sync work as closures, catch_unwind each,
    // and use the results for the async parts.

    log_sched!("Tick #{tick} starting");

    // 1. Pause check (no panic risk, trivial)
    if is_paused.load(Ordering::Acquire) {
        log_sched!("Tick #{tick} skipped (paused)");
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        return true;
    }

    // 2. Blocked app check (may panic in osascript, but that's a subprocess — fine)
    let should_skip = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let blocked_apps = crate::config::AppConfig::load_or_default()
            .privacy
            .blocked_apps;
        let frontmost = get_frontmost_app();
        frontmost.map_or(false, |app| blocked_apps.iter().any(|b| app.contains(b)))
    }));
    match should_skip {
        Ok(true) => {
            log_sched!("Skipped capture: frontmost app is blocked");
            tokio::time::sleep(capture_duration).await;
            return true;
        }
        Err(e) => {
            log_sched!("PANIC in blocked app check: {}", extract_panic_msg(e));
            return false;
        }
        Ok(false) => {}
    }

    // 3. Screenshot capture (CPU-bound, uses spawn_blocking — catch its JoinError)
    log_sched!("Capture attempt #{tick}");
    let dedup_threshold;
    let frame = {
        let cfg_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let persisted_cfg = crate::config::AppConfig::load_or_default();
            let cfg = crate::screen::ScreenshotConfig {
                capture_all_displays: persisted_cfg.screenshot.capture_all_displays,
                ..Default::default()
            };
            (cfg.dedup_threshold, cfg)
        }));
        let (dt, cfg) = match cfg_result {
            Ok(v) => v,
            Err(e) => {
                log_sched!("PANIC building screenshot config: {}", extract_panic_msg(e));
                return false;
            }
        };
        dedup_threshold = dt;

        match tokio::task::spawn_blocking(move || {
            // Inner catch_unwind: protect against panics in image processing
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::screen::take_screenshot(&cfg)
            }))
        })
        .await
        {
            Ok(Ok(Ok(f))) => {
                log_sched!("Frame captured: {} bytes, hash={}", f.jpeg_bytes.len(), f.hash);
                f
            }
            Ok(Ok(Err(e))) => {
                log_sched!("Screenshot failed: {e}");
                tokio::time::sleep(capture_duration).await;
                return true; // not a panic, just a capture failure
            }
            Ok(Err(panic_err)) => {
                log_sched!("PANIC in take_screenshot: {}", extract_panic_msg(panic_err));
                return false;
            }
            Err(join_err) => {
                log_sched!("Blocking task panicked: {join_err}");
                return false;
            }
        }
    };

    // 4. Dedup (lock on ScreenshotState is RwLock — no panic risk beyond poison)
    let is_dup_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let last = screenshot_state.get_last_hash();
        crate::screen::is_duplicate(&frame.hash, last.as_deref(), dedup_threshold)
    }));
    let is_dup = match is_dup_result {
        Ok(v) => v,
        Err(e) => {
            log_sched!("PANIC in dedup check: {}", extract_panic_msg(e));
            return false;
        }
    };

    log_sched!("Duplicate detection: similar={is_dup}, distance={dedup_threshold}");
    if is_dup {
        if let Some(ref seg_id) = current_segment_id {
            // DB access — catch panics from rusqlite internals
            let db_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                db.get_segment_by_id(seg_id)
            }));
            match db_result {
                Ok(Ok(Some(mut seg))) => {
                    let ts = frame.captured_at.format("%Y-%m-%d %H:%M:%S").to_string();
                    seg.end_time = ts;
                    seg.duration_secs += capture_duration.as_secs() as i64;
                    let _ = db.update_segment(&seg);
                }
                Ok(Err(e)) => log_sched!("DB error in dedup segment update: {e}"),
                Err(panic_err) => {
                    log_sched!("PANIC in DB dedup update: {}", extract_panic_msg(panic_err));
                    return false;
                }
                _ => {}
            }
        }
        let _ = app_handle.emit(
            "analysis:status",
            serde_json::json!({"status": "no_change", "last_seen": frame.captured_at.format("%Y-%m-%d %H:%M:%S").to_string()}),
        );
        log_sched!("Duplicate frame, skipped (waiting {}s)", capture_duration.as_secs());
        tokio::time::sleep(capture_duration).await;
        return true;
    }

    // Set last hash (RwLock write — catch poison)
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        screenshot_state.set_last_hash(frame.hash.clone())
    }));

    // 5. Base64 encode (CPU-bound via spawn_blocking)
    let jpeg_bytes = frame.jpeg_bytes.clone();
    let b64 = tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::image::encode_base64(&jpeg_bytes)
        }))
    })
    .await
    .unwrap_or_else(|_| Ok(String::new()))
    .unwrap_or_default();

    pending_frames.push(PendingFrame {
        frame,
        b64_cache: Some(b64),
    });
    log_sched!("Frame enqueued: pending_count={}", pending_frames.len());

    // 5.5 Idle detection: if the user was idle for a long time (>= half analysis_interval),
    //     treat this as a new session and reset the analysis timer.
    //     This prevents immediate analysis after wake-from-idle.
    let idle_threshold = analysis_duration / 2;
    let was_idle = last_activity
        .map(|t| t.elapsed() >= idle_threshold)
        .unwrap_or(true); // first frame after startup also counts as "new session"
    if was_idle {
        *last_analysis = std::time::Instant::now();
        log_sched!(
            "New session detected (was idle for >= {:?}), resetting analysis timer",
            idle_threshold
        );
    }
    *last_activity = Some(std::time::Instant::now());

    // 6. Decide whether to run a batch analysis
    let elapsed_since_analysis = last_analysis.elapsed();
    let time_to_analyze = elapsed_since_analysis >= analysis_duration;
    let buffer_full = pending_frames.len() >= MAX_IMAGES_PER_BATCH;
    let should_analyze = (time_to_analyze || buffer_full) && !pending_frames.is_empty();

    if should_analyze {
        log_sched!(
            "Starting batch analysis: reason={}, pending={}, elapsed={:?}",
            if buffer_full { "buffer_full" } else { "interval_elapsed" },
            pending_frames.len(),
            elapsed_since_analysis,
        );

        // Take all pending frames (drain the buffer)
        let batch: Vec<PendingFrame> = std::mem::take(pending_frames);

        // 7. Load LLM config fresh every time (no cache — cheap DB query)
        let (llm_config, use_batch) = match load_llm_config(db) {
            Some(cfg) => cfg,
            None => {
                log_sched!("No active LLM config, skipping batch");
                tokio::time::sleep(capture_duration).await;
                return true;
            }
        };

        let locale = crate::config::AppConfig::load_or_default().locale;
        let prompt = crate::config::get_batch_analysis_prompt(&locale);

        if use_batch {
            Scheduler::handle_batch_submit(
                &llm_config,
                batch,
                prompt,
                pending_batches,
                app_handle,
            )
            .await;
        } else {
            Scheduler::handle_realtime_analyze(
                db,
                app_handle,
                batch,
                prompt,
                capture_duration,
                last_category,
                current_segment_id,
            )
            .await;
        }

        *last_analysis = std::time::Instant::now();
    }

    log_sched!("Waiting {}s until next capture", capture_duration.as_secs());
    tokio::time::sleep(capture_duration).await;
    true
}

/// Extract a human-readable message from a panic payload.
fn extract_panic_msg(panic_payload: Box<dyn std::any::Any + Send>) -> String {
    panic_payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic_payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "Unknown panic".to_string())
}

/// Load the active LLM configuration from the database and determine
/// whether to use batch API or real-time API. Called on every batch
/// submission so configuration changes are picked up immediately.
fn load_llm_config(db: &crate::db::Database) -> Option<(crate::config::LlmConfig, bool)> {
    let db_config = match db.get_active_config() {
        Ok(Some(c)) => c,
        _ => return None,
    };
    let llm_cfg = crate::config::LlmConfig {
        name: db_config.name.clone(),
        provider: db_config.provider.clone(),
        base_url: db_config.base_url.clone(),
        model: db_config.model.clone(),
        api_key: db_config.api_key.clone(),
        max_tokens: db_config.max_tokens as u32,
        is_active: true,
        use_batch_api: db_config.use_batch_api.unwrap_or(false),
    };
    let batch_flag = llm_cfg.use_batch_api
        && (llm_cfg.provider.to_lowercase() == "openai"
            || llm_cfg.provider.to_lowercase() == "custom");
    Some((llm_cfg, batch_flag))
}

/// Extract the model name from an `LlmAdapter` trait object.
#[allow(dead_code)]
fn get_adapter_model(adapter: &dyn crate::llm::adapter::LlmAdapter) -> String {
    adapter.model_name().to_string()
}

/// Extract the provider name from an `LlmAdapter` trait object.
#[allow(dead_code)]
fn get_adapter_provider(adapter: &dyn crate::llm::adapter::LlmAdapter) -> String {
    adapter.provider_name().to_string()
}

/// Get the name of the frontmost (active) application.
#[cfg(target_os = "macos")]
fn get_frontmost_app() -> Option<String> {
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get name of first application process whose frontmost is true",
        ])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn get_frontmost_app() -> Option<String> {
    active_win_pos_rs::get_active_window()
        .ok()
        .map(|w| w.app_name)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn get_frontmost_app() -> Option<String> {
    // Linux: could use xdotool, but return None for now (no blocklist checking)
    None
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

// Safety: all internal state is behind atomics or mutexes.
unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}
