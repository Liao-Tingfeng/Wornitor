//! Cross-platform screen capture via `xcap` crate.
//!
//! Supports Windows (DXGI), macOS (CoreGraphics), and Linux (X11 SHM).

use super::ScreenError;

/// Helper: swap B and R channels in-place (xcap returns BGRA on some platforms).
fn bgra_to_rgba(pixels: &mut [u8]) {
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
}

/// Capture the main display and return raw RGBA pixel data + dimensions.
pub fn capture_main_display(_include_cursor: bool) -> Result<(Vec<u8>, u32, u32), ScreenError> {
    let monitors =
        xcap::Monitor::all().map_err(|e| ScreenError::CaptureFailed(format!("xcap: {e}")))?;
    let monitor = monitors
        .first()
        .ok_or(ScreenError::CaptureFailed("No display found".into()))?;
    let image = monitor
        .capture_image()
        .map_err(|e| ScreenError::CaptureFailed(format!("xcap capture: {e}")))?;

    let (w, h) = (image.width(), image.height());
    let mut pixels = image.as_raw().to_vec();

    // xcap returns BGRA on macOS; swap to RGBA
    #[cfg(target_os = "macos")]
    bgra_to_rgba(&mut pixels);

    eprintln!(
        "[SCREEN] Main display captured: {}x{}, {} bytes",
        w,
        h,
        pixels.len()
    );
    Ok((pixels, w, h))
}

/// Capture all connected displays and return a horizontally-stitched RGBA image.
pub fn capture_all_displays(_include_cursor: bool) -> Result<(Vec<u8>, u32, u32), ScreenError> {
    let monitors =
        xcap::Monitor::all().map_err(|e| ScreenError::CaptureFailed(format!("xcap: {e}")))?;
    if monitors.is_empty() {
        return Err(ScreenError::CaptureFailed("No displays found".into()));
    }
    if monitors.len() == 1 {
        return capture_main_display(_include_cursor);
    }

    // Capture each monitor and stitch horizontally
    let mut images: Vec<(Vec<u8>, u32, u32)> = Vec::new();
    for monitor in &monitors {
        let img = monitor
            .capture_image()
            .map_err(|e| ScreenError::CaptureFailed(format!("xcap: {e}")))?;
        let (w, h) = (img.width(), img.height());
        let mut pixels = img.as_raw().to_vec();

        #[cfg(target_os = "macos")]
        bgra_to_rgba(&mut pixels);

        images.push((pixels, w, h));
    }

    let total_width: u32 = images.iter().map(|(_, w, _)| w).sum();
    let max_height: u32 = *images.iter().map(|(_, _, h)| h).max().unwrap_or(&0);
    let mut combined = vec![0u8; (total_width as usize) * (max_height as usize) * 4];

    let mut x_offset = 0;
    for (pixels, w, h) in &images {
        for y in 0..*h {
            let src_start = (y * w * 4) as usize;
            let src_end = src_start + (*w * 4) as usize;
            let dst_start = ((y * total_width + x_offset) * 4) as usize;
            let dst_end = dst_start + (*w * 4) as usize;
            if dst_end <= combined.len() && src_end <= pixels.len() {
                combined[dst_start..dst_end].copy_from_slice(&pixels[src_start..src_end]);
            }
        }
        x_offset += w;
    }

    eprintln!(
        "[SCREEN] All displays ({}): {}x{}, {} bytes",
        monitors.len(),
        total_width,
        max_height,
        combined.len(),
    );
    Ok((combined, total_width, max_height))
}
