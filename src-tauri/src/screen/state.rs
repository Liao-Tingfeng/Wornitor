//! Shared runtime state for the screenshot subsystem.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Thread-safe shared state used by the screenshot capture loop and Tauri
/// commands.
///
/// This is stored inside `crate::AppState` and registered as Tauri managed state
/// so that IPC commands can read/update the last-frame hash for dedup.
#[derive(Clone)]
pub struct ScreenshotState {
    inner: Arc<ScreenshotStateInner>,
}

struct ScreenshotStateInner {
    /// Whether capture is paused.  Atomic so it can be read from the
    /// capture loop without blocking.
    pub is_paused: AtomicBool,

    /// dHash hex string of the most recently captured frame.
    pub last_hash: Mutex<Option<String>>,
}

impl ScreenshotState {
    /// Create a new state with capture unpaused and no previous hash.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ScreenshotStateInner {
                is_paused: AtomicBool::new(false),
                last_hash: Mutex::new(None),
            }),
        }
    }

    /// Check whether capture is currently paused.
    pub fn paused(&self) -> bool {
        self.inner.is_paused.load(Ordering::Acquire)
    }

    /// Pause the capture loop.
    pub fn pause(&self) {
        self.inner.is_paused.store(true, Ordering::Release);
    }

    /// Resume the capture loop.
    pub fn resume(&self) {
        self.inner.is_paused.store(false, Ordering::Release);
    }

    /// Returns `true` if a background capture task is currently running.
    /// Note: this is a best-effort indicator; the scheduler now manages the loop.
    pub fn is_running(&self) -> bool {
        false
    }

    /// Set the last-captured hash value.
    pub fn set_last_hash(&self, hash: String) {
        let mut guard = self.inner.last_hash.lock().unwrap();
        *guard = Some(hash);
    }

    /// Get the last-captured hash value, if any.
    pub fn get_last_hash(&self) -> Option<String> {
        let guard = self.inner.last_hash.lock().unwrap();
        guard.clone()
    }

    /// Clear error state (no-op, kept for API consistency).
    pub fn clear_error(&self) {}

    /// Return a clone of the inner `Arc` for sharing across threads.
    pub fn clone_arc(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl Default for ScreenshotState {
    fn default() -> Self {
        Self::new()
    }
}
