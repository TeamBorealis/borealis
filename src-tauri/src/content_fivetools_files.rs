//! 5etools-format homebrew files picked one by one (B-238): a JSON object with a `_meta` block and
//! entity arrays (`spell`, `item`, `monster`, `class`, `subclass`, ...). Every platform takes them
//! through the WebView file chooser, several at once, optionally beside one `.zip` of a `data/`
//! folder they build on.
//!
//! The files are written into the `homebrew/` sub-folder of a fresh scratch data folder (the zip,
//! when given, unpacked into the same folder first), which is granted as a `5etools-folder` pick:
//! scan and import then run exactly as for a chosen folder, which already reads `homebrew/*.json`.
//! Each file is checked before anything is written: it must parse as JSON and look like
//! 5etools-format data, and a Borealis content file is sent back to the custom content import.
//! Deterministic: no AI is involved.

use crate::content_fivetools_zip::{decode_zip, unpack_5etools_zip, MAX_FILE_BYTES, STAGE_DIR};
use crate::content_preview::ImportFileInput;
use crate::error::{ApiError, ApiResult};
use crate::picked_paths::{PickPurpose, PickedPathDto};
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use borealis_import_5etools::folder::HOMEBREW_DIR;
use borealis_import_5etools::homebrew;
use std::collections::BTreeSet;
use std::path::Path;

/// Most files staged at once.
pub const MAX_FILES: usize = 64;

/// The message the custom content import gives for a 5etools-format JSON file.
#[must_use]
pub fn route_to_5etools_message(file_name: &str) -> String {
    format!(
        "{file_name} is a 5etools-format homebrew file. Import it in Additional Content > \
         5etools-format import > \"{}\".",
        HOMEBREW_BUTTON
    )
}

/// Label of the 5etools tab's button for homebrew files (the UI shows the same text).
pub const HOMEBREW_BUTTON: &str = "Choose 5etools-format homebrew .json files";

/// `invalid_argument` pointing to the 5etools import when `bytes` hold a 5etools-format JSON file.
///
/// # Errors
/// [`route_to_5etools_message`] for a 5etools-format file.
pub fn refuse_5etools_json(file_name: &str, bytes: &[u8]) -> ApiResult<()> {
    if homebrew::is_5etools_json(bytes) {
        return Err(ApiError::invalid(route_to_5etools_message(file_name)));
    }
    Ok(())
}

fn base_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

fn is_ext(name: &str, ext: &str) -> bool {
    base_name(name)
        .rsplit_once('.')
        .is_some_and(|(_, e)| e.eq_ignore_ascii_case(ext))
}

/// A file name safe on every file system: ASCII letters, digits, ` `, `-`, `_` and `.` kept,
/// anything else `_`; never hidden; always ending in `.json`.
#[must_use]
pub fn safe_file_name(name: &str) -> String {
    let stem = crate::content_import::stem(name);
    let clean: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .take(100)
        .collect();
    let clean = clean.trim_matches(|c| c == '.' || c == ' ');
    let stem = if clean.is_empty() { "homebrew" } else { clean };
    format!("{stem}.json")
}

/// `name` made unique among `taken` (compared without case) by a `-2`, `-3`... suffix.
fn unique_name(name: String, taken: &mut BTreeSet<String>) -> String {
    let stem = name.trim_end_matches(".json").to_string();
    let mut candidate = name;
    let mut n = 1;
    while !taken.insert(candidate.to_lowercase()) {
        n += 1;
        candidate = format!("{stem}-{n}.json");
    }
    candidate
}

/// One checked homebrew file, ready to write.
struct Brew {
    name: String,
    bytes: Vec<u8>,
    importable: usize,
    unsupported: Vec<String>,
}

fn decode_json(input: &ImportFileInput) -> ApiResult<Vec<u8>> {
    let name = base_name(&input.file_name);
    let text = input.content_base64.trim();
    if text.len() / 4 * 3 > MAX_FILE_BYTES + 3 {
        return Err(ApiError::invalid(format!("{name} is too large to import.")));
    }
    let bytes = STANDARD
        .decode(text)
        .map_err(|_| ApiError::invalid("The file content is not valid base64."))?;
    if bytes.is_empty() || bytes.len() > MAX_FILE_BYTES {
        return Err(ApiError::invalid(format!(
            "{name} is empty or too large to import."
        )));
    }
    Ok(bytes)
}

/// Decode and check one `.json` file.
fn check_json(input: &ImportFileInput) -> ApiResult<Brew> {
    let name = base_name(&input.file_name);
    let bytes = decode_json(input)?;
    let value = homebrew::parse_json(&bytes)
        .ok_or_else(|| ApiError::invalid(format!("{name} is not valid JSON.")))?;
    if value.get("elements").is_some() {
        return Err(ApiError::invalid(format!(
            "{name} is a Borealis content file. Import it under Additional Content > Custom content."
        )));
    }
    if !homebrew::looks_like_5etools(&value) {
        return Err(ApiError::invalid(format!(
            "{name} is not a 5etools-format homebrew file: it has no _meta block and no \
             5etools-format entries."
        )));
    }
    let unsupported = value
        .as_object()
        .map(|m| homebrew::unsupported_kinds(m).into_keys().collect())
        .unwrap_or_default();
    Ok(Brew {
        name: name.to_string(),
        importable: homebrew::importable_entries(&value),
        bytes,
        unsupported,
    })
}

/// The display name of a staged set: the file, or the first file and how many more.
fn set_name(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_string(),
        [first, rest @ ..] => format!("{first} and {} more", rest.len()),
    }
}

/// Check every input; returns the homebrew files and the one zip, if any.
fn check_inputs(files: &[ImportFileInput]) -> ApiResult<(Vec<Brew>, Option<Vec<u8>>)> {
    if files.is_empty() {
        return Err(ApiError::invalid(
            "Choose at least one 5etools-format .json file.",
        ));
    }
    if files.len() > MAX_FILES {
        return Err(ApiError::invalid(format!(
            "Choose at most {MAX_FILES} files at once."
        )));
    }
    let mut brews = Vec::new();
    let mut zip = None;
    for input in files {
        if is_ext(&input.file_name, "zip") {
            if zip.is_some() {
                return Err(ApiError::invalid(
                    "Choose at most one .zip of a data/ folder at a time.",
                ));
            }
            zip = Some(decode_zip(input)?);
        } else if is_ext(&input.file_name, "json") {
            brews.push(check_json(input)?);
        } else {
            return Err(ApiError::invalid(format!(
                "{} is not a .json or .zip file.",
                base_name(&input.file_name)
            )));
        }
    }
    let importable: usize = brews.iter().map(|b| b.importable).sum();
    if zip.is_none() && importable == 0 {
        let kinds: BTreeSet<&str> = brews
            .iter()
            .flat_map(|b| b.unsupported.iter().map(String::as_str))
            .collect();
        let held = if kinds.is_empty() {
            "it holds no entries".to_string()
        } else {
            format!(
                "it only holds {}",
                kinds.into_iter().collect::<Vec<_>>().join(", ")
            )
        };
        return Err(ApiError::invalid(format!(
            "Nothing in {} can be imported: {held}.",
            set_name(&brews.iter().map(|b| b.name.as_str()).collect::<Vec<_>>())
        )));
    }
    Ok((brews, zip))
}

/// Write the zip's data files and the homebrew files under `dest` (the data folder).
fn write_stage(dest: &Path, brews: &[Brew], zip: Option<&[u8]>) -> ApiResult<usize> {
    let mut written = match zip {
        Some(bytes) => unpack_5etools_zip(bytes, dest)?,
        None => 0,
    };
    let dir = dest.join(HOMEBREW_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| ApiError::io("stage the files", e))?;
    let mut taken: BTreeSet<String> = std::fs::read_dir(&dir)
        .map_err(|e| ApiError::io("stage the files", e))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .collect();
    for brew in brews {
        let name = unique_name(safe_file_name(&brew.name), &mut taken);
        std::fs::write(dir.join(name), &brew.bytes)
            .map_err(|e| ApiError::io("stage the files", e))?;
        written += 1;
    }
    Ok(written)
}

impl AppState {
    /// Stage 5etools-format homebrew `.json` files (and at most one `.zip` of a `data/` folder they
    /// build on) and return a `5etools-folder` token for `scan_5etools_folder` and
    /// `import_5etools`. The previously staged files or zip are removed.
    ///
    /// # Errors
    /// `invalid_argument` for a file that is not 5etools-format JSON (a Borealis content file is
    /// sent to the custom content import), an unsafe or oversized zip, or a set with nothing to
    /// import; `io` if the files cannot be written.
    pub fn stage_5etools_files(&self, files: &[ImportFileInput]) -> ApiResult<PickedPathDto> {
        let (brews, zip) = check_inputs(files)?;
        let _guard = self.content_lock()?;
        let stage = self.paths.data.join(STAGE_DIR);
        if stage.exists() {
            std::fs::remove_dir_all(&stage).map_err(|e| ApiError::io("stage the files", e))?;
        }
        let dest = stage
            .join(uuid::Uuid::new_v4().simple().to_string())
            .join("data");
        let written = match write_stage(&dest, &brews, zip.as_deref()) {
            Ok(n) => n,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&stage);
                return Err(e);
            }
        };
        tracing::info!(
            files = written,
            homebrew = brews.len(),
            "5etools-format files staged"
        );
        let picked = self.picked.grant(dest, PickPurpose::FivetoolsFolder)?;
        let names: Vec<&str> = files.iter().map(|f| base_name(&f.file_name)).collect();
        let name = set_name(&names);
        Ok(PickedPathDto {
            name: name.clone(),
            path: name,
            ..picked
        })
    }
}

#[cfg(test)]
#[path = "content_fivetools_files_tests.rs"]
mod tests;
