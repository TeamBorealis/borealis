//! Image and PDF attachments for vision-capable sessions.
//!
//! Everything here is pure (no process spawning) so the exact wire shapes are golden-testable.
//!
//! * **Claude** takes `--input-format stream-json`: one JSON user message on stdin whose
//!   `content` holds `image` and `document` (base64 PDF) blocks plus the prompt as a `text` block.
//! * **Codex** takes images as `-i <file>` and has no PDF input, so a PDF contributes its text
//!   layer to the prompt; a PDF without a text layer yields [`AiError::NeedsImages`].
//!
//! Images are downscaled to at most [`MAX_LONG_EDGE`] px and must end up at most
//! [`MAX_IMAGE_BYTES`] bytes. File names supplied by callers are only ever shown to the model as
//! labels; they never become paths or argv.

use std::ffi::OsString;
use std::io::Cursor;
use std::path::PathBuf;

use base64::Engine as _;
use image::{imageops::FilterType, DynamicImage, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::command::{self, CommandInputs, CommandPlan};
use crate::{AiError, Backend};

/// Longest allowed image edge after downscaling (the vision models' sweet spot).
pub const MAX_LONG_EDGE: u32 = 1568;
/// Largest encoded image sent to a model.
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
/// Largest PDF accepted.
pub const MAX_PDF_BYTES: usize = 20 * 1024 * 1024;
/// Refuse to decode images with more pixels than this (decompression-bomb guard).
pub const MAX_PIXELS: u64 = 40_000_000;
/// Decoder allocation ceiling (`image::Limits::max_alloc`).
pub const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;
/// Largest accepted image width or height.
pub const MAX_IMAGE_DIM: u32 = 16_384;
/// Stack for the PDF parsing thread (lopdf recurses on nested objects).
const PDF_THREAD_STACK: usize = 64 * 1024 * 1024;
/// Cap on text extracted from a PDF before decompression output stops being collected.
pub const MAX_PDF_EXTRACT_BYTES: usize = 8 * 1024 * 1024;

fn image_limits() -> image::Limits {
    let mut l = image::Limits::default();
    l.max_alloc = Some(MAX_DECODE_ALLOC);
    l.max_image_width = Some(MAX_IMAGE_DIM);
    l.max_image_height = Some(MAX_IMAGE_DIM);
    l
}
/// Cap on extracted PDF text handed to Codex.
pub const MAX_PDF_TEXT_CHARS: usize = 200_000;
/// A PDF with fewer visible characters per page than this is treated as scanned.
const MIN_TEXT_CHARS_PER_PAGE: usize = 25;
const JPEG_QUALITY: u8 = 85;

/// What an attachment is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum AttachmentKind {
    /// A raster image; `mime` is `image/png`, `image/jpeg` or `image/webp`.
    Image {
        /// Declared MIME type.
        mime: String,
    },
    /// A PDF document.
    Pdf,
}

/// One uploaded file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Image or PDF.
    #[serde(flatten)]
    pub kind: AttachmentKind,
    /// Raw file bytes.
    pub bytes: Vec<u8>,
    /// Display name (label only).
    pub name: String,
}

/// An attachment after validation, downscaling and backend routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparedItem {
    /// Encoded image ready to send.
    Image {
        /// Label.
        name: String,
        /// `image/png` or `image/jpeg` or `image/webp`.
        mime: &'static str,
        /// Encoded bytes (at most [`MAX_IMAGE_BYTES`]).
        bytes: Vec<u8>,
    },
    /// Native PDF (Claude `document` block).
    Pdf {
        /// Label.
        name: String,
        /// PDF bytes.
        bytes: Vec<u8>,
    },
    /// Text layer of a PDF (Codex fallback).
    PdfText {
        /// Label.
        name: String,
        /// Extracted text.
        text: String,
    },
}

fn invalid(msg: impl Into<String>) -> AiError {
    AiError::Invalid(msg.into())
}

fn mime_of(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png => Some("image/png"),
        ImageFormat::Jpeg => Some("image/jpeg"),
        ImageFormat::WebP => Some("image/webp"),
        _ => None,
    }
}

fn extension_of(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => "png",
    }
}

/// Validate, downscale and (if needed) re-encode an image.
pub fn prepare_image(
    name: &str,
    declared_mime: &str,
    bytes: &[u8],
) -> Result<PreparedItem, AiError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(AiError::io)?;
    reader.limits(image_limits());
    let format = reader
        .format()
        .ok_or_else(|| invalid(format!("`{name}` is not a recognised image")))?;
    let mime = mime_of(format).ok_or_else(|| {
        invalid(format!(
            "`{name}` ({declared_mime}) is not PNG, JPEG or WebP; convert it first"
        ))
    })?;
    let (w, h) = reader
        .into_dimensions()
        .map_err(|e| invalid(format!("`{name}` is not a readable image: {e}")))?;
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(invalid(format!("`{name}` has too many pixels ({w}x{h})")));
    }
    if w.max(h) <= MAX_LONG_EDGE && bytes.len() <= MAX_IMAGE_BYTES {
        return Ok(PreparedItem::Image {
            name: name.to_owned(),
            mime,
            bytes: bytes.to_vec(),
        });
    }
    let mut full = ImageReader::with_format(Cursor::new(bytes), format);
    full.limits(image_limits());
    let mut img = full
        .decode()
        .map_err(|e| invalid(format!("`{name}` could not be decoded: {e}")))?;
    if w.max(h) > MAX_LONG_EDGE {
        img = img.resize(MAX_LONG_EDGE, MAX_LONG_EDGE, FilterType::Lanczos3);
    }
    let (mime, out) = encode_within_limit(&img, format == ImageFormat::Jpeg)?;
    if out.len() > MAX_IMAGE_BYTES {
        return Err(invalid(format!(
            "`{name}` is still {} MB after downscaling; the limit is {} MB",
            out.len() / (1024 * 1024),
            MAX_IMAGE_BYTES / (1024 * 1024)
        )));
    }
    Ok(PreparedItem::Image {
        name: name.to_owned(),
        mime,
        bytes: out,
    })
}

/// Lossless PNG unless the source was JPEG or the PNG is too big, then JPEG.
fn encode_within_limit(
    img: &DynamicImage,
    prefer_jpeg: bool,
) -> Result<(&'static str, Vec<u8>), AiError> {
    if !prefer_jpeg {
        let png = encode(img, ImageFormat::Png)?;
        if png.len() <= MAX_IMAGE_BYTES {
            return Ok(("image/png", png));
        }
    }
    let flat = flatten_on_white(img);
    Ok(("image/jpeg", encode(&flat, ImageFormat::Jpeg)?))
}

fn flatten_on_white(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = u32::from(p[3]);
        let mix = |c: u8| ((u32::from(c) * a + 255 * (255 - a)) / 255) as u8;
        rgb.put_pixel(x, y, image::Rgb([mix(p[0]), mix(p[1]), mix(p[2])]));
    }
    DynamicImage::ImageRgb8(rgb)
}

fn encode(img: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>, AiError> {
    let mut out = Vec::new();
    let cursor = &mut Cursor::new(&mut out);
    match format {
        ImageFormat::Jpeg => img
            .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
                cursor,
                JPEG_QUALITY,
            ))
            .map_err(AiError::io)?,
        _ => img.write_to(cursor, format).map_err(AiError::io)?,
    }
    Ok(out)
}

/// Text layer of a PDF. `Ok(None)` means it has (almost) no text, i.e. it is scanned.
pub fn pdf_text_layer(bytes: &[u8]) -> Result<Option<String>, AiError> {
    // lopdf recurses on nested objects; parse on a big-stack thread so a hostile file yields an
    // error instead of aborting the process. The bytes are copied because the thread is 'static.
    let owned = bytes.to_vec();
    std::thread::Builder::new()
        .name("pdf-extract".into())
        .stack_size(PDF_THREAD_STACK)
        .spawn(move || pdf_text_layer_inner(&owned))
        .map_err(AiError::io)?
        .join()
        .map_err(|_| invalid("the PDF could not be processed (parser panicked)"))?
}

fn pdf_text_layer_inner(bytes: &[u8]) -> Result<Option<String>, AiError> {
    let doc =
        lopdf::Document::load_mem(bytes).map_err(|e| invalid(format!("unreadable PDF: {e}")))?;
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    if pages.is_empty() {
        return Ok(None);
    }
    // Page by page, stopping once the cap is hit, so a flate bomb cannot grow the text unbounded.
    let mut text = String::new();
    for n in &pages {
        if text.len() >= MAX_PDF_EXTRACT_BYTES {
            break;
        }
        if let Ok(t) = doc.extract_text(&[*n]) {
            text.push_str(&t);
            text.push('\n');
        }
    }
    let visible = text.chars().filter(|c| !c.is_whitespace()).count();
    if visible < MIN_TEXT_CHARS_PER_PAGE * pages.len() {
        return Ok(None);
    }
    Ok(Some(text.chars().take(MAX_PDF_TEXT_CHARS).collect()))
}

/// Validate and route every attachment for `backend`.
pub fn prepare(backend: Backend, attachments: &[Attachment]) -> Result<Vec<PreparedItem>, AiError> {
    attachments
        .iter()
        .map(|a| match &a.kind {
            AttachmentKind::Image { mime } => prepare_image(&a.name, mime, &a.bytes),
            AttachmentKind::Pdf => prepare_pdf(backend, a),
        })
        .collect()
}

fn prepare_pdf(backend: Backend, a: &Attachment) -> Result<PreparedItem, AiError> {
    if !a.bytes.starts_with(b"%PDF-") {
        return Err(invalid(format!("`{}` is not a PDF", a.name)));
    }
    if a.bytes.len() > MAX_PDF_BYTES {
        return Err(invalid(format!(
            "`{}` exceeds the {} MB PDF limit",
            a.name,
            MAX_PDF_BYTES / (1024 * 1024)
        )));
    }
    match backend {
        Backend::Claude => Ok(PreparedItem::Pdf {
            name: a.name.clone(),
            bytes: a.bytes.clone(),
        }),
        Backend::Codex => match pdf_text_layer(&a.bytes)? {
            Some(text) => Ok(PreparedItem::PdfText {
                name: a.name.clone(),
                text,
            }),
            None => Err(AiError::NeedsImages {
                name: a.name.clone(),
            }),
        },
    }
}

/// Number of prepared items that Codex will receive as `-i` files.
pub fn image_count(items: &[PreparedItem]) -> usize {
    items
        .iter()
        .filter(|i| matches!(i, PreparedItem::Image { .. }))
        .count()
}

/// File name (inside the support dir) for the `n`th Codex image. Never derived from user input.
pub fn codex_file_name(n: usize, mime: &str) -> String {
    format!("attachment-{n}.{}", extension_of(mime))
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The single stream-json user message Claude reads on stdin.
pub fn claude_stream_input(prompt: &str, items: &[PreparedItem]) -> Vec<u8> {
    let mut content: Vec<Value> = Vec::new();
    for (n, item) in items.iter().enumerate() {
        match item {
            PreparedItem::Image { name, mime, bytes } => {
                content.push(
                    json!({"type": "text", "text": format!("[Attachment {}: {name}]", n + 1)}),
                );
                content.push(json!({"type": "image", "source": {
                    "type": "base64", "media_type": mime, "data": b64(bytes)}}));
            }
            PreparedItem::Pdf { name, bytes } => {
                content.push(
                    json!({"type": "text", "text": format!("[Attachment {}: {name}]", n + 1)}),
                );
                content.push(json!({"type": "document", "source": {
                    "type": "base64", "media_type": "application/pdf", "data": b64(bytes)}}));
            }
            PreparedItem::PdfText { .. } => {}
        }
    }
    content.push(json!({"type": "text", "text": prompt}));
    let msg = json!({"type": "user", "message": {"role": "user", "content": content}});
    let mut line = msg.to_string().into_bytes();
    line.push(b'\n');
    line
}

fn codex_prompt(prompt: &str, items: &[PreparedItem]) -> String {
    let mut out = String::new();
    for item in items {
        if let PreparedItem::PdfText { name, text } = item {
            out.push_str(&format!("[Attached PDF text layer: {name}]\n{text}\n\n"));
        }
    }
    out.push_str(prompt);
    out
}

/// Like [`command::build`], with `items` attached. `codex_images` are the on-disk paths of the
/// image items, in order (the caller writes them with [`codex_file_name`] names).
pub fn build_with_attachments(
    inputs: &CommandInputs<'_>,
    items: &[PreparedItem],
    codex_images: &[PathBuf],
) -> CommandPlan {
    let mut plan = command::build(inputs);
    if items.is_empty() {
        return plan;
    }
    match inputs.backend {
        Backend::Claude => {
            let prompt = String::from_utf8_lossy(&plan.stdin).into_owned();
            plan.stdin = claude_stream_input(&prompt, items);
            plan.args
                .splice(1..1, ["--input-format", "stream-json"].map(OsString::from));
        }
        Backend::Codex => {
            let prompt = String::from_utf8_lossy(&plan.stdin).into_owned();
            plan.stdin = codex_prompt(&prompt, items).into_bytes();
            // `-i` is variadic, so every `-i` group must be followed by another flag. Placing
            // them right after `exec` (before `--json`) keeps the trailing `-` out of the list.
            let flags = codex_images
                .iter()
                .flat_map(|p| [OsString::from("-i"), p.as_os_str().to_os_string()]);
            plan.args.splice(1..1, flags.collect::<Vec<_>>());
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn png(w: u32, h: u32, noisy: bool) -> Vec<u8> {
        let mut img = image::RgbImage::new(w, h);
        let mut seed = 0x9E37_79B9_u32;
        for p in img.pixels_mut() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *p = if noisy {
                image::Rgb([(seed >> 8) as u8, (seed >> 16) as u8, (seed >> 24) as u8])
            } else {
                image::Rgb([255, 255, 255])
            };
        }
        encode(&DynamicImage::ImageRgb8(img), ImageFormat::Png).unwrap()
    }

    fn dims(bytes: &[u8]) -> (u32, u32) {
        ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .unwrap()
            .into_dimensions()
            .unwrap()
    }

    fn pdf(text: &str) -> Vec<u8> {
        let stream = format!("BT /F1 12 Tf 10 40 Td ({text}) Tj ET");
        let objs = [
            "<</Type/Catalog/Pages 2 0 R>>".to_owned(),
            "<</Type/Pages/Kids[3 0 R]/Count 1>>".to_owned(),
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 400 100]/Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>".to_owned(),
            format!("<</Length {}>>\nstream\n{stream}\nendstream", stream.len()),
            "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_owned(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offs = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offs.push(out.len());
            out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
        for o in offs {
            out.extend(format!("{o:010} 00000 n \n").bytes());
        }
        out.extend(
            format!(
                "trailer<</Size {}/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n",
                objs.len() + 1
            )
            .bytes(),
        );
        out
    }

    fn att_image(bytes: Vec<u8>) -> Attachment {
        Attachment {
            kind: AttachmentKind::Image {
                mime: "image/png".into(),
            },
            bytes,
            name: "hi.png".into(),
        }
    }

    fn inputs(backend: Backend) -> CommandInputs<'static> {
        CommandInputs {
            backend,
            program: Path::new("/bin/cli"),
            workdir: Path::new("/tmp/w"),
            prompt: "read it",
            system: None,
            system_file: None,
            model: None,
            effort: None,
            tools: command::ToolPolicy::None,
            schema: None,
            native_schema: false,
            schema_file: None,
            use_user_config: false,
            profile_args: &[],
        }
    }

    #[test]
    fn small_image_passes_through_unchanged() {
        let bytes = png(64, 32, false);
        let PreparedItem::Image {
            bytes: out, mime, ..
        } = prepare_image("a", "image/png", &bytes).unwrap()
        else {
            panic!("expected image")
        };
        assert_eq!(out, bytes);
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn large_image_is_downscaled_to_long_edge() {
        let bytes = png(3136, 1000, false);
        let PreparedItem::Image { bytes: out, .. } =
            prepare_image("a", "image/png", &bytes).unwrap()
        else {
            panic!("expected image")
        };
        assert_eq!(dims(&out), (MAX_LONG_EDGE, 500));
        assert!(out.len() <= MAX_IMAGE_BYTES);
    }

    #[test]
    fn oversized_noisy_png_falls_back_to_jpeg_within_limit() {
        let bytes = png(1500, 1200, true); // ~5.4 MB of incompressible PNG, within the edge limit
        assert!(bytes.len() > MAX_IMAGE_BYTES);
        let PreparedItem::Image {
            bytes: out, mime, ..
        } = prepare_image("n", "image/png", &bytes).unwrap()
        else {
            panic!("expected image")
        };
        assert_eq!(mime, "image/jpeg");
        assert!(out.len() <= MAX_IMAGE_BYTES);
    }

    #[test]
    fn image_over_dimension_limit_is_rejected() {
        let mut hdr = Vec::new();
        hdr.extend(70_000u32.to_be_bytes());
        hdr.extend(10u32.to_be_bytes());
        hdr.extend([8, 2, 0, 0, 0]);
        let mut crc_input = b"IHDR".to_vec();
        crc_input.extend(&hdr);
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(13u32.to_be_bytes());
        png.extend(&crc_input);
        png.extend(crc32(&crc_input).to_be_bytes());
        assert!(prepare_image("wide", "image/png", &png).is_err());
    }

    #[test]
    fn pixel_bomb_and_garbage_are_rejected() {
        // IHDR-only PNG claiming 8000x8000 (64M pixels).
        let mut hdr = Vec::new();
        hdr.extend(8_000u32.to_be_bytes());
        hdr.extend(8_000u32.to_be_bytes());
        hdr.extend([8, 2, 0, 0, 0]);
        let mut crc_input = b"IHDR".to_vec();
        crc_input.extend(&hdr);
        let mut bomb = b"\x89PNG\r\n\x1a\n".to_vec();
        bomb.extend(13u32.to_be_bytes());
        bomb.extend(&crc_input);
        bomb.extend(crc32(&crc_input).to_be_bytes());
        let idat = b"IDAT\x00".to_vec();
        bomb.extend(1u32.to_be_bytes());
        bomb.extend(&idat);
        bomb.extend(crc32(&idat).to_be_bytes());
        assert!(matches!(
            prepare_image("b", "image/png", &bomb),
            Err(AiError::Invalid(m)) if m.contains("too many pixels")
        ));
        assert!(prepare_image("g", "image/png", b"not an image").is_err());
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for b in data {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    #[test]
    fn pdf_size_and_magic_guards() {
        let mut big = b"%PDF-1.4".to_vec();
        big.resize(MAX_PDF_BYTES + 1, 0);
        let a = |bytes: Vec<u8>| Attachment {
            kind: AttachmentKind::Pdf,
            bytes,
            name: "x.pdf".into(),
        };
        assert!(prepare(Backend::Claude, &[a(big)]).is_err());
        assert!(prepare(Backend::Claude, &[a(b"hello".to_vec())]).is_err());
    }

    #[test]
    fn pdf_routing_native_for_claude_text_for_codex_scanned_needs_images() {
        let text_pdf = pdf("Fireball deals 8d6 fire damage in a twenty foot radius sphere");
        let att = Attachment {
            kind: AttachmentKind::Pdf,
            bytes: text_pdf,
            name: "spell.pdf".into(),
        };
        let claude = prepare(Backend::Claude, std::slice::from_ref(&att)).unwrap();
        assert!(matches!(&claude[0], PreparedItem::Pdf { .. }));
        let codex = prepare(Backend::Codex, std::slice::from_ref(&att)).unwrap();
        let PreparedItem::PdfText { text, .. } = &codex[0] else {
            panic!("expected text")
        };
        assert!(text.contains("Fireball"), "{text}");

        let scanned = Attachment {
            bytes: pdf(""),
            name: "scan.pdf".into(),
            ..att
        };
        assert!(matches!(
            prepare(Backend::Codex, std::slice::from_ref(&scanned)),
            Err(AiError::NeedsImages { name }) if name == "scan.pdf"
        ));
        // Claude reads scanned PDFs natively.
        assert!(prepare(Backend::Claude, &[scanned]).is_ok());
    }

    #[test]
    fn claude_stream_json_golden() {
        let items = vec![
            PreparedItem::Image {
                name: "hi.png".into(),
                mime: "image/png",
                bytes: vec![1, 2, 3],
            },
            PreparedItem::Pdf {
                name: "s.pdf".into(),
                bytes: b"%PDF".to_vec(),
            },
        ];
        let got = String::from_utf8(claude_stream_input("read it", &items)).unwrap();
        let expected = concat!(
            r#"{"message":{"content":["#,
            r#"{"text":"[Attachment 1: hi.png]","type":"text"},"#,
            r#"{"source":{"data":"AQID","media_type":"image/png","type":"base64"},"type":"image"},"#,
            r#"{"text":"[Attachment 2: s.pdf]","type":"text"},"#,
            r#"{"source":{"data":"JVBERg==","media_type":"application/pdf","type":"base64"},"type":"document"},"#,
            r#"{"text":"read it","type":"text"}],"role":"user"},"type":"user"}"#,
            "\n"
        );
        assert_eq!(got, expected);
    }

    #[test]
    fn claude_args_gain_stream_json_input_and_keep_isolation() {
        let items = prepare(Backend::Claude, &[att_image(png(8, 8, false))]).unwrap();
        let plan = build_with_attachments(&inputs(Backend::Claude), &items, &[]);
        let a: Vec<String> = plan
            .args
            .iter()
            .map(|x| x.to_string_lossy().into())
            .collect();
        assert_eq!(a[0], "-p");
        assert_eq!(&a[1..3], ["--input-format", "stream-json"]);
        assert!(a.windows(2).any(|w| w[0] == "--tools" && w[1].is_empty()));
        assert!(a.contains(&"--strict-mcp-config".to_owned()));
        assert!(!a.iter().any(|x| x == "read it"));
        assert!(plan.stdin.ends_with(b"\n"));
    }

    #[test]
    fn codex_args_put_images_before_flags_and_prompt_dash_last() {
        let items = prepare(Backend::Codex, &[att_image(png(8, 8, false))]).unwrap();
        assert_eq!(image_count(&items), 1);
        let paths = [PathBuf::from("/s/attachment-0.png")];
        let plan = build_with_attachments(&inputs(Backend::Codex), &items, &paths);
        let a: Vec<String> = plan
            .args
            .iter()
            .map(|x| x.to_string_lossy().into())
            .collect();
        assert_eq!(&a[..4], ["exec", "-i", "/s/attachment-0.png", "--json"]);
        assert_eq!(a.last().unwrap(), "-");
        assert!(a.contains(&"read-only".to_owned()));
        assert_eq!(codex_file_name(2, "image/jpeg"), "attachment-2.jpg");
    }

    #[test]
    fn codex_pdf_text_is_prepended_to_prompt() {
        let items = vec![PreparedItem::PdfText {
            name: "s.pdf".into(),
            text: "FIREBALL".into(),
        }];
        let plan = build_with_attachments(&inputs(Backend::Codex), &items, &[]);
        let stdin = String::from_utf8(plan.stdin).unwrap();
        assert!(stdin.starts_with("[Attached PDF text layer: s.pdf]\nFIREBALL"));
        assert!(stdin.ends_with("read it"));
    }

    #[test]
    fn no_attachments_leaves_plan_untouched() {
        let i = inputs(Backend::Claude);
        assert_eq!(build_with_attachments(&i, &[], &[]), command::build(&i));
    }
}
