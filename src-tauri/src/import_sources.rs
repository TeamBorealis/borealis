//! Turning the UI's import sources (pasted text, backend-picked files, uploaded bytes) into the
//! pipeline's [`ImportSource`], with limits that match `borealis-ai`'s attachment guards.

use crate::error::{ApiError, ApiResult};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_ai::attachment::{prepare, MAX_IMAGE_BYTES, MAX_PDF_BYTES};
use borealis_ai::{Attachment, AttachmentKind, Backend};
use borealis_assist::import::anything::ImportSource;
use borealis_assist::import::{office_text, OfficeFormat};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[path = "import_rules_content.rs"]
pub mod rules_content;

/// Most sources in one import.
pub const MAX_SOURCES: usize = 12;
/// A raw image may exceed the 5 MiB the model gets because it is downscaled first; this caps what
/// is accepted before that (a phone photo is typically 3 to 12 MiB).
pub const MAX_RAW_IMAGE_BYTES: usize = 4 * MAX_IMAGE_BYTES;
/// Largest PDF accepted: the importer reads its text layer and splits scanned pages into parts
/// under the model's 20 MB limit, so a book far larger than one model call is fine.
pub const MAX_IMPORT_PDF_BYTES: usize = 256 * 1024 * 1024;
/// Largest Word / OpenDocument file accepted (a zip of XML and pictures).
pub const MAX_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;
/// Longest text (pasted, text files and documents together): detection reads long text window by
/// window, later stages read each element's own section.
pub const MAX_TEXT_CHARS: usize = 2_000_000;
/// Total bytes across all sources.
pub const MAX_TOTAL_BYTES: usize = 320 * 1024 * 1024;
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
    Document(OfficeFormat),
}

const UNSUPPORTED: &str =
    "Use an image (PNG, JPEG, WebP), a PDF, a Word (.docx) or OpenDocument (.odt) file, or a text file.";

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
    } else if let Some(f) = OfficeFormat::of_name(&name).or(match mime.as_str() {
        DOCX_MIME => Some(OfficeFormat::Docx),
        ODT_MIME => Some(OfficeFormat::Odt),
        _ => None,
    }) {
        Ok(Kind::Document(f))
    } else if mime.starts_with("text/")
        || mime == "application/json"
        || [".txt", ".md", ".markdown", ".json"]
            .iter()
            .any(|e| name.ends_with(e))
    {
        Ok(Kind::Text)
    } else {
        Err(ApiError::invalid(format!(
            "\"{}\" is not a supported file. {UNSUPPORTED}",
            base_name(&name)
        )))
    }
}

fn size_limit(kind: &Kind) -> usize {
    match kind {
        Kind::Image => MAX_RAW_IMAGE_BYTES,
        Kind::Pdf => MAX_IMPORT_PDF_BYTES,
        Kind::Text => MAX_TEXT_CHARS * 4,
        Kind::Document(_) => MAX_DOCUMENT_BYTES,
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
            Kind::Text => self.add_text(&String::from_utf8_lossy(&bytes), &quoted(name)),
            Kind::Document(format) => {
                let text = office_text(format, &bytes).map_err(|e| {
                    ApiError::invalid(format!("\"{}\" could not be read: {e}", base_name(name)))
                })?;
                self.add_text(&text, &quoted(name))
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

    /// Add one text source. Aurora, 5etools and Borealis content is refused here, before any
    /// model sees it: those imports are deterministic (Additional Content), never AI.
    fn add_text(&mut self, text: &str, subject: &str) -> ApiResult<()> {
        if let Some(kind) = rules_content::detect(text) {
            return Err(ApiError::invalid(rules_content::refusal(kind, subject)));
        }
        if !self.text.is_empty() {
            self.text.push_str("\n\n");
        }
        self.text.push_str(text.trim());
        Ok(())
    }
}

/// A file name as the refusals quote it.
fn quoted(name: &str) -> String {
    format!("\"{}\"", base_name(name))
}

/// How refusals name pasted or typed text.
const PASTED: &str = "The pasted text";

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
        "docx" => DOCX_MIME,
        "odt" => ODT_MIME,
        // Unknown extensions get no type, so `classify` refuses them by name (a key file stays out).
        _ => "",
    }
}

const DOCX_MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
const ODT_MIME: &str = "application/vnd.oasis.opendocument.text";

/// Validate attachments as the model will get them. A PDF over the model's limit is split into
/// parts during the import, so here it only has to be a PDF.
fn check_attachments(attachments: &[Attachment]) -> ApiResult<()> {
    let (large, fits): (Vec<&Attachment>, Vec<&Attachment>) = attachments
        .iter()
        .partition(|a| a.kind == AttachmentKind::Pdf && a.bytes.len() > MAX_PDF_BYTES);
    if let Some(a) = large.iter().find(|a| !a.bytes.starts_with(b"%PDF-")) {
        return Err(ApiError::invalid(format!("\"{}\" is not a PDF.", a.name)));
    }
    let fits: Vec<Attachment> = fits.into_iter().cloned().collect();
    prepare(Backend::Claude, &fits).map_err(|e| ApiError::invalid(e.to_string()))?;
    Ok(())
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
                acc.add_text(text, PASTED)?;
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
    if acc.text.chars().count() > MAX_TEXT_CHARS {
        return Err(ApiError::invalid(format!(
            "That text is too long to import (the limit is {MAX_TEXT_CHARS} characters). Split it up."
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
            check_attachments(&acc.attachments)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    const AURORA: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Test Spark" type="Spell" source="Synthetic" id="ID_SYN_SPELL_TEST_SPARK" />
</elements>"#;
    const FIVETOOLS: &str =
        r#"{"spell":[{"name":"Test Spark","source":"SYN","page":1,"entries":["A spark."]}]}"#;
    const BOREALIS: &str = r#"{"format":"borealis-content/1","elements":[]}"#;

    fn bytes(name: &str, mime: &str, text: &str) -> ImportSourceDto {
        ImportSourceDto::Bytes {
            name: name.into(),
            mime: mime.into(),
            base64: STANDARD.encode(text),
        }
    }

    fn refused(sources: &[ImportSourceDto]) -> String {
        match prepare_sources(sources, &PickedFiles::new()) {
            Ok(_) => panic!("rules content reached the AI importer"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn pasted_aurora_xml_is_refused_with_a_pointer_to_custom_content() {
        let msg = refused(&[ImportSourceDto::Text {
            text: AURORA.into(),
        }]);
        assert!(msg.contains("The pasted text is Aurora content."), "{msg}");
        assert!(msg.contains("Additional Content > Custom content"), "{msg}");
    }

    #[test]
    fn aurora_xml_in_a_text_file_is_refused() {
        let msg = refused(&[bytes("my class.txt", "text/plain", AURORA)]);
        assert!(msg.contains("\"my class.txt\" is Aurora content."), "{msg}");
    }

    #[test]
    fn fivetools_json_is_refused_with_a_pointer_to_5etools_import() {
        let msg = refused(&[bytes("spells-syn.json", "application/json", FIVETOOLS)]);
        assert!(
            msg.contains("\"spells-syn.json\" is 5etools data."),
            "{msg}"
        );
        assert!(msg.contains("Additional Content > 5etools import"), "{msg}");
        let pasted = refused(&[ImportSourceDto::Text {
            text: FIVETOOLS.into(),
        }]);
        assert!(pasted.contains("5etools import"), "{pasted}");
    }

    #[test]
    fn a_borealis_pack_is_refused() {
        let msg = refused(&[bytes("pack.json", "", BOREALIS)]);
        assert!(msg.contains("Borealis content pack"), "{msg}");
    }

    #[test]
    fn notes_beside_an_image_are_checked_too() {
        let png = ImportSourceDto::Bytes {
            name: "page.png".into(),
            mime: "image/png".into(),
            base64: STANDARD.encode([137u8, 80, 78, 71]),
        };
        let msg = refused(&[
            png,
            ImportSourceDto::Text {
                text: AURORA.into(),
            },
        ]);
        assert!(msg.contains("Aurora content"), "{msg}");
    }

    #[test]
    fn ordinary_text_and_json_still_import() {
        let ok = prepare_sources(
            &[
                ImportSourceDto::Text {
                    text: "Test Spark. Cantrip, evocation. A tiny spark.".into(),
                },
                bytes(
                    "notes.json",
                    "application/json",
                    r#"{"name":"Test Spark","level":0}"#,
                ),
            ],
            &PickedFiles::new(),
        )
        .expect("ordinary sources");
        assert!(matches!(ok.source, ImportSource::Text(t) if t.contains("Test Spark")));
    }
}
