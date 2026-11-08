//! `#[tauri::command]` wrappers for the notes view: the Obsidian vault (GitHub #15, [`crate::vault`]). The webview has
//! no file-system permission; it names vault files by vault-relative path, and every path is checked in the backend.
//! The vault folder itself is chosen with `set_vault_dir` (`commands_settings.rs`).

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use crate::vault::dto::{
    DeleteStatus, RenameDone, RenamePlan, VaultAttachment, VaultNote, VaultSave, VaultTree,
};
use crate::vault::index::{Backlink, SearchHit, TagCount};
use tauri::State;

type S<'a> = State<'a, AppState>;

/// `vault:changed` in the UI contract: a batch of changes made to the vault by other apps.
pub const EVENT_VAULT_CHANGED: &str = "borealis://vault-changed";
/// `app:lifecycle` in the UI contract (phones): the app went to the background or came back to the foreground.
pub const EVENT_APP_LIFECYCLE: &str = "borealis://app-lifecycle";
/// Longest web address opened from a note.
const MAX_URL_LEN: usize = 2048;

/// The vault and its folder tree.
#[tauri::command(async)]
pub fn vault_tree(state: S<'_>) -> ApiResult<VaultTree> {
    state.vault_tree()
}

/// A note (or another text file) as it is on disk.
#[tauri::command(async)]
pub fn vault_read(state: S<'_>, path: String) -> ApiResult<VaultNote> {
    state.vault_read(&path)
}

/// Save the user's edit; `base_hash` is the hash the note was loaded with (`None` creates it).
#[tauri::command(async)]
pub fn vault_save(
    state: S<'_>,
    path: String,
    text: String,
    base_hash: Option<String>,
) -> ApiResult<VaultSave> {
    state.vault_save(&path, &text, base_hash.as_deref())
}

/// Create a note (`name` may be a path from an unresolved link).
#[tauri::command(async)]
pub fn vault_create_note(
    state: S<'_>,
    name: Option<String>,
    from: Option<String>,
    folder: Option<String>,
) -> ApiResult<VaultNote> {
    state.vault_create_note(name.as_deref(), from.as_deref(), folder.as_deref())
}

/// Link a character to a vault note (B-222), or unlink it (`path` = `None`).
#[tauri::command(async)]
pub fn set_character_note(
    state: S<'_>,
    character_id: String,
    path: Option<String>,
) -> ApiResult<crate::dto::CharacterSnapshot> {
    state.set_character_note(&character_id, path.as_deref())
}

/// Create a note for a character from its details and link it (B-222).
#[tauri::command(async)]
pub fn vault_create_character_note(
    state: S<'_>,
    character_id: String,
) -> ApiResult<crate::dto::CharacterSnapshot> {
    state.vault_create_character_note(&character_id)
}

/// Delete a file to the OS trash, or for good when `permanent`.
#[tauri::command(async)]
pub fn vault_delete(state: S<'_>, path: String, permanent: bool) -> ApiResult<DeleteStatus> {
    state.vault_delete(&path, permanent)
}

/// What renaming would change (shown before the user confirms).
#[tauri::command(async)]
pub fn vault_rename_preview(state: S<'_>, path: String, new_name: String) -> ApiResult<RenamePlan> {
    state.vault_rename_preview(&path, &new_name)
}

/// Rename a file; `update_links` rewrites the links to it (only when the user confirmed).
#[tauri::command(async)]
pub fn vault_rename(
    state: S<'_>,
    path: String,
    new_name: String,
    update_links: bool,
) -> ApiResult<RenameDone> {
    state.vault_rename(&path, &new_name, update_links)
}

/// The file each link target in the note `from` opens (`null` = unresolved).
#[tauri::command(async)]
pub fn vault_resolve(
    state: S<'_>,
    from: String,
    targets: Vec<String>,
) -> ApiResult<Vec<Option<String>>> {
    state.vault_resolve(&from, &targets)
}

/// Notes linking to `path`.
#[tauri::command(async)]
pub fn vault_backlinks(state: S<'_>, path: String) -> ApiResult<Vec<Backlink>> {
    state.vault_backlinks(&path)
}

/// Every tag with its note count.
#[tauri::command(async)]
pub fn vault_tags(state: S<'_>) -> ApiResult<Vec<TagCount>> {
    state.vault_tags()
}

/// Search note names and text, or list the notes with `tag`.
#[tauri::command(async)]
pub fn vault_search(state: S<'_>, query: String, tag: Option<String>) -> ApiResult<Vec<SearchHit>> {
    state.vault_search(&query, tag.as_deref())
}

/// A vault image as a `data:` URL.
#[tauri::command(async)]
pub fn vault_image(state: S<'_>, path: String) -> ApiResult<String> {
    state.vault_image(&path)
}

/// The link text to insert in `from` for the vault file `to`, as `.obsidian/app.json` says.
#[tauri::command(async)]
pub fn vault_link_text(state: S<'_>, from: String, to: String, embed: bool) -> ApiResult<String> {
    state.vault_link_text(&from, &to, embed)
}

/// Copy the file behind a `vault-attachment` pick into the vault's attachment folder.
#[tauri::command(async)]
pub fn vault_attach(state: S<'_>, token: String, from: String) -> ApiResult<VaultAttachment> {
    state.vault_attach(&token, &from)
}

/// Desktop: start (`enabled`) or stop watching the vault; changes arrive as `borealis://vault-changed` events.
#[cfg(desktop)]
#[tauri::command(async)]
pub fn vault_watch(app: tauri::AppHandle, state: S<'_>, enabled: bool) -> ApiResult<()> {
    use tauri::Emitter as _;
    if !enabled {
        return state.vault_watch_stop();
    }
    state.vault_watch_start(move |change| {
        let _ = app.emit(EVENT_VAULT_CHANGED, &change);
    })
}

/// Phones: no vault watcher. The notes view reads the vault again when the app comes back ([`on_window_event`]).
#[cfg(not(desktop))]
#[tauri::command(async)]
pub fn vault_watch(enabled: bool) -> ApiResult<()> {
    let _ = enabled;
    Ok(())
}

/// Phones (B-221): tell the notes view when the app leaves or returns to the foreground, so it saves the open note and
/// then reads the vault again (Obsidian Sync or iCloud may have changed it meanwhile).
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    #[cfg(mobile)]
    {
        use tauri::Emitter as _;
        let foreground = match event {
            tauri::WindowEvent::Suspended => false,
            tauri::WindowEvent::Resumed => true,
            _ => return,
        };
        let _ = window.emit(
            EVENT_APP_LIFECYCLE,
            serde_json::json!({ "foreground": foreground }),
        );
    }
    #[cfg(desktop)]
    let _ = (window, event);
}

/// A web address from a note, if it is one Borealis opens: `http`, `https` or `mailto`, normalised.
///
/// # Errors
/// `invalid_argument` for anything else.
pub fn web_address(url: &str) -> ApiResult<String> {
    let refused = || ApiError::invalid("Borealis opens only web and e-mail links.");
    if url.len() > MAX_URL_LEN {
        return Err(refused());
    }
    let parsed = tauri::Url::parse(url.trim()).map_err(|_| refused())?;
    match parsed.scheme() {
        "http" | "https" if parsed.host_str().is_some() => Ok(parsed.into()),
        "mailto" => Ok(parsed.into()),
        _ => Err(refused()),
    }
}

/// Open a web link from a note in the system browser (the user clicked it).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command(async)]
pub fn vault_open_url(url: String) -> ApiResult<()> {
    crate::commands_app_update::open_external(&web_address(&url)?)
}

/// Phones: see the desktop command.
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command(async)]
pub fn vault_open_url(app: tauri::AppHandle, url: String) -> ApiResult<()> {
    crate::commands_app_update::open_external(&app, &web_address(&url)?)
}

#[cfg(test)]
mod tests {
    use super::web_address;

    #[test]
    fn only_web_and_mail_links_open() {
        assert_eq!(
            web_address("https://example.org/a b").unwrap(),
            "https://example.org/a%20b"
        );
        assert!(web_address("mailto:dm@example.org").is_ok());
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "obsidian://open?vault=x",
            "-a Calculator",
            "https://",
            "C:\\Windows",
        ] {
            assert!(web_address(bad).is_err(), "{bad}");
        }
    }
}
