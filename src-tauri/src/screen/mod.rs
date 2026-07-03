//! Screen capture module — cross-platform screenshot logic using xcap.
//!
//! Provides:
//! - [`ScreenshotConfig`] – capture parameters
//! - [`ScreenshotFrame`] – a single captured frame with metadata
//! - [`ScreenshotState`] – shared runtime state (pause flag + last-hash for dedup)
//! - [`take_screenshot`] – the main capture function
//! - [`is_duplicate`] – perceptual-hash based dedup
//! - [`save_to_disk`] – frame persistence on disk

use std::path::Path;

use chrono::{Datelike, Local};
use image::GenericImageView;
use uuid::Uuid;

use crate::image as img;
use image::RgbaImage;

mod capture;
mod error;
mod state;

pub use error::ScreenError;
pub use state::ScreenshotState;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the screenshot capture routine.
#[derive(Debug, Clone)]
pub struct ScreenshotConfig {
    /// Interval between captures (seconds).  Default: 30.
    pub interval_secs: u32,
    /// JPEG compression quality (1–100).  Default: 85.
    pub jpeg_quality: u8,
    /// Maximum width of the output image (pixels); height is scaled to
    /// preserve aspect ratio.  Default: 1200.
    pub max_width: u32,
    /// Whether to include the mouse cursor in the capture.  Default: true.
    pub include_cursor: bool,
    /// Hamming-distance threshold for dHash dedup.
    /// Frames whose hash distance is ≤ this value are considered duplicates.
    /// Default: 5.
    pub dedup_threshold: u32,
    /// Capture all connected displays and stitch them together.
    /// When false (default), only the main display is captured.
    pub capture_all_displays: bool,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            interval_secs: 30,
            jpeg_quality: 85,
            max_width: 1200,
            include_cursor: true,
            dedup_threshold: 5,
            capture_all_displays: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Captured frame
// ---------------------------------------------------------------------------

/// A single screenshot frame with metadata.
#[derive(Debug, Clone)]
pub struct ScreenshotFrame {
    /// UUID v4 identifier for this frame.
    pub id: String,
    /// Timestamp of capture (local time).
    pub captured_at: chrono::NaiveDateTime,
    /// Absolute file path on disk.
    pub file_path: String,
    /// Image width (pixels).
    pub width: u32,
    /// Image height (pixels).
    pub height: u32,
    /// dHash perceptual hash (16-char hex string).
    pub hash: String,
    /// JPEG-encoded image bytes (ready for LLM transfer via base64).
    pub jpeg_bytes: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Core functions
// ---------------------------------------------------------------------------

/// Capture the main display and return a [`ScreenshotFrame`].
///
/// Uses `/usr/sbin/screencapture` (macOS built-in CLI) to capture the primary
/// display, then:
/// 1. Scales the image to `config.max_width` (preserving aspect ratio).
/// 2. JPEG-encodes with the given quality.
/// 3. Computes a dHash for dedup.
pub fn take_screenshot(config: &ScreenshotConfig) -> Result<ScreenshotFrame, ScreenError> {
    // 1. Capture raw frame from the display(s).
    let (raw_bytes, raw_width, raw_height) = if config.capture_all_displays {
        capture::capture_all_displays(config.include_cursor)?
    } else {
        capture::capture_main_display(config.include_cursor)?
    };

    // 2. Create a DynamicImage from the raw RGBA bytes.
    let buffer = RgbaImage::from_raw(raw_width, raw_height, raw_bytes).ok_or_else(|| {
        ScreenError::ImageError("Failed to create image buffer from raw capture".into())
    })?;
    let dyn_img = image::DynamicImage::ImageRgba8(buffer);

    // 3. Resize and compress.
    let jpeg_bytes = img::compress_jpeg(&dyn_img, config.jpeg_quality, config.max_width);

    // 4. Compute dHash (on the resized image for consistency).
    let resized = img::resize_image(&dyn_img, config.max_width);
    let hash = img::compute_dhash(&resized);
    let (final_w, final_h) = resized.dimensions();

    // 5. Build frame metadata.
    let now = Local::now().naive_local();
    let frame = ScreenshotFrame {
        id: Uuid::new_v4().to_string(),
        captured_at: now,
        file_path: String::new(), // filled by save_to_disk
        width: final_w,
        height: final_h,
        hash,
        jpeg_bytes,
    };

    Ok(frame)
}

/// Determine whether two frames are duplicates based on perceptual hash.
///
/// Returns `true` when the Hamming distance between `new_hash` and
/// `last_hash` (when present) is ≤ `threshold`.
pub fn is_duplicate(new_hash: &str, last_hash: Option<&str>, threshold: u32) -> bool {
    match last_hash {
        Some(old) => img::hamming_distance(new_hash, old) <= threshold,
        None => false,
    }
}

/// Persist a [`ScreenshotFrame`] to disk.
///
/// Directory layout: `{base_path}/screenshots/YYYY/MM/DD/{uuid}.jpg`
///
/// On success returns the absolute path of the written file and updates
/// `frame.file_path` in place (via interior mutability the caller sees it).
pub fn save_to_disk(frame: &ScreenshotFrame, base_path: &Path) -> Result<String, ScreenError> {
    let date = frame.captured_at;
    let dir = base_path
        .join("screenshots")
        .join(format!("{:04}", date.year()))
        .join(format!("{:02}", date.month()))
        .join(format!("{:02}", date.day()));

    std::fs::create_dir_all(&dir)?;

    let file_name = format!("{}.jpg", frame.id);
    let file_path = dir.join(&file_name);
    std::fs::write(&file_path, &frame.jpeg_bytes)?;

    Ok(file_path.to_string_lossy().into_owned())
}

/// Remove screenshot files older than retention_days from disk.
/// Returns the number of files deleted.
pub fn clean_screenshot_files(
    base_path: &std::path::Path,
    retention_days: u32,
) -> Result<u64, ScreenError> {
    let screenshots_dir = base_path.join("screenshots");
    if !screenshots_dir.exists() {
        return Ok(0);
    }
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(
            retention_days as u64 * 86400,
        ))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let mut count: u64 = 0;
    fn walk_and_clean(
        dir: &std::path::Path,
        cutoff: std::time::SystemTime,
        count: &mut u64,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk_and_clean(&path, cutoff, count)?;
                // Try to remove empty dir
                let _ = std::fs::remove_dir(&path);
            } else if path.extension().map_or(false, |e| e == "jpg") {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if modified < cutoff {
                            std::fs::remove_file(&path)?;
                            *count += 1;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    let _ = walk_and_clean(&screenshots_dir, cutoff, &mut count);
    Ok(count)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_duplicate_no_previous() {
        assert!(!is_duplicate("abc", None, 5));
    }

    #[test]
    fn test_is_duplicate_exact_match() {
        let h = "abcd1234abcd1234";
        assert!(is_duplicate(h, Some(h), 5));
    }

    #[test]
    fn test_is_duplicate_below_threshold() {
        // Purposely very close hashes (only last nibble differs).
        let a = "abcd1234abcd1230";
        let b = "abcd1234abcd123f";
        // These differ by potentially many bits, but we just verify the API works.
        let dist = crate::image::hamming_distance(a, b);
        assert_eq!(is_duplicate(a, Some(b), dist), true);
        assert_eq!(is_duplicate(a, Some(b), dist - 1), false);
    }

    #[test]
    fn test_save_to_disk_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let frame = ScreenshotFrame {
            id: "test-uuid".into(),
            captured_at: chrono::NaiveDateTime::parse_from_str(
                "2025-01-15 10:30:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            file_path: String::new(),
            width: 800,
            height: 600,
            hash: "abcd1234abcd1234".into(),
            jpeg_bytes: vec![0xff, 0xd8, 0xff, 0xe0], // fake JPEG header
        };

        let path = save_to_disk(&frame, dir.path()).unwrap();
        assert!(path.ends_with("screenshots/2025/01/15/test-uuid.jpg"));
        assert!(std::fs::exists(&path).unwrap_or(false));
    }

    #[test]
    fn test_config_defaults() {
        let cfg = ScreenshotConfig::default();
        assert_eq!(cfg.interval_secs, 30);
        assert_eq!(cfg.jpeg_quality, 85);
        assert_eq!(cfg.max_width, 1200);
        assert!(cfg.include_cursor);
        assert_eq!(cfg.dedup_threshold, 5);
        assert!(!cfg.capture_all_displays);
    }
}
