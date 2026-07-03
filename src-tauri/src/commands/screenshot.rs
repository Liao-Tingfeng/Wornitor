//! Tauri IPC commands for the screenshot module.
//!
//! Exposes: `take_screenshot`.

use tauri::State;

use crate::screen::{
    save_to_disk, take_screenshot as do_take_screenshot, ScreenshotConfig, ScreenshotFrame,
};

// ---------------------------------------------------------------------------
// Response types for IPC
// ---------------------------------------------------------------------------

/// Simplified result sent to the frontend after a screenshot.
#[derive(Debug, serde::Serialize)]
pub struct ScreenshotResult {
    pub id: String,
    pub captured_at: String,
    pub file_path: String,
    pub width: u32,
    pub height: u32,
    pub hash: String,
    /// Base64-encoded JPEG bytes (for LLM transmission).
    pub jpeg_base64: String,
}

impl From<ScreenshotFrame> for ScreenshotResult {
    fn from(frame: ScreenshotFrame) -> Self {
        let jpeg_base64 = crate::image::encode_base64(&frame.jpeg_bytes);
        Self {
            id: frame.id,
            captured_at: frame.captured_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            file_path: frame.file_path,
            width: frame.width,
            height: frame.height,
            hash: frame.hash,
            jpeg_base64,
        }
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Take a single screenshot and return the result to the frontend immediately.
#[tauri::command]
pub async fn take_screenshot(state: State<'_, crate::AppState>) -> Result<ScreenshotResult, String> {
    eprintln!("[CMD] take_screenshot called");
    let config = ScreenshotConfig::default();
    let mut frame = do_take_screenshot(&config).map_err(|e| e.to_string())?;

    // Persist to disk using the db path parent as base.
    let base_dir = crate::dirs_db_path()
        .map(std::path::PathBuf::from)
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let path = save_to_disk(&frame, &base_dir).map_err(|e| e.to_string())?;
    frame.file_path = path;

    // Update last hash for dedup.
    state.screenshot_state.set_last_hash(frame.hash.clone());

    Ok(ScreenshotResult::from(frame))
}
