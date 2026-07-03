//! Image processing utilities for screenshot module.
//!
//! Provides JPEG compression, resize, dHash perceptual hashing,
//! Hamming distance, base64 encoding, and region blur (for privacy).

use image::{imageops::FilterType, DynamicImage, GenericImageView};

/// Compress an image to JPEG bytes.
///
/// First resizes to `max_width` (preserving aspect ratio), then encodes as JPEG
/// with the given `quality` (1–100, where ~85 is a good balance).
pub fn compress_jpeg(img: &DynamicImage, quality: u8, max_width: u32) -> Vec<u8> {
    use image::codecs::jpeg::JpegEncoder;
    let resized = resize_image(img, max_width);
    let rgb = resized.to_rgb8(); // JPEG does not support alpha
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    encoder
        .encode(
            &rgb,
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .expect("JPEG encoding should not fail");
    buf.into_inner()
}

/// Resize the image so that its width does not exceed `max_width`.
///
/// The aspect ratio is preserved. If the image is already narrower than
/// `max_width`, a clone is returned unchanged.
pub fn resize_image(img: &DynamicImage, max_width: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    if w <= max_width {
        return img.clone();
    }
    let new_h = (h as f64 * max_width as f64 / w as f64).round() as u32;
    img.resize_exact(max_width, new_h.max(1), FilterType::Lanczos3)
}

/// Compute a dHash (difference hash) of the image and return it as a hex string.
///
/// dHash produces a 64-bit hash by resizing to 9×8, converting to greyscale,
/// and comparing adjacent horizontal pixels.  The hex string is 16 characters
/// (lowercase).  See also: <http://www.hackerfactor.com/blog/index.php?/archives/529-Kind-of-Like-That.html>
pub fn compute_dhash(img: &DynamicImage) -> String {
    // Reduce colour to greyscale and resize to 9×8 (9 wide, 8 tall).
    let grey = img.grayscale().resize_exact(9, 8, FilterType::Lanczos3);
    let pixels = grey.to_luma8();

    let mut bits: u64 = 0;
    let mut idx = 0;
    for y in 0..8 {
        for x in 0..8 {
            let left = pixels.get_pixel(x, y)[0];
            let right = pixels.get_pixel(x + 1, y)[0];
            if left > right {
                bits |= 1 << idx;
            }
            idx += 1;
        }
    }
    format!("{:016x}", bits)
}

/// Compute the Hamming distance between two dHash hex strings.
///
/// Both strings should be 16-character hex strings produced by `compute_dhash`.
/// Returns the number of differing bits.
pub fn hamming_distance(hash1: &str, hash2: &str) -> u32 {
    // Parse both hex strings as u64.
    let a = u64::from_str_radix(hash1, 16).unwrap_or(0);
    let b = u64::from_str_radix(hash2, 16).unwrap_or(0);
    let xor = a ^ b;
    xor.count_ones()
}

/// Encode bytes as a standard base64 string (WITH padding).
///
/// Some LLM API providers require standard base64 with padding (`=` suffixes)
/// and reject `STANDARD_NO_PAD`.  We therefore use `STANDARD` here.
pub fn encode_base64(bytes: &[u8]) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    // Debug: print first 50 and last 20 chars so we can verify the output
    let preview = if encoded.len() > 70 {
        format!("{}...{}", &encoded[..50], &encoded[encoded.len() - 20..])
    } else {
        encoded.clone()
    };
    eprintln!(
        "[IMAGE] encode_base64: {} bytes in → {} chars out, preview={preview}",
        bytes.len(),
        encoded.len()
    );
    encoded
}

/// Apply a Gaussian blur to a rectangular region of the image.
///
/// This is intended for future privacy features (e.g., blurring detected
/// credentials or faces before sending frames to an LLM).
///
/// `sigma` controls the blur intensity (typical range 2.0–10.0).
pub fn apply_blur_region(img: &mut DynamicImage, x: u32, y: u32, w: u32, h: u32, sigma: f32) {
    use image::imageops::blur;
    // Clip to image bounds.
    let (img_w, img_h) = img.dimensions();
    let x = x.min(img_w.saturating_sub(1));
    let y = y.min(img_h.saturating_sub(1));
    let w = w.min(img_w - x);
    let h = h.min(img_h - y);
    if w == 0 || h == 0 {
        return;
    }

    // Extract the region as a standalone RGBA image, blur it, then paste back.
    let mut rgba = img.to_rgba8();
    let region: image::RgbaImage =
        image::ImageBuffer::from_fn(w, h, |dx, dy| *rgba.get_pixel(x + dx, y + dy));
    let blurred = blur(&region, sigma);
    for dy in 0..h {
        for dx in 0..w {
            let px = blurred.get_pixel(dx, dy);
            rgba.put_pixel(x + dx, y + dy, *px);
        }
    }
    *img = DynamicImage::ImageRgba8(rgba);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    fn make_test_image(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            w,
            h,
            image::Rgba([128, 64, 200, 255]),
        ))
    }

    #[test]
    fn test_resize_noop_when_smaller() {
        let img = make_test_image(100, 50);
        let resized = resize_image(&img, 200);
        assert_eq!(resized.dimensions(), (100, 50));
    }

    #[test]
    fn test_resize_downscales_proportionally() {
        let img = make_test_image(2000, 1000);
        let resized = resize_image(&img, 800);
        let (w, h) = resized.dimensions();
        assert_eq!(w, 800);
        assert_eq!(h, 400);
    }

    #[test]
    fn test_dhash_is_stable() {
        let img = make_test_image(64, 64);
        let h1 = compute_dhash(&img);
        let h2 = compute_dhash(&img);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn test_hamming_distance_same() {
        let h = "abcd1234abcd1234";
        assert_eq!(hamming_distance(h, h), 0);
    }

    #[test]
    fn test_hamming_distance_different() {
        let a = "0000000000000000";
        let b = "ffffffffffffffff";
        assert_eq!(hamming_distance(a, b), 64);
    }

    #[test]
    fn test_compress_jpeg_returns_bytes() {
        let img = make_test_image(100, 100);
        let bytes = compress_jpeg(&img, 85, 200);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = b"hello world";
        let b64 = encode_base64(data);
        assert!(!b64.is_empty());
        assert!(
            b64.ends_with("="),
            "STANDARD encoding should end with padding"
        );
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_blur_region_no_panic() {
        let mut img = make_test_image(200, 200);
        apply_blur_region(&mut img, 10, 10, 50, 50, 3.0);
        // Just check no crash.
        assert!(img.width() > 0);
    }

    #[test]
    fn test_blur_region_out_of_bounds_clips() {
        let mut img = make_test_image(100, 100);
        apply_blur_region(&mut img, 90, 90, 100, 100, 3.0);
        assert!(img.width() > 0);
    }
}
