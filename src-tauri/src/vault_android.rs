//! Android: the Obsidian vault's document tree through the in-app Kotlin `VaultPlugin` (B-221, decision U17b), the
//! same in-app plugin pattern as `SharePlugin` ([`crate::native_share`]). The plugin picks the folder with
//! `ACTION_OPEN_DOCUMENT_TREE`, keeps a persisted URI permission, and does the Storage Access Framework calls on its own
//! background thread; [`crate::vault::tree_store`] keeps the notes view's rules on top of it.
//!
//! The plugin is registered before the app state exists, so it parks its handle here ([`VaultTreeHandle`]) and the
//! startup installs it into the state ([`install`]).

use crate::vault::tree_store::{DocEntry, DocumentTree, TreeError};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Runtime};

/// The Kotlin plugin, as a document tree.
pub struct AndroidTree<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> std::fmt::Debug for AndroidTree<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AndroidTree")
    }
}

/// The registered plugin, kept in app state until the startup installs it.
pub struct VaultTreeHandle(pub Arc<dyn DocumentTree>);

/// Registers `app.borealis.VaultPlugin` (Kotlin, in the Android project).
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("borealis-vault")
        .setup(|app, api| {
            let handle = api.register_android_plugin("app.borealis", "VaultPlugin")?;
            app.manage(PickHandle(handle.clone()));
            app.manage(VaultTreeHandle(Arc::new(AndroidTree(handle))));
            Ok(())
        })
        .build()
}

/// Give the app state the document tree access (startup, once the state exists).
pub fn install<R: Runtime, M: Manager<R>>(manager: &M, state: &crate::state::AppState) {
    match manager.try_state::<VaultTreeHandle>() {
        Some(h) => state.vault.set_document_tree(Arc::clone(&h.0)),
        None => tracing::warn!("vault: the document tree plugin is not registered"),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TreeArgs<'a> {
    tree: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PathArgs<'a> {
    tree: &'a str,
    path: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadArgs<'a> {
    tree: &'a str,
    path: &'a str,
    max: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WriteArgs<'a> {
    tree: &'a str,
    path: &'a str,
    data: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenameArgs<'a> {
    tree: &'a str,
    path: &'a str,
    name: &'a str,
}

#[derive(Deserialize)]
struct Entries {
    entries: Vec<DocEntry>,
}

#[derive(Deserialize)]
struct Stat {
    entry: Option<DocEntry>,
}

#[derive(Deserialize)]
struct Data {
    /// Base64; `None` when the file is larger than asked.
    data: Option<String>,
}

#[derive(Deserialize)]
struct PickedTree {
    /// The tree URI; `None` when the user cancelled.
    tree: Option<String>,
}

/// The plugin's reject codes (see `VaultPlugin.kt`).
fn tree_error(e: tauri::plugin::mobile::PluginInvokeError) -> TreeError {
    use tauri::plugin::mobile::PluginInvokeError;
    match e {
        PluginInvokeError::InvokeRejected(r) => match r.code.as_deref() {
            Some("notFound") => TreeError::NotFound,
            Some("exists") => TreeError::Exists,
            Some("notAFolder") => TreeError::NotAFolder,
            _ => TreeError::Failed(r.message.unwrap_or_else(|| "the call was refused".into())),
        },
        other => TreeError::Failed(other.to_string()),
    }
}

impl<R: Runtime> AndroidTree<R> {
    fn call<T: serde::de::DeserializeOwned>(
        &self,
        command: &str,
        args: impl Serialize,
    ) -> Result<T, TreeError> {
        self.0.run_mobile_plugin(command, args).map_err(tree_error)
    }

    fn done(&self, command: &str, args: impl Serialize) -> Result<(), TreeError> {
        self.call::<serde_json::Value>(command, args).map(|_| ())
    }
}

impl<R: Runtime> DocumentTree for AndroidTree<R> {
    fn check(&self, tree: &str) -> Result<(), TreeError> {
        self.done("check", TreeArgs { tree })
    }

    fn list(&self, tree: &str) -> Result<Vec<DocEntry>, TreeError> {
        self.call::<Entries>("list", TreeArgs { tree })
            .map(|e| e.entries)
    }

    fn children(&self, tree: &str, dir: &str) -> Result<Vec<DocEntry>, TreeError> {
        self.call::<Entries>("children", PathArgs { tree, path: dir })
            .map(|e| e.entries)
    }

    fn stat(&self, tree: &str, rel: &str) -> Result<Option<DocEntry>, TreeError> {
        self.call::<Stat>("stat", PathArgs { tree, path: rel })
            .map(|s| s.entry)
    }

    fn read(&self, tree: &str, rel: &str, max: u64) -> Result<Option<Vec<u8>>, TreeError> {
        let data = self.call::<Data>(
            "read",
            ReadArgs {
                tree,
                path: rel,
                max,
            },
        )?;
        data.data
            .map(|b| {
                STANDARD
                    .decode(b)
                    .map_err(|e| TreeError::Failed(e.to_string()))
            })
            .transpose()
    }

    fn write(&self, tree: &str, rel: &str, bytes: &[u8]) -> Result<(), TreeError> {
        self.done(
            "write",
            WriteArgs {
                tree,
                path: rel,
                data: STANDARD.encode(bytes),
            },
        )
    }

    fn create(&self, tree: &str, rel: &str, bytes: &[u8]) -> Result<(), TreeError> {
        self.done(
            "create",
            WriteArgs {
                tree,
                path: rel,
                data: STANDARD.encode(bytes),
            },
        )
    }

    fn rename(&self, tree: &str, rel: &str, name: &str) -> Result<(), TreeError> {
        self.done(
            "rename",
            RenameArgs {
                tree,
                path: rel,
                name,
            },
        )
    }

    fn delete(&self, tree: &str, rel: &str) -> Result<(), TreeError> {
        self.done("delete", PathArgs { tree, path: rel })
    }

    fn release(&self, tree: &str) {
        if let Err(e) = self.done("release", TreeArgs { tree }) {
            tracing::warn!("vault: the old folder permission was not released: {e}");
        }
    }
}

/// Show the system folder picker and wait for the user: the picked tree's URI (its permission is persisted), or
/// `None` when they cancel. Call off the main thread.
///
/// # Errors
/// When the plugin is missing or the picker cannot open.
pub fn pick_tree<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<Option<String>, String> {
    let plugin = app
        .try_state::<PickHandle<R>>()
        .ok_or_else(|| "the folder picker is not available".to_string())?;
    plugin
        .0
        .run_mobile_plugin::<PickedTree>("pickTree", ())
        .map(|p| p.tree)
        .map_err(|e| e.to_string())
}

/// The plugin handle for the picker (the document tree in [`VaultTreeHandle`] is type-erased).
pub struct PickHandle<R: Runtime>(pub PluginHandle<R>);
