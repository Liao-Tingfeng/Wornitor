mod commands;
mod config;
mod db;
mod image;
mod llm;
mod scheduler;
mod screen;
mod tray;

use db::Database;
use tray::TrayState;

// ── Application State ───────────────────────────────────────────

pub struct AppState {
    pub db: Database,
    pub screenshot_state: screen::ScreenshotState,
    pub scheduler: scheduler::Scheduler,
    pub tray_state: TrayState,
}

// ═════════════════════════════════════════════════════════════════
//  DB query helpers (non-command, shared across commands)
// ═════════════════════════════════════════════════════════════════

// All IPC commands are defined in the `commands` module tree.
// This file only registers them and wires up the application.

// ── Application Entry ───────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_path = dirs_db_path().unwrap_or_else(|| "wornitor.db".to_string());
    eprintln!("[APP] Wornitor starting, data dir: {db_path}");
    let database = Database::new(&db_path).expect("Failed to initialize database");
    let screenshot_state = screen::ScreenshotState::default();
    let tray_state = TrayState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            db: database,
            screenshot_state,
            scheduler: scheduler::Scheduler::new(),
            tray_state: tray_state.clone(),
        })
        .setup(move |app| {
            eprintln!("[APP] Tauri builder initialized");
            #[cfg(desktop)]
            if let Err(e) = tray::create_tray(app.handle(), tray_state) {
                eprintln!("[TRAY] Failed to create system tray: {e}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Screenshot commands
            commands::screenshot::take_screenshot,
            // Config commands
            commands::config::get_config,
            commands::config::update_config,
            commands::config::get_llm_config_presets,
            commands::config::save_llm_config,
            commands::config::delete_llm_config,
            commands::config::test_llm_connection,
            commands::config::list_llm_models,
            commands::config::get_analysis_prompt,
            commands::config::set_locale,
            // Report commands
            commands::report::generate_report,
            commands::report::generate_daily_report,
            commands::report::get_report_data,
            commands::report::generate_report_summary,
            commands::report::get_daily_summary,
            commands::report::get_activity_breakdown,
            // Analysis commands
            commands::analysis::trigger_analysis,
            commands::analysis::start_recording,
            commands::analysis::stop_recording,
            commands::analysis::pause_recording,
            commands::analysis::resume_recording,
            commands::analysis::get_recording_status,
            commands::analysis::get_today_timeline,
            commands::analysis::merge_segments,
            commands::analysis::update_segment,
            commands::analysis::delete_segment,
            commands::analysis::add_manual_segment,
            // DB query commands
            commands::db::get_segments_by_date,
            commands::db::get_segments_in_range,
            commands::db::get_screenshot,
            commands::db::get_screenshots_in_range,
            commands::db::get_screenshots_by_ids,
            commands::db::get_recent_screenshots,
            commands::db::get_daily_summaries_in_range,
            commands::db::get_period_summaries_by_type,
            commands::db::list_llm_configs,
            commands::db::get_active_llm_config,
            commands::db::get_active_privacy_rules,
            commands::db::get_category_breakdown,
            commands::db::get_daily_trend,
            commands::db::clean_old_data,
            // Cost / usage commands
            commands::db::get_daily_usage,
            commands::db::get_monthly_usage,
            generate_weekly_report,
            generate_monthly_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Determine a suitable database path for the current platform.
pub(crate) fn dirs_db_path() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").ok()?;
        let dir = format!("{home}/Library/Application Support/com.wornitor.app");
        let _ = std::fs::create_dir_all(&dir);
        Some(format!("{dir}/wornitor.db"))
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").ok()?;
        let dir = format!("{home}/.local/share/wornitor");
        let _ = std::fs::create_dir_all(&dir);
        Some(format!("{dir}/wornitor.db"))
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").ok()?;
        let dir = format!("{appdata}\\Wornitor");
        let _ = std::fs::create_dir_all(&dir);
        Some(format!("{dir}\\wornitor.db"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

#[tauri::command]
async fn generate_weekly_report(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
    end_date: String,
) -> Result<String, String> {
    commands::report::generate_weekly_report_impl(state.inner(), &app_handle, &end_date).await
}

#[tauri::command]
async fn generate_monthly_report(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
    year: i32,
    month: u32,
) -> Result<String, String> {
    commands::report::generate_monthly_report_impl(state.inner(), &app_handle, year, month).await
}
