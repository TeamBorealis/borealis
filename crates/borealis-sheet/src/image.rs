//! Portrait decoding for the PDF: JPEG passes through (DCT), PNG is decoded to raw 8-bit samples
//! (written FlateDecode) plus an optional alpha plane (written as an `SMask`).

use png::{BitDepth, ColorType, Decoder, Limits, Transformations};
use std::io::Cursor;

/// Largest decoded portrait buffer accepted (guards against decompression bombs).
const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;

/// How the samples in [`PdfImage::data`] are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Samples {
    /// A complete JPEG file, embedded with `DCTDecode`.
    Jpeg,
    /// Raw 8-bit samples, compressed with `FlateDecode` when written.
    Raw,
}

/// An image ready to be written as a PDF XObject.
#[derive(Debug, Clone)]
pub(crate) struct PdfImage {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// 1 (DeviceGray) or 3 (DeviceRGB).
    pub components: u8,
    pub samples: Samples,
    /// One 8-bit alpha value per pixel, when the source had transparency.
    pub alpha: Option<Vec<u8>>,
}

/// Read the pixel size and component count from a JPEG's start-of-frame marker.
fn jpeg_info(data: &[u8]) -> Option<(u32, u32, u8)> {
    let mut i = 2;
    while i + 9 < data.len() {
        if data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        let len = usize::from(u16::from_be_bytes([data[i + 2], data[i + 3]]));
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let h = u32::from(u16::from_be_bytes([data[i + 5], data[i + 6]]));
            let w = u32::from(u16::from_be_bytes([data[i + 7], data[i + 8]]));
            return Some((w, h, data[i + 9]));
        }
        i += 2 + len;
    }
    None
}

fn from_jpeg(data: Vec<u8>) -> Option<PdfImage> {
    let (width, height, components) = jpeg_info(&data)?;
    matches!(components, 1 | 3).then_some(PdfImage {
        data,
        width,
        height,
        components,
        samples: Samples::Jpeg,
        alpha: None,
    })
}

fn from_png(data: &[u8]) -> Option<PdfImage> {
    let mut decoder = Decoder::new_with_limits(
        Cursor::new(data),
        Limits {
            bytes: MAX_DECODED_BYTES,
        },
    );
    decoder.set_transformations(Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.bit_depth != BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    let (components, alpha_at) = match info.color_type {
        ColorType::Grayscale => (1, None),
        ColorType::GrayscaleAlpha => (1, Some(1)),
        ColorType::Rgb => (3, None),
        ColorType::Rgba => (3, Some(3)),
        ColorType::Indexed => return None,
    };
    let stride = usize::from(components) + usize::from(alpha_at.is_some());
    let (data, alpha) = if alpha_at.is_some() {
        let mut color = Vec::with_capacity(buf.len() / stride * usize::from(components));
        let mut alpha = Vec::with_capacity(buf.len() / stride);
        for px in buf.chunks_exact(stride) {
            color.extend_from_slice(&px[..usize::from(components)]);
            alpha.push(px[usize::from(components)]);
        }
        // Fully opaque alpha planes carry no information.
        let alpha = alpha.iter().any(|a| *a != 255).then_some(alpha);
        (color, alpha)
    } else {
        (buf, None)
    };
    Some(PdfImage {
        data,
        width: info.width,
        height: info.height,
        components,
        samples: Samples::Raw,
        alpha,
    })
}

/// Decode WebP / GIF (first frame) with the `image` crate into raw RGB plus an optional alpha plane.
fn from_decoded(data: &[u8], format: image::ImageFormat) -> Option<PdfImage> {
    let mut reader = image::ImageReader::with_format(Cursor::new(data), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODED_BYTES as u64);
    reader.limits(limits);
    let rgba = reader.decode().ok()?.into_rgba8();
    let (width, height) = rgba.dimensions();
    let mut color = Vec::with_capacity(rgba.len() / 4 * 3);
    let mut alpha = Vec::with_capacity(rgba.len() / 4);
    for px in rgba.pixels() {
        color.extend_from_slice(&px.0[..3]);
        alpha.push(px.0[3]);
    }
    let alpha = alpha.iter().any(|a| *a != 255).then_some(alpha);
    Some(PdfImage {
        data: color,
        width,
        height,
        components: 3,
        samples: Samples::Raw,
        alpha,
    })
}

/// Decode a portrait for embedding. `None` when the format or color model is unsupported.
pub(crate) fn decode_portrait(mime: &str, bytes: Vec<u8>) -> Option<PdfImage> {
    match mime {
        "image/jpeg" => from_jpeg(bytes),
        "image/png" => from_png(&bytes),
        "image/webp" => from_decoded(&bytes, image::ImageFormat::WebP),
        "image/gif" => from_decoded(&bytes, image::ImageFormat::Gif),
        _ => None,
    }
}
