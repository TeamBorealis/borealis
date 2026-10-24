//! Reading files the user handed over natively: a Finder drop on the window or the
//! "Load Character" open panel.
//!
//! A native drop arrives as a window event with real paths. They never reach the webview: each
//! acceptable file becomes a one-shot `dropped-file` token in [`PickedPaths`] and the UI gets only
//! tokens and file names. The UI then trades tokens for file contents with `take_picked_files`,
//! which consumes them, so a token can be read once, for its purpose, while it is live.

use crate::content_import::MAX_PACK_BYTES;
use crate::content_preview::OpenedFile;
use crate::error::{ApiError, ApiResult};
use crate::picked_paths::{file_name, PickPurpose, PickedPaths, MAX_PICKS};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Event carrying a native file drop on the main window (`files:dropped` in the UI contract).
pub const EVENT_FILE_DROP: &str = "borealis://file-drop";

/// What a drop may carry: rules content (XML, `.index`, `.zip`, JSON) and character files.
pub const DROP_EXTENSIONS: &[&str] = &["xml", "index", "zip", "json", "dnd5e"];
/// The extension of a Borealis bundle (`borealis-bundle/1`); opened by path through `bundle_open`.
pub const BUNDLE_EXTENSION: &str = "borealis";
/// What the "Load Character" panel may return.
pub const CHARACTER_EXTENSIONS: &[&str] = &["dnd5e", "json"];
/// Most bytes one `take_picked_files` call reads in total.
pub const MAX_READ_BYTES: u64 = MAX_PACK_BYTES as u64;

/// One accepted file of a drop: a token for `take_picked_files` and the name to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DroppedFile {
    pub token: String,
    pub name: String,
}

/// `borealis://file-drop` payload. `rejected` names the dropped items that cannot be imported.
/// `folders` holds at most one dropped folder, as a `content-folder` token: the same token the
/// "Import my content folder" picker mints, so it goes through the same preview.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDrop {
    pub files: Vec<DroppedFile>,
    pub folders: Vec<DroppedFile>,
    /// `.borealis` bundles, as `bundle-file` tokens for `bundle_open` (never read into the webview).
    pub bundles: Vec<DroppedFile>,
    pub rejected: Vec<String>,
}

/// The extensions a purpose may read through `take_picked_files`; `None` when it may not.
#[must_use]
pub fn readable_extensions(purpose: PickPurpose) -> Option<&'static [&'static str]> {
    match purpose {
        PickPurpose::DroppedFile => Some(DROP_EXTENSIONS),
        PickPurpose::CharacterFile => Some(CHARACTER_EXTENSIONS),
        _ => None,
    }
}

fn has_extension(path: &Path, allowed: &[&str]) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| allowed.contains(&e.as_str()))
}

/// Turn the paths of a native drop into `dropped-file` tokens, and the first dropped folder into
/// a `content-folder` token. Further folders, unknown file types and anything beyond
/// [`MAX_PICKS`] are listed as rejected (by name only).
///
/// # Errors
/// `internal` if the token store's lock is poisoned.
pub fn grant_drop(picks: &PickedPaths, paths: &[PathBuf]) -> ApiResult<FileDrop> {
    let mut drop = FileDrop::default();
    for path in paths {
        if path.is_dir() && drop.folders.is_empty() {
            let picked = picks.grant(path.clone(), PickPurpose::ContentFolder)?;
            drop.folders.push(DroppedFile {
                token: picked.token,
                name: picked.name,
            });
            continue;
        }
        if path.is_file()
            && has_extension(path, &[BUNDLE_EXTENSION])
            && drop.bundles.len() < MAX_PICKS
        {
            let picked = picks.grant(path.clone(), PickPurpose::BundleFile)?;
            drop.bundles.push(DroppedFile {
                token: picked.token,
                name: picked.name,
            });
            continue;
        }
        let ok =
            path.is_file() && has_extension(path, DROP_EXTENSIONS) && drop.files.len() < MAX_PICKS;
        if !ok {
            drop.rejected.push(file_name(path));
            continue;
        }
        let picked = picks.grant(path.clone(), PickPurpose::DroppedFile)?;
        drop.files.push(DroppedFile {
            token: picked.token,
            name: picked.name,
        });
    }
    Ok(drop)
}

/// Read and consume `tokens` minted for `purpose` (`dropped-file` or `character-file`). Every
/// token is checked before any is consumed; a call that fails consumes none.
///
/// # Errors
/// `invalid_argument` for a purpose that cannot be read this way, an unknown, expired, reused or
/// wrong-purpose token, a disallowed file type, an empty file or too many bytes in total.
pub fn take_picked_files(
    picks: &PickedPaths,
    purpose: PickPurpose,
    tokens: &[String],
) -> ApiResult<Vec<OpenedFile>> {
    let allowed = readable_extensions(purpose)
        .ok_or_else(|| ApiError::invalid("Those files cannot be read this way."))?;
    if tokens.is_empty() || tokens.len() > MAX_PICKS {
        return Err(ApiError::invalid("Choose between 1 and 32 files."));
    }
    let mut paths = Vec::with_capacity(tokens.len());
    let mut total = 0u64;
    for token in tokens {
        let path = picks.resolve(token, purpose)?;
        if !has_extension(&path, allowed) {
            return Err(ApiError::invalid(format!(
                "{} is not a file Borealis can open here.",
                file_name(&path)
            )));
        }
        let len = std::fs::metadata(&path)
            .map_err(|_| ApiError::invalid(format!("{} could not be read.", file_name(&path))))?
            .len();
        if len == 0 {
            return Err(ApiError::invalid(format!("{} is empty.", file_name(&path))));
        }
        total = total.saturating_add(len);
        if total > MAX_READ_BYTES {
            return Err(ApiError::invalid(
                "Those files are too large to open together.",
            ));
        }
        paths.push(path);
    }
    let mut out = Vec::with_capacity(paths.len());
    for (token, path) in tokens.iter().zip(paths) {
        picks.take(token, purpose)?;
        let bytes = std::fs::read(&path)?;
        out.push(OpenedFile {
            file_name: file_name(&path),
            content_base64: STANDARD.encode(bytes),
        });
    }
    Ok(out)
}

/// `WindowEvent::DragDrop` on the main window: mint tokens for the dropped files and tell the UI.
/// Hover events are ignored (the drop target is the whole window).
#[cfg(desktop)]
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    use tauri::{DragDropEvent, Emitter, Manager, WindowEvent};
    let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event else {
        return;
    };
    if window.label() != "main" || paths.is_empty() {
        return;
    }
    let Some(state) = window.try_state::<crate::state::AppState>() else {
        return;
    };
    match grant_drop(&state.picked, paths) {
        Ok(drop) => {
            let _ = window.emit_to("main", EVENT_FILE_DROP, drop);
        }
        Err(e) => tracing::warn!("file drop ignored: {e:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn files(dir: &Path, names: &[&str]) -> Vec<PathBuf> {
        names
            .iter()
            .map(|n| {
                let p = dir.join(n);
                fs::write(&p, format!("content of {n}")).unwrap();
                p
            })
            .collect()
    }

    #[test]
    fn a_dropped_bundle_gets_a_bundle_token_and_is_never_read_as_content() {
        let dir = tempfile::tempdir().unwrap();
        let paths = files(dir.path(), &["Mira.borealis", "Other.BOREALIS", "hero.dnd5e"]);
        let picks = PickedPaths::default();
        let drop = grant_drop(&picks, &paths).unwrap();
        let bundles: Vec<_> = drop.bundles.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(bundles, ["Mira.borealis", "Other.BOREALIS"]);
        let files: Vec<_> = drop.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(files, ["hero.dnd5e"]);
        let token = drop.bundles[0].token.clone();
        assert!(picks.resolve(&token, PickPurpose::DroppedFile).is_err());
        assert!(
            take_picked_files(&picks, PickPurpose::BundleFile, &[token.clone()]).is_err(),
            "bundles are opened by path, not read into the webview"
        );
        assert!(picks.resolve(&token, PickPurpose::BundleFile).is_ok());
    }

    #[test]
    fn a_drop_issues_tokens_and_names_only_for_allowed_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = files(
            dir.path(),
            &[
                "pack.XML",
                "set.index",
                "all.zip",
                "x.json",
                "hero.dnd5e",
                "id_rsa",
                "notes.txt",
            ],
        );
        let sub = dir.path().join("folder.xml");
        fs::create_dir(&sub).unwrap();
        paths.push(sub);
        let second = dir.path().join("second");
        fs::create_dir(&second).unwrap();
        paths.push(second);
        paths.push(dir.path().join("gone.xml"));
        let picks = PickedPaths::default();
        let drop = grant_drop(&picks, &paths).unwrap();
        let names: Vec<_> = drop.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            ["pack.XML", "set.index", "all.zip", "x.json", "hero.dnd5e"]
        );
        assert_eq!(drop.rejected, ["id_rsa", "notes.txt", "second", "gone.xml"]);
        let folders: Vec<_> = drop.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            folders,
            ["folder.xml"],
            "the first folder is offered as a content folder"
        );
        assert!(drop.folders[0].token.starts_with("pick_"));
        let folder_token = drop.folders[0].token.clone();
        assert!(
            picks
                .resolve(&folder_token, PickPurpose::ContentFolder)
                .is_ok(),
            "the token works exactly like the folder picker's"
        );
        assert!(picks
            .resolve(&folder_token, PickPurpose::DroppedFile)
            .is_err());
        let wire = serde_json::to_string(&drop).unwrap();
        assert!(!wire.contains(&*dir.path().to_string_lossy()), "{wire}");
        assert!(drop.files.iter().all(|f| f.token.starts_with("pick_")));
    }

    #[test]
    fn dropped_tokens_read_once_and_only_as_dropped_files() {
        let dir = tempfile::tempdir().unwrap();
        let picks = PickedPaths::default();
        let drop = grant_drop(&picks, &files(dir.path(), &["a.xml", "b.dnd5e"])).unwrap();
        let tokens: Vec<String> = drop.files.iter().map(|f| f.token.clone()).collect();
        for wrong in [
            PickPurpose::ImportFile,
            PickPurpose::CharacterFile,
            PickPurpose::ContentFolder,
        ] {
            assert!(take_picked_files(&picks, wrong, &tokens).is_err());
        }
        let read = take_picked_files(&picks, PickPurpose::DroppedFile, &tokens).unwrap();
        assert_eq!(read[0].file_name, "a.xml");
        assert_eq!(
            STANDARD.decode(&read[1].content_base64).unwrap(),
            b"content of b.dnd5e"
        );
        assert!(take_picked_files(&picks, PickPurpose::DroppedFile, &tokens).is_err());
    }

    #[test]
    fn a_failing_read_consumes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let picks = PickedPaths::default();
        let drop = grant_drop(&picks, &files(dir.path(), &["a.xml"])).unwrap();
        let good = drop.files[0].token.clone();
        let both = [good.clone(), "pick_forged".to_string()];
        assert!(take_picked_files(&picks, PickPurpose::DroppedFile, &both).is_err());
        assert!(take_picked_files(&picks, PickPurpose::DroppedFile, &[good]).is_ok());
    }

    #[test]
    fn raw_paths_and_other_purposes_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = files(dir.path(), &["a.xml"]).remove(0);
        let picks = PickedPaths::default();
        let raw = [path.to_string_lossy().into_owned()];
        assert!(take_picked_files(&picks, PickPurpose::DroppedFile, &raw).is_err());
        let import = picks.grant(path, PickPurpose::ImportFile).unwrap();
        assert!(
            take_picked_files(&picks, PickPurpose::ImportFile, &[import.token]).is_err(),
            "only dropped and character files are readable"
        );
        assert!(take_picked_files(&picks, PickPurpose::DroppedFile, &[]).is_err());
    }

    #[test]
    fn character_picks_must_be_character_files_and_not_empty() {
        let dir = tempfile::tempdir().unwrap();
        let picks = PickedPaths::default();
        let [hero, notes] =
            <[PathBuf; 2]>::try_from(files(dir.path(), &["h.dnd5e", "n.xml"])).unwrap();
        let empty = dir.path().join("e.json");
        fs::write(&empty, b"").unwrap();
        let grant = |p: &PathBuf| {
            picks
                .grant(p.clone(), PickPurpose::CharacterFile)
                .unwrap()
                .token
        };
        let (hero_t, notes_t, empty_t) = (grant(&hero), grant(&notes), grant(&empty));
        assert!(take_picked_files(&picks, PickPurpose::CharacterFile, &[notes_t]).is_err());
        assert!(take_picked_files(&picks, PickPurpose::CharacterFile, &[empty_t]).is_err());
        let read = take_picked_files(&picks, PickPurpose::CharacterFile, &[hero_t]).unwrap();
        assert_eq!(read[0].file_name, "h.dnd5e");
    }

    #[test]
    fn expired_drop_tokens_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let picks = PickedPaths::with_ttl(std::time::Duration::ZERO);
        let drop = grant_drop(&picks, &files(dir.path(), &["a.xml"])).unwrap();
        let t = [drop.files[0].token.clone()];
        assert!(take_picked_files(&picks, PickPurpose::DroppedFile, &t).is_err());
    }

    #[test]
    fn a_drop_beyond_the_pick_cap_rejects_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let names: Vec<String> = (0..=MAX_PICKS).map(|i| format!("f{i}.xml")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let picks = PickedPaths::default();
        let drop = grant_drop(&picks, &files(dir.path(), &refs)).unwrap();
        assert_eq!(drop.files.len(), MAX_PICKS);
        assert_eq!(drop.rejected, [format!("f{MAX_PICKS}.xml")]);
    }
}
