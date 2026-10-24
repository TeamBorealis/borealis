//! Downscale oversized portraits when a character is imported, so a multi-megabyte picture from
//! a legacy file is not stored and re-embedded at full size.

use image::{ImageFormat, ImageReader, Limits};
use std::io::Cursor;

/// Longest edge, in pixels, a stored portrait keeps. Larger pictures are scaled down to fit;
/// smaller ones are left byte-for-byte untouched. 1024 px stays sharp when printed at 300 dpi
/// (about 3.4 inches) and on the sheet's largest portrait slot.
pub const MAX_PORTRAIT_EDGE_PX: u32 = 1024;

/// Largest decoded portrait buffer accepted (guards against decompression bombs).
const MAX_DECODE_ALLOC_BYTES: u64 = 256 * 1024 * 1024;

/// JPEG quality used when a JPEG portrait is re-encoded after scaling.
const JPEG_QUALITY: u8 = 88;

/// A portrait re-encoded at a smaller size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizedPortrait {
    /// New file extension without the dot (`jpg` or `png`).
    pub extension: &'static str,
    /// Encoded bytes.
    pub bytes: Vec<u8>,
}

/// Scale `bytes` down so neither edge exceeds [`MAX_PORTRAIT_EDGE_PX`].
///
/// Returns `None` when nothing should change: the image already fits, the format is not
/// PNG/JPEG/WebP, or it cannot be decoded or encoded (the original is then kept as is). JPEG stays
/// JPEG; PNG and WebP become PNG (the image crate encodes WebP lossless only, and PNG keeps
/// transparency).
#[must_use]
pub fn downscale_portrait(bytes: &[u8]) -> Option<ResizedPortrait> {
    let format = image::guess_format(bytes).ok()?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP
    ) {
        return None;
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    if decoded.width() <= MAX_PORTRAIT_EDGE_PX && decoded.height() <= MAX_PORTRAIT_EDGE_PX {
        return None;
    }
    let scaled = decoded.resize(
        MAX_PORTRAIT_EDGE_PX,
        MAX_PORTRAIT_EDGE_PX,
        image::imageops::FilterType::Lanczos3,
    );
    let mut out = Cursor::new(Vec::new());
    if format == ImageFormat::Jpeg {
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
        scaled.to_rgb8().write_with_encoder(encoder).ok()?;
        return Some(ResizedPortrait {
            extension: "jpg",
            bytes: out.into_inner(),
        });
    }
    scaled.write_to(&mut out, ImageFormat::Png).ok()?;
    Some(ResizedPortrait {
        extension: "png",
        bytes: out.into_inner(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbaImage};

    fn encode(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            width,
            height,
            image::Rgba([200, 30, 30, 255]),
        ));
        let mut out = Cursor::new(Vec::new());
        match format {
            ImageFormat::Jpeg => img.to_rgb8().write_to(&mut out, format).unwrap(),
            _ => img.write_to(&mut out, format).unwrap(),
        }
        out.into_inner()
    }

    fn dims(bytes: &[u8]) -> (u32, u32) {
        let img = image::load_from_memory(bytes).unwrap();
        (img.width(), img.height())
    }

    #[test]
    fn small_portraits_are_left_alone() {
        assert!(downscale_portrait(&encode(300, 200, ImageFormat::Png)).is_none());
    }

    #[test]
    fn large_png_is_scaled_to_the_cap_keeping_aspect_ratio() {
        let out = downscale_portrait(&encode(2048, 1024, ImageFormat::Png)).unwrap();
        assert_eq!(out.extension, "png");
        assert_eq!(
            dims(&out.bytes),
            (MAX_PORTRAIT_EDGE_PX, MAX_PORTRAIT_EDGE_PX / 2)
        );
    }

    #[test]
    fn large_jpeg_stays_jpeg() {
        let out = downscale_portrait(&encode(1500, 1500, ImageFormat::Jpeg)).unwrap();
        assert_eq!(out.extension, "jpg");
        assert_eq!(
            dims(&out.bytes),
            (MAX_PORTRAIT_EDGE_PX, MAX_PORTRAIT_EDGE_PX)
        );
    }

    #[test]
    fn large_webp_is_scaled_and_stored_as_png() {
        let out = downscale_portrait(&encode(2048, 1024, ImageFormat::WebP)).unwrap();
        assert_eq!(out.extension, "png");
        assert_eq!(
            dims(&out.bytes),
            (MAX_PORTRAIT_EDGE_PX, MAX_PORTRAIT_EDGE_PX / 2)
        );
    }

    #[test]
    fn small_webp_is_left_alone() {
        assert!(downscale_portrait(&encode(300, 200, ImageFormat::WebP)).is_none());
    }

    #[test]
    fn garbage_is_left_alone() {
        assert!(downscale_portrait(b"not an image").is_none());
    }
}
