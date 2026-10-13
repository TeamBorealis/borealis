//! Turning the UI's import sources (pasted text, backend-picked files, uploaded bytes) into the
//! pipeline's [`ImportSource`], with limits that match `borealis-ai`'s attachment guards.

use crate::error::{ApiError, ApiResult};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_ai::attachment::{prepare, MAX_IMAGE_BYTES, MAX_PDF_BYTES, MAX_PDF_TEXT_CHARS};
use borealis_ai::{Attachment, AttachmentKind, Backend};
use borealis_assist::import::anything::ImportSource;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Most sources in one import.
pub const MAX_SOURCES: usize = 12;
/// A raw image may exceed the 5 MiB the model gets because it is downscaled first; this caps what
/// is accepted before that (a phone photo is typically 3 to 12 MiB).
pub const MAX_RAW_IMAGE_BYTES: usize = 4 * MAX_IMAGE_BYTES;
/// Total bytes across all sources.
pub const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
/// Longest note that accompanies attachments.
pub const MAX_NOTES_CHARS: usize = 4000;

/// One thing to import, as the UI sends it.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ImportSourceDto {
    /// Pasted or typed text.
    Text { text: String },
    /// A file the user chose in the backend's native dialog (`pick_path` with purpose
    /// `import-file`). Only the opaque token travels; name and type come from the picked path.
    File { token: String },
    /// File contents read by the webview (drop, picker, camera).
    Bytes {
        name: String,
        #[serde(default)]
        mime: String,
        base64: String,
    },
}

/// What the pipeline gets.
#[derive(Debug, Clone)]
pub struct PreparedSource {
    /// Text or attachments.
    pub source: ImportSource,
    /// Name shown in the pack's provenance.
    pub source_name: String,
    /// Typed notes that came with images or PDFs ("this is a 2024 Fighter subclass"). They ride
    /// along in every model step; text-only imports have none (the text is the source).
    pub notes: Option<String>,
}

enum Kind {
    Image,
    Pdf,
    Text,
}

fn base_name(name: &str) -> String {
    let n = name.rsplit(['/', '\\']).next().unwrap_or(name).trim();
    let n: String = n.chars().filter(|c| !c.is_control()).take(80).collect();
    if n.is_empty() {
        "upload".into()
    } else {
        n
    }
}

fn classify(name: &str, mime: &str) -> ApiResult<Kind> {
    let (name, mime) = (name.to_lowercase(), mime.to_lowercase());
    if mime.starts_with("image/")
        || [".png", ".jpg", ".jpeg", ".webp"]
            .iter()
            .any(|e| name.ends_with(e))
    {
        Ok(Kind::Image)
    } else if mime == "application/pdf" || name.ends_with(".pdf") {
        Ok(Kind::Pdf)
    } else if mime.starts_with("text/")
        || mime == "application/json"
        || [".txt", ".md", ".markdown", ".json"]
            .iter()
            .any(|e| name.ends_with(e))
    {
        Ok(Kind::Text)
    } else {
        Err(ApiError::invalid(format!(
            "\"{}\" is not a supported file. Use an image (PNG, JPEG, WebP), a PDF, or a text file.",
            base_name(&name)
        )))
    }
}

fn size_limit(kind: &Kind) -> usize {
    match kind {
        Kind::Image => MAX_RAW_IMAGE_BYTES,
        Kind::Pdf => MAX_PDF_BYTES,
        Kind::Text => MAX_PDF_TEXT_CHARS * 4,
    }
}

fn too_big(name: &str, limit: usize) -> ApiError {
    ApiError::invalid(format!(
        "\"{}\" is too large to import (the limit is {} MB).",
        base_name(name),
        limit / (1024 * 1024)
    ))
}

struct Collected {
    text: String,
    attachments: Vec<Attachment>,
    total: usize,
}

impl Collected {
    fn add_file(&mut self, name: &str, mime: &str, bytes: Vec<u8>) -> ApiResult<()> {
        let kind = classify(name, mime)?;
        let limit = size_limit(&kind);
        if bytes.is_empty() {
            return Err(ApiError::invalid(format!(
                "\"{}\" is empty.",
                base_name(name)
            )));
        }
        if bytes.len() > limit {
            return Err(too_big(name, limit.max(1024 * 1024)));
        }
        self.total += bytes.len();
        if self.total > MAX_TOTAL_BYTES {
            return Err(too_big("these files", MAX_TOTAL_BYTES));
        }
        match kind {
            Kind::Text => {
                self.add_text(&String::from_utf8_lossy(&bytes));
                Ok(())
            }
            Kind::Image => {
                self.attachments.push(Attachment {
                    kind: AttachmentKind::Image {
                        mime: mime.to_string(),
                    },
                    bytes,
                    name: base_name(name),
                });
                Ok(())
            }
            Kind::Pdf => {
                self.attachments.push(Attachment {
                    kind: AttachmentKind::Pdf,
                    bytes,
                    name: base_name(name),
                });
                Ok(())
            }
        }
    }

    fn add_text(&mut self, text: &str) {
        if !self.text.is_empty() {
            self.text.push_str("\n\n");
        }
        self.text.push_str(text.trim());
    }
}

/// MIME type for a picked file, from its extension (the webview does not get to say).
fn mime_for(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or_default();
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "txt" | "md" | "markdown" => "text/plain",
        // Unknown extensions get no type, so `classify` refuses them by name (a key file stays out).
        _ => "",
    }
}

/// Picked paths behind the `File` tokens of one import, resolved before the blocking work.
pub type PickedFiles = HashMap<String, PathBuf>;

fn read_native(p: &Path, acc: &mut Collected) -> ApiResult<String> {
    let name = crate::picked_paths::file_name(p);
    let mime = mime_for(&name);
    let meta = std::fs::metadata(p).map_err(|e| ApiError::io("read the file", e))?;
    if !meta.is_file() {
        return Err(ApiError::invalid("That is not a file."));
    }
    let kind = classify(&name, mime)?;
    let limit = size_limit(&kind);
    if usize::try_from(meta.len()).map_or(true, |n| n > limit) {
        return Err(too_big(&name, limit.max(1024 * 1024)));
    }
    let bytes = std::fs::read(p).map_err(|e| ApiError::io("read the file", e))?;
    acc.add_file(&name, mime, bytes)?;
    Ok(name)
}

fn decode_bytes(name: &str, mime: &str, b64: &str) -> ApiResult<Vec<u8>> {
    let limit = size_limit(&classify(name, mime)?);
    if b64.len() / 4 * 3 > limit + 3 {
        return Err(too_big(name, limit.max(1024 * 1024)));
    }
    STANDARD
        .decode(b64.trim())
        .map_err(|_| ApiError::invalid(format!("\"{}\" could not be read.", base_name(name))))
}

/// Collect, size-check and validate the sources.
///
/// # Errors
/// `invalid_argument` for no sources, too many, a bad or oversized file, an unreadable image or PDF,
/// or text mixed with images and PDFs, or a `File` token missing from `picked`; `io_error` when a
/// picked file cannot be read.
pub fn prepare_sources(
    sources: &[ImportSourceDto],
    picked: &PickedFiles,
) -> ApiResult<PreparedSource> {
    if sources.is_empty() {
        return Err(ApiError::invalid(
            "Add some text, an image or a PDF to import.",
        ));
    }
    if sources.len() > MAX_SOURCES {
        return Err(ApiError::invalid(format!(
            "Too many files at once (the limit is {MAX_SOURCES})."
        )));
    }
    let mut acc = Collected {
        text: String::new(),
        attachments: Vec::new(),
        total: 0,
    };
    let mut names = Vec::new();
    for s in sources {
        match s {
            ImportSourceDto::Text { text } => {
                acc.total += text.len();
                acc.add_text(text);
                names.push("Pasted text".to_string());
            }
            ImportSourceDto::File { token } => {
                let path = picked.get(token).ok_or_else(|| {
                    ApiError::invalid("That file choice has expired. Choose the file again.")
                })?;
                let name = read_native(path, &mut acc)?;
                names.push(base_name(&name));
            }
            ImportSourceDto::Bytes { name, mime, base64 } => {
                let bytes = decode_bytes(name, mime, base64)?;
                acc.add_file(name, mime, bytes)?;
                names.push(base_name(name));
            }
        }
    }
    if acc.text.chars().count() > MAX_PDF_TEXT_CHARS {
        return Err(ApiError::invalid(format!(
            "That text is too long to import (the limit is {MAX_PDF_TEXT_CHARS} characters). Split it up."
        )));
    }
    let source_name = names.join(", ").chars().take(120).collect();
    let (source, notes) = match (acc.text.is_empty(), acc.attachments.is_empty()) {
        (true, true) => return Err(ApiError::invalid("There is nothing to import.")),
        (false, true) => (ImportSource::Text(acc.text), None),
        (no_notes, false) => {
            if acc.text.chars().count() > MAX_NOTES_CHARS {
                return Err(ApiError::invalid(format!(
                    "The notes are too long (the limit is {MAX_NOTES_CHARS} characters when files are attached)."
                )));
            }
            prepare(Backend::Claude, &acc.attachments)
                .map_err(|e| ApiError::invalid(e.to_string()))?;
            (
                ImportSource::Attachments(acc.attachments),
                (!no_notes).then_some(acc.text),
            )
        }
    };
    Ok(PreparedSource {
        source,
        source_name,
        notes,
    })
}
