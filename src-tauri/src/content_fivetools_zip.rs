//! A 5etools `data/` folder handed over as a `.zip`. Phones have no folder picker, so the 5etools
//! tab takes a zip through the WebView file chooser; desktop accepts the same zip.
//!
//! The archive is unpacked into a fresh scratch folder in the data directory. Only the JSON files
//! the 5etools importer reads are taken (root `*.json`, `class/`, `spells/`, `homebrew/` and the
//! spell source lookup); everything else is ignored. An entry name that could escape the folder
//! refuses the whole archive, and entry count, file size and total size are capped. The folder is
//! then granted as a `5etools-folder` pick, so scan and import run exactly as for a chosen folder.
//! Deterministic: no AI is involved.

use crate::content_preview::ImportFileInput;
use crate::error::{ApiError, ApiResult};
use crate::picked_paths::{PickPurpose, PickedPathDto};
use crate::state::AppState;
use crate::zip_read::{entry_names, for_each_entry, ZipLimits};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::collections::BTreeSet;
use std::path::Path;

/// Largest `.zip` accepted (compressed bytes).
pub const MAX_ZIP_BYTES: usize = 64 * 1024 * 1024;
/// Most entries the archive may list.
pub const MAX_ARCHIVE_ENTRIES: usize = 20_000;
/// Most files taken from the archive.
pub const MAX_TAKEN_FILES: usize = 2_000;
/// Most decompressed bytes taken in total.
pub const MAX_UNPACKED_BYTES: usize = 256 * 1024 * 1024;
/// Largest single file taken (the importer's own per-file cap).
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

/// Scratch folder (in the data directory) the latest zip is unpacked into. The `.tmp-` prefix lets
/// the startup sweep remove a stale one.
const STAGE_DIR: &str = ".tmp-5etools-zip";
/// Every 5etools `data/` folder has this file at its root.
const MARKER: &str = "books.json";
/// Sub-folders whose `*.json` files the importer reads.
const DATA_SUBDIRS: [&str; 4] = ["class", "spells", "bestiary", "homebrew"];
/// The one generated file the importer reads.
const SPELL_LOOKUP: &str = "generated/gendata-spell-source-lookup.json";

const LIMITS: ZipLimits = ZipLimits {
    max_entries: MAX_ARCHIVE_ENTRIES,
    max_total_bytes: MAX_UNPACKED_BYTES,
    max_entry_bytes: MAX_FILE_BYTES,
    refuse_unsafe: true,
};

fn not_a_data_zip() -> ApiError {
    ApiError::invalid(
        "This .zip does not contain a 5etools data folder. Zip your 5etools data/ folder and choose that file.",
    )
}

/// macOS resource forks (`__MACOSX/`, `._name`) and other hidden files.
fn is_noise(name: &str) -> bool {
    name.split('/')
        .any(|seg| seg == "__MACOSX" || seg.starts_with('.'))
}

/// The folder prefix (`""` or ending in `/`) that holds the data files: the shallowest folder with
/// `books.json`, else the shallowest folder named `data`, else the archive root.
#[must_use]
pub fn data_root(names: &[String]) -> String {
    let shallowest = |found: Vec<String>| {
        found.into_iter().min_by(|a, b| {
            a.matches('/')
                .count()
                .cmp(&b.matches('/').count())
                .then(a.cmp(b))
        })
    };
    let marked = names
        .iter()
        .filter(|n| !is_noise(n))
        .filter(|n| n.rsplit('/').next() == Some(MARKER))
        .map(|n| n[..n.len() - MARKER.len()].to_string())
        .collect();
    if let Some(root) = shallowest(marked) {
        return root;
    }
    let data_dirs = names
        .iter()
        .filter(|n| !is_noise(n))
        .filter_map(|n| {
            let mut end = 0;
            for seg in n.split('/') {
                end += seg.len() + 1;
                if seg == "data" && end <= n.len() {
                    return Some(n[..end].to_string());
                }
            }
            None
        })
        .collect();
    shallowest(data_dirs).unwrap_or_default()
}

/// True for a path (relative to the data root) the importer reads.
#[must_use]
pub fn is_data_file(rel: &str) -> bool {
    if !rel.ends_with(".json") || is_noise(rel) {
        return false;
    }
    match rel.split_once('/') {
        None => true,
        Some((dir, file)) => {
            rel == SPELL_LOOKUP || (DATA_SUBDIRS.contains(&dir) && !file.contains('/'))
        }
    }
}

/// `name` relative to the data root, when it is a file the importer reads.
fn relative<'a>(root: &str, name: &'a str) -> Option<&'a str> {
    name.strip_prefix(root).filter(|r| is_data_file(r))
}

/// Unpack the 5etools data files of `zip` into `dest` (created if missing). Returns how many
/// files were written.
///
/// # Errors
/// `invalid_argument` for a damaged, unsafe or oversized archive, or one without 5etools data.
pub fn unpack_5etools_zip(zip: &[u8], dest: &Path) -> ApiResult<usize> {
    let names = entry_names(zip, &LIMITS)?;
    let root = data_root(&names);
    let taken: Vec<&str> = names.iter().filter_map(|n| relative(&root, n)).collect();
    if taken.len() > MAX_TAKEN_FILES {
        return Err(ApiError::invalid("That .zip file holds too many files."));
    }
    // Phones and Macs often have case-insensitive file systems: two names that differ only in
    // case would overwrite each other, so the archive is refused instead.
    let mut seen = BTreeSet::new();
    if !taken.iter().all(|r| seen.insert(r.to_lowercase())) {
        return Err(ApiError::invalid(
            "That .zip file lists the same file twice.",
        ));
    }
    if taken.is_empty() {
        return Err(not_a_data_zip());
    }
    std::fs::create_dir_all(dest).map_err(|e| ApiError::io("unpack the .zip", e))?;
    let mut written = 0;
    for_each_entry(
        zip,
        &LIMITS,
        |name| relative(&root, name).is_some(),
        |name, bytes| {
            let r = relative(&root, name).ok_or_else(not_a_data_zip)?;
            let path = r.split('/').fold(dest.to_path_buf(), |p, seg| p.join(seg));
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| ApiError::io("unpack the .zip", e))?;
            }
            std::fs::write(&path, bytes).map_err(|e| ApiError::io("unpack the .zip", e))?;
            written += 1;
            Ok(())
        },
    )?;
    Ok(written)
}

fn decode_zip(input: &ImportFileInput) -> ApiResult<Vec<u8>> {
    if !input.file_name.to_lowercase().ends_with(".zip") {
        return Err(ApiError::invalid(
            "Choose a .zip of your 5etools data/ folder.",
        ));
    }
    let text = input.content_base64.trim();
    if text.len() / 4 * 3 > MAX_ZIP_BYTES + 3 {
        return Err(ApiError::invalid("That .zip file is too large to import."));
    }
    let bytes = STANDARD
        .decode(text)
        .map_err(|_| ApiError::invalid("The file content is not valid base64."))?;
    if bytes.is_empty() || bytes.len() > MAX_ZIP_BYTES {
        return Err(ApiError::invalid(
            "That .zip file is empty or too large to import.",
        ));
    }
    Ok(bytes)
}

impl AppState {
    /// Unpack a `.zip` of a 5etools `data/` folder and return a `5etools-folder` token for it, to
    /// pass to `scan_5etools_folder` and `import_5etools`. The previous unpacked zip is removed.
    ///
    /// # Errors
    /// `invalid_argument` for a file that is not a safe 5etools data zip; `io` if it cannot be
    /// written.
    pub fn stage_5etools_zip(&self, input: &ImportFileInput) -> ApiResult<PickedPathDto> {
        let bytes = decode_zip(input)?;
        let _guard = self.content_lock()?;
        let stage = self.paths.data.join(STAGE_DIR);
        if stage.exists() {
            std::fs::remove_dir_all(&stage).map_err(|e| ApiError::io("unpack the .zip", e))?;
        }
        let dest = stage
            .join(uuid::Uuid::new_v4().simple().to_string())
            .join("data");
        let files = match unpack_5etools_zip(&bytes, &dest) {
            Ok(n) => n,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&stage);
                return Err(e);
            }
        };
        tracing::info!(files, zip_bytes = bytes.len(), "5etools zip unpacked");
        let picked = self.picked.grant(dest, PickPurpose::FivetoolsFolder)?;
        let name = crate::content_import::stem(&input.file_name).to_string() + ".zip";
        Ok(PickedPathDto {
            name: name.clone(),
            path: name,
            ..picked
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip_read::test_zip::build;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn finds_the_data_root() {
        assert_eq!(
            data_root(&names(&["data/books.json", "data/races.json"])),
            "data/"
        );
        assert_eq!(data_root(&names(&["books.json", "class/class-x.json"])), "");
        assert_eq!(
            data_root(&names(&[
                "5etools-src/package.json",
                "5etools-src/data/books.json",
                "5etools-src/data/homebrew/books.json",
                "__MACOSX/data/._books.json",
            ])),
            "5etools-src/data/"
        );
        assert_eq!(data_root(&names(&["x/data/homebrew/a.json"])), "x/data/");
        assert_eq!(data_root(&names(&["races.json"])), "");
    }

    #[test]
    fn takes_only_the_files_the_importer_reads() {
        for ok in [
            "races.json",
            "books.json",
            "class/class-wizard.json",
            "spells/spells-phb.json",
            "homebrew/my.json",
            "bestiary/bestiary-mm.json",
            "bestiary/template.json",
            SPELL_LOOKUP,
        ] {
            assert!(is_data_file(ok), "{ok}");
        }
        for no in [
            "bestiary/deep/bestiary-x.json",
            "class/deep/class-x.json",
            "generated/gendata-tables.json",
            "class/._class-x.json",
            ".hidden.json",
            "readme.md",
            "img/x.png",
        ] {
            assert!(!is_data_file(no), "{no}");
        }
    }

    #[test]
    fn unpacks_a_data_zip_and_ignores_everything_else() {
        let dir = tempfile::tempdir().unwrap();
        let z = build(
            &[
                ("data/books.json", b"{\"book\":[]}"),
                ("data/races.json", b"{\"race\":[]}"),
                ("data/class/class-x.json", b"{}"),
                ("data/bestiary/bestiary-mm.json", b"{}"),
                ("data/img/a.png", b"png"),
                ("__MACOSX/data/._races.json", b"junk"),
                ("data/", b""),
            ],
            true,
        );
        let n = unpack_5etools_zip(&z, dir.path()).unwrap();
        assert_eq!(n, 4);
        assert_eq!(
            std::fs::read(dir.path().join("races.json")).unwrap(),
            b"{\"race\":[]}"
        );
        assert!(dir.path().join("class/class-x.json").is_file());
        // creatures are read (they fill companion selects)
        assert!(dir.path().join("bestiary/bestiary-mm.json").is_file());
        assert!(!dir.path().join("img").exists());
    }

    #[test]
    fn zip_slip_refuses_the_archive_and_writes_nothing() {
        let outer = tempfile::tempdir().unwrap();
        let dest = outer.path().join("dest");
        for evil in ["data/../../evil.json", "../books.json", "/tmp/evil.json"] {
            let z = build(&[("data/books.json", b"{}"), (evil, b"{}")], false);
            let err = unpack_5etools_zip(&z, &dest).unwrap_err();
            assert!(err.message.contains("outside"), "{evil}: {}", err.message);
            assert!(!dest.exists());
            assert!(!outer.path().join("evil.json").exists());
        }
    }

    #[test]
    fn caps_and_duplicates_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let many: Vec<String> = (0..=MAX_TAKEN_FILES)
            .map(|i| format!("data/homebrew/h{i}.json"))
            .collect();
        let entries: Vec<(&str, &[u8])> = many.iter().map(|n| (n.as_str(), &b"{}"[..])).collect();
        assert!(unpack_5etools_zip(&build(&entries, false), dir.path()).is_err());
        let dup = build(&[("Races.json", b"{}"), ("races.json", b"{}")], false);
        assert!(unpack_5etools_zip(&dup, dir.path())
            .unwrap_err()
            .message
            .contains("twice"));
    }

    #[test]
    fn a_zip_without_5etools_data_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let z = build(
            &[("notes/readme.md", b"hi"), ("pack.xml", b"<elements/>")],
            false,
        );
        let err = unpack_5etools_zip(&z, dir.path()).unwrap_err();
        assert!(
            err.message.contains("5etools data folder"),
            "{}",
            err.message
        );
    }

    fn fixture_zip(prefix: &str) -> Vec<u8> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/borealis-import-5etools/tests/fixtures/data");
        let mut files = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().map(Result::unwrap) {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let rel = p
                        .strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.push((format!("{prefix}{rel}"), std::fs::read(&p).unwrap()));
                }
            }
        }
        files.sort();
        let entries: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        build(&entries, true)
    }

    fn app() -> (tempfile::TempDir, AppState) {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::state::AppPaths {
            content_root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: dir.path().join("data"),
        };
        (dir, AppState::new(paths).unwrap())
    }

    #[test]
    fn a_staged_zip_scans_and_imports_like_the_folder() {
        let (_dir, app) = app();
        let folder = app
            .picked
            .grant(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../crates/borealis-import-5etools/tests/fixtures/data"),
                PickPurpose::FivetoolsFolder,
            )
            .unwrap();
        let want = app.scan_5etools_folder(&folder.token).unwrap();
        let input = ImportFileInput {
            file_name: "my-data.zip".into(),
            content_base64: STANDARD.encode(fixture_zip("data/")),
        };
        let staged = app.stage_5etools_zip(&input).unwrap();
        assert_eq!(staged.name, "my-data.zip");
        let got = app.scan_5etools_folder(&staged.token).unwrap();
        let abbrs = |s: &crate::content_dto::FiveToolsScan| -> Vec<String> {
            s.sources.iter().map(|x| x.abbr.clone()).collect()
        };
        assert_eq!(abbrs(&got), abbrs(&want));
        assert!(!got.sources.is_empty());
        let request = |folder: &str| crate::content_dto::FiveToolsImportRequest {
            folder: folder.to_string(),
            sources: abbrs(&want),
            include_fluff: false,
        };
        let sink = crate::content_packs::NullSink;
        let from_zip = app.import_5etools(&request(&staged.token), &sink).unwrap();
        let from_folder = app.import_5etools(&request(&folder.token), &sink).unwrap();
        assert!(from_zip.converted > 0);
        assert_eq!(from_zip.converted, from_folder.converted);
        assert_eq!(from_folder.replaced, from_folder.packs.len());

        // A second zip replaces the first unpacked folder; its token stops working.
        let again = app.stage_5etools_zip(&input).unwrap();
        assert!(app.scan_5etools_folder(&staged.token).is_err());
        assert!(app.scan_5etools_folder(&again.token).is_ok());
    }

    #[test]
    fn a_refused_zip_leaves_no_scratch_behind() {
        let (_dir, app) = app();
        let evil = build(&[("data/books.json", b"{}"), ("../x.json", b"{}")], false);
        let input = ImportFileInput {
            file_name: "evil.zip".into(),
            content_base64: STANDARD.encode(evil),
        };
        assert!(app.stage_5etools_zip(&input).is_err());
        assert!(!app.paths.data.join(STAGE_DIR).exists());
    }

    #[test]
    fn only_zip_files_are_accepted() {
        let input = ImportFileInput {
            file_name: "races.json".into(),
            content_base64: STANDARD.encode(b"{}"),
        };
        assert!(decode_zip(&input).unwrap_err().message.contains(".zip"));
    }
}
