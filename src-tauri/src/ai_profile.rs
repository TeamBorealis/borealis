//! The Borealis AI profile on the desktop: install/verify/repair at startup and the per-run MCP command.
//!
//! The profile assets ship as app resources (`ai-profile/`), are laid down read-only in
//! `<data>/ai-profile` and re-checked on every launch. The MCP server is this same
//! executable launched with `--mcp` (see `mcp_mode`).

use crate::state::{pack_id_of, user_pack_dirs, AppState, BUNDLED_PACKS, CORE_PACK};
use borealis_ai::profile::{install_profile, verify_profile};
use borealis_ai::{McpCommand, ProfileConfig, ProfileStatus};
use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

/// Marker (next to the manifest) holding the app version the profile was installed for.
const VERSION_MARKER: &str = "installed-for-app-version";

/// What startup did to the profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileOutcome {
    /// Nothing was installed yet.
    Installed,
    /// The app version changed; assets were refreshed.
    Updated,
    /// Everything matched.
    Verified,
    /// Files were modified, missing or extra; the profile was reinstalled. Lists the paths.
    Repaired(Vec<String>),
}

/// Install, update, verify or repair the profile under `target`.
///
/// # Errors
/// I/O errors while (re)installing.
pub fn ensure_profile(
    resources: &Path,
    target: &Path,
    app_version: &str,
) -> io::Result<ProfileOutcome> {
    let marker = target.join(VERSION_MARKER);
    let stored = std::fs::read_to_string(&marker).ok();
    let present = target.join("manifest.json").is_file() && target.join("profile").is_dir();
    let outcome = if !present {
        install_profile(resources, target)?;
        ProfileOutcome::Installed
    } else if stored.as_deref().map(str::trim) != Some(app_version) {
        install_profile(resources, target)?;
        ProfileOutcome::Updated
    } else {
        match verify_profile(target) {
            ProfileStatus::Ok => ProfileOutcome::Verified,
            ProfileStatus::Tampered(files) => {
                install_profile(resources, target)?;
                ProfileOutcome::Repaired(files)
            }
        }
    };
    if outcome != ProfileOutcome::Verified {
        std::fs::write(&marker, app_version)?;
    }
    Ok(outcome)
}

/// The MCP launch command: `<exe> --mcp --content-dir <d>... [--packs a,b]`.
#[must_use]
pub fn mcp_command(
    exe: &Path,
    content_dirs: &[PathBuf],
    packs: Option<&BTreeSet<String>>,
) -> McpCommand {
    let mut args = vec!["--mcp".to_string()];
    for d in content_dirs {
        args.push("--content-dir".into());
        args.push(d.to_string_lossy().into_owned());
    }
    if let Some(p) = packs {
        args.push("--packs".into());
        args.push(p.iter().cloned().collect::<Vec<_>>().join(","));
    }
    McpCommand {
        program: exe.to_path_buf(),
        args,
    }
}

/// [`mcp_command`] for one ask that shares one campaign's notes: adds `--session-scope <campaign>`.
/// Only the ask the user ticked "Include notes from <campaign>" for gets it; the default launch has
/// no scope, so `search_sessions` does not exist for the model.
#[must_use]
pub fn mcp_command_scoped(
    exe: &Path,
    content_dirs: &[PathBuf],
    packs: Option<&BTreeSet<String>>,
    session_campaign: Option<&str>,
) -> McpCommand {
    let mut command = mcp_command(exe, content_dirs, packs);
    if let Some(id) = session_campaign {
        command.args.push("--session-scope".into());
        command.args.push(id.to_string());
    }
    command
}

/// Everything needed to build a [`ProfileConfig`] per run (the pack list changes with the user's choices).
#[derive(Debug, Clone)]
pub struct ProfileBase {
    /// Profile install root.
    pub dir: PathBuf,
    /// This executable.
    pub exe: PathBuf,
    /// Bundled content root and the user library.
    pub content_dirs: Vec<PathBuf>,
}

impl ProfileBase {
    /// The profile config for one run.
    #[must_use]
    pub fn config(&self, packs: Option<&BTreeSet<String>>) -> ProfileConfig {
        ProfileConfig::new(
            self.dir.clone(),
            mcp_command(&self.exe, &self.content_dirs, packs),
        )
    }

    /// [`Self::config`] for an ask that was granted `session_campaign`'s notes.
    #[must_use]
    pub fn config_with_sessions(
        &self,
        packs: Option<&BTreeSet<String>>,
        session_campaign: Option<&str>,
    ) -> ProfileConfig {
        ProfileConfig::new(
            self.dir.clone(),
            mcp_command_scoped(&self.exe, &self.content_dirs, packs, session_campaign),
        )
    }
}

impl AppState {
    /// Ids of the packs that are not disabled (bundled and user library), `core` always included.
    #[must_use]
    pub fn enabled_pack_ids(&self) -> BTreeSet<String> {
        let prefs = crate::content_prefs::ContentPrefs::load(&self.paths.prefs_file());
        let mut ids: BTreeSet<String> = BUNDLED_PACKS
            .iter()
            .filter(|p| **p == CORE_PACK || !prefs.disabled_packs.contains(**p))
            .map(|p| (*p).to_string())
            .collect();
        for dir in user_pack_dirs(&self.paths.library()) {
            if let Some(id) = pack_id_of(&dir).filter(|i| !prefs.disabled_packs.contains(i)) {
                ids.insert(id);
            }
        }
        ids
    }
}
