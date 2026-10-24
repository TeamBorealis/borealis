//! Bundles the OS handed to the app ("open with", a share into Borealis). They are queued as paths
//! (never read into the webview) until the UI asks for them; the UI then gets `bundle-file` tokens
//! it passes to `bundle_open`.
//!
//! Android's activity reads a shared file into memory (it only has a content URI), so those bytes
//! are written to `<data>/sync/incoming` first and queued like any other path.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_bundle::Bundle;

use crate::content_preview::OpenedFile;
use crate::picked_files::BUNDLE_EXTENSION;
use crate::state::AppState;

/// Most queued paths kept; the oldest is dropped beyond this.
const MAX_QUEUED: usize = 16;

static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// True for a file name ending in `.borealis` (any case).
#[must_use]
pub fn is_bundle_name(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(BUNDLE_EXTENSION))
}

fn queue(path: PathBuf) {
    if let Ok(mut q) = OPENED.lock() {
        if q.len() >= MAX_QUEUED {
            q.remove(0);
        }
        q.push(path);
    }
}

/// Queue a `.borealis` file the OS opened with Borealis. Anything else is ignored; answers
/// whether the path was a bundle (so the caller does not also read it as content).
pub fn push_opened_path(path: &Path) -> bool {
    let named = path
        .file_name()
        .is_some_and(|n| is_bundle_name(&n.to_string_lossy()));
    if !named {
        return false;
    }
    if path.is_file() {
        queue(path.to_path_buf());
    }
    true
}

/// The paths queued since the last call, oldest first.
#[must_use]
pub fn take_opened_paths() -> Vec<PathBuf> {
    OPENED
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default()
}

/// A name with no extension at all, like the opaque ids some providers hand out.
fn has_no_extension(name: &str) -> bool {
    !name.contains('.')
}

/// Whether `bytes` are a readable Borealis bundle (used only for extension-less names).
fn looks_like_bundle(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK") && Bundle::open(Cursor::new(bytes)).is_ok()
}

/// Take the bundles out of `files` (Android's byte-carrying opens): a `.borealis` name, or a file
/// with no extension that opens as a bundle (the Downloads provider names files by opaque id).
/// Their bytes go to `<data>/sync/incoming` and are queued as paths; the rest is returned.
pub fn claim_bundles(state: &AppState, files: Vec<OpenedFile>) -> Vec<OpenedFile> {
    let mut rest = Vec::with_capacity(files.len());
    for file in files {
        let Ok(bytes) = STANDARD.decode(&file.content_base64) else {
            rest.push(file);
            continue;
        };
        let is_bundle = is_bundle_name(&file.file_name)
            || (has_no_extension(&file.file_name) && looks_like_bundle(&bytes));
        if !is_bundle {
            rest.push(file);
            continue;
        }
        let dir = super::incoming_dir(state);
        let path = dir.join(format!("open-{}.borealis", uuid::Uuid::new_v4().simple()));
        match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, bytes)) {
            Ok(()) => queue(path),
            Err(e) => tracing::warn!("could not keep an opened bundle: {e}"),
        }
    }
    rest
}
