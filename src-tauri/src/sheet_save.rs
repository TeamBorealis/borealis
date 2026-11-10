//! "Save Character Sheet" on phones (B-148, tester video C15). Desktop saves through the native
//! save dialog (`commands_gear::save_sheet`) and is unchanged. Phones used to hand the bytes to a
//! web-view download that named no place and offered no way on.
//!
//! - **Android**: the system "Save to" picker (`ACTION_CREATE_DOCUMENT`, in the in-app
//!   `SharePlugin.saveFile`) writes the PDF where the user chose, then opens it in the PDF viewer,
//!   as Aurora's Save opens the file in the system viewer (printing happens there).
//! - **iOS**: the Files export picker (`tauri-plugin-dialog`'s save dialog, which exports the file
//!   of that name in the app's Documents folder) copies the PDF to the place the user chose.
//!
//! Either way the answer names where the file went, or carries neither a path nor bytes when the
//! user cancelled.

use crate::dto::ExportResult;
use crate::error::{ApiError, ApiResult};
use crate::sheet_api::SheetFile;
use std::path::Path;

/// The sheet bytes in a temp file under `dir` (deleted when dropped, after the platform has
/// copied it).
///
/// # Errors
/// `Io` when the folder or the file cannot be written.
pub fn temp_copy(dir: &Path, bytes: &[u8]) -> ApiResult<tempfile::NamedTempFile> {
    use std::io::Write as _;
    let io = |e| ApiError::io("prepare the sheet", e);
    std::fs::create_dir_all(dir).map_err(io)?;
    let mut tmp = tempfile::Builder::new()
        .suffix(".pdf")
        .tempfile_in(dir)
        .map_err(io)?;
    tmp.write_all(bytes).map_err(io)?;
    Ok(tmp)
}

/// What a phone save answers: where the file went (`path`, for the toast), or nothing at all when
/// the user cancelled. Never the bytes: the web view must not download them a second time.
#[must_use]
pub fn saved_result(file: &SheetFile, location: Option<String>) -> ExportResult {
    ExportResult {
        file_name: file.file_name.clone(),
        mime: file.mime.into(),
        path: location,
        data_base64: None,
    }
}

/// The file name alone (a sheet name never carries folders; this keeps it so).
#[must_use]
pub fn plain_name(file_name: &str) -> &str {
    Path::new(file_name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Character sheet.pdf")
}

/// Folder of the iCloud Drive container in a picked iOS path.
const ICLOUD_FOLDER: &str = "com~apple~CloudDocs";
/// Folder of the local file provider ("On My iPhone") in a picked iOS path.
const LOCAL_PROVIDER_FOLDER: &str = "File Provider Storage";

/// iOS: the place the Files app shows for a picked destination ("iCloud Drive/Sheets/Brannoc.pdf",
/// "On My iPhone/Brannoc.pdf"), else the destination's folder and name.
#[must_use]
pub fn ios_location(dest: &Path) -> String {
    let parts: Vec<String> = dest
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let under = |root: &str, label: &str| {
        parts.iter().position(|p| p == root).map(|i| {
            std::iter::once(label.to_string())
                .chain(parts[i + 1..].iter().cloned())
                .collect::<Vec<_>>()
                .join("/")
        })
    };
    under(ICLOUD_FOLDER, "iCloud Drive")
        .or_else(|| under(LOCAL_PROVIDER_FOLDER, "On My iPhone"))
        .unwrap_or_else(|| {
            parts
                .iter()
                .rev()
                .take(2)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("/")
        })
}

/// iOS: what the Save toast names for the picker's answer `dest` when the sheet was offered as
/// `name`. A new file or Replace answers the file, so the toast names it ("On My iPhone/Brannoc.pdf").
/// Keep Both (a file of that name was there) answers only the folder: the Files app then names the
/// new copy itself ("Brannoc 2.pdf" on a device, the same name on the simulator), and the app
/// cannot read that folder to learn it. The toast then names the folder and says why the file may
/// not be called `name`, rather than guess a name.
#[must_use]
pub fn ios_saved_location(dest: &Path, name: &str) -> String {
    let place = ios_location(dest);
    let names_a_file = dest
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    if names_a_file {
        place
    } else {
        format!("{place} (Files may have renamed it, as {name} was already there)")
    }
}

/// Android: the "Save to" picker, then the PDF viewer (see the module docs).
///
/// # Errors
/// `Io` when the temp copy cannot be written or the picker fails to open.
#[cfg(target_os = "android")]
pub fn save<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    file: &SheetFile,
    data_dir: &Path,
) -> ApiResult<ExportResult> {
    let tmp = temp_copy(&data_dir.join("sheet-share"), &file.bytes)?;
    let saved = crate::native_share::save_file(
        app,
        &crate::native_share::ShareFileArgs {
            path: &tmp.path().to_string_lossy(),
            file_name: plain_name(&file.file_name),
            mime: file.mime,
            title: "Save character sheet",
        },
    )?;
    let location = saved.saved.then(|| {
        saved
            .location
            .unwrap_or_else(|| plain_name(&file.file_name).to_string())
    });
    Ok(saved_result(file, location))
}

/// iOS: the Files export picker (see the module docs). The dialog plugin exports the file named
/// like the sheet in the app's Documents folder, so the PDF is written there first and removed
/// once the picker has copied it.
///
/// # Errors
/// `Io` when the PDF cannot be written to the Documents folder.
#[cfg(target_os = "ios")]
pub fn save<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    file: &SheetFile,
) -> ApiResult<ExportResult> {
    use tauri_plugin_dialog::DialogExt;
    let home = std::env::var_os("HOME")
        .ok_or_else(|| ApiError::invalid("The app's Documents folder is not available."))?;
    let docs = std::path::PathBuf::from(home).join("Documents");
    let name = plain_name(&file.file_name);
    let io = |e| ApiError::io("prepare the sheet", e);
    std::fs::create_dir_all(&docs).map_err(io)?;
    let src = docs.join(name);
    std::fs::write(&src, &file.bytes).map_err(io)?;
    let picked = app.dialog().file().set_file_name(name).blocking_save_file();
    if let Err(e) = std::fs::remove_file(&src) {
        tracing::warn!("could not remove the exported sheet copy: {e}");
    }
    let location = picked
        .and_then(|p| p.into_path().ok())
        .map(|p| ios_saved_location(&p, name));
    Ok(saved_result(file, location))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> SheetFile {
        SheetFile {
            file_name: "Brannoc.pdf".into(),
            mime: "application/pdf",
            bytes: b"%PDF-1.4".to_vec(),
        }
    }

    #[test]
    fn the_temp_copy_holds_the_sheet_and_goes_away_after() {
        let dir = tempfile::tempdir().unwrap();
        let tmp = temp_copy(&dir.path().join("sheet-share"), b"%PDF-1.4").unwrap();
        let path = tmp.path().to_path_buf();
        assert_eq!(std::fs::read(&path).unwrap(), b"%PDF-1.4");
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("pdf"));
        drop(tmp);
        assert!(!path.exists());
    }

    #[test]
    fn a_phone_save_names_the_place_and_never_returns_the_bytes() {
        let saved = saved_result(&sheet(), Some("Download/Brannoc.pdf".into()));
        assert_eq!(saved.path.as_deref(), Some("Download/Brannoc.pdf"));
        assert_eq!(saved.data_base64, None);
        assert_eq!(saved.mime, "application/pdf");
        let cancelled = saved_result(&sheet(), None);
        assert_eq!((cancelled.path, cancelled.data_base64), (None, None));
    }

    #[test]
    fn a_sheet_name_never_carries_folders() {
        assert_eq!(plain_name("Brannoc.pdf"), "Brannoc.pdf");
        assert_eq!(plain_name("../../Brannoc.pdf"), "Brannoc.pdf");
        assert_eq!(plain_name(""), "Character sheet.pdf");
    }

    #[test]
    fn ios_locations_read_as_the_files_app_shows_them() {
        let icloud = Path::new(
            "/private/var/mobile/Library/Mobile Documents/com~apple~CloudDocs/Sheets/Brannoc.pdf",
        );
        assert_eq!(ios_location(icloud), "iCloud Drive/Sheets/Brannoc.pdf");
        let local = Path::new(
            "/private/var/mobile/Containers/Shared/AppGroup/X/File Provider Storage/Brannoc.pdf",
        );
        assert_eq!(ios_location(local), "On My iPhone/Brannoc.pdf");
        assert_eq!(
            ios_location(Path::new("/var/elsewhere/Drive/Brannoc.pdf")),
            "Drive/Brannoc.pdf"
        );
    }

    #[test]
    fn an_ios_save_names_the_file_only_when_the_picker_answers_one() {
        let local = "/private/var/mobile/Containers/Shared/AppGroup/X/File Provider Storage";
        // a new file, or Replace: the picker answers the file
        let file = format!("{local}/Brannoc.pdf");
        assert_eq!(
            ios_saved_location(Path::new(&file), "Brannoc.pdf"),
            "On My iPhone/Brannoc.pdf"
        );
        let upper = format!("{local}/Sheets/Brannoc.PDF");
        assert_eq!(
            ios_saved_location(Path::new(&upper), "Brannoc.pdf"),
            "On My iPhone/Sheets/Brannoc.PDF"
        );
        // Keep Both: the picker answers the folder (with or without a trailing slash), and the
        // Files app may name the copy "Brannoc 2.pdf", so no file name is claimed
        let keep_both =
            "On My iPhone (Files may have renamed it, as Brannoc.pdf was already there)";
        assert_eq!(
            ios_saved_location(Path::new(local), "Brannoc.pdf"),
            keep_both
        );
        assert_eq!(
            ios_saved_location(Path::new(&format!("{local}/")), "Brannoc.pdf"),
            keep_both
        );
        let icloud = "/private/var/mobile/Library/Mobile Documents/com~apple~CloudDocs/Sheets/";
        assert_eq!(
            ios_saved_location(Path::new(icloud), "Brannoc.pdf"),
            "iCloud Drive/Sheets (Files may have renamed it, as Brannoc.pdf was already there)"
        );
    }
}
