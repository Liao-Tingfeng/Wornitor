//! Error types for the screen capture module.

use thiserror::Error;

/// Errors that can occur during screen capture and processing.
#[derive(Debug, Error)]
pub enum ScreenError {
    /// Screen Recording permission has not been granted by the user.
    #[error("Screen Recording permission denied")]
    PermissionDenied,

    /// The capture API reported a failure.
    #[error("Failed to capture screen: {0}")]
    CaptureFailed(String),

    /// An image processing operation failed.
    #[error("Image processing error: {0}")]
    ImageError(String),

    /// An I/O error (file read/write, directory creation, etc.).
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// No display / monitor found on the system.
    #[error("No display found")]
    NoDisplayFound,
}
