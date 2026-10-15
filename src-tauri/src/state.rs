//! Application state: content store + engine, settings, and open-character sessions.

use crate::content_prefs::ContentPrefs;
use crate::dto::RollRecord;
use crate::error::{ApiError, ApiResult};
use crate::settings::AppSettings;
use borealis_character::Character;
use borealis_content::{ContentStore, ContentStoreBuilder, Element, PackManifest, Ruleset};
use borealis_engine::Engine;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Bundled content packs, in load order (later packs may extend earlier ones).
pub const BUNDLED_PACKS: [&str; 5] = [
    "core",
    "srd-5.1",
    "srd-5.2.1",
    "srd-5.1-monsters",
    "srd-5.2.1-monsters",
];

/// Where the application keeps its files.
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Directory holding the bundled `content/<pack>` folders.
    pub content_root: PathBuf,
    /// Per-user data directory.
    pub data: PathBuf,
}

impl AppPaths {
    /// User content packs.
    #[must_use]
    pub fn library(&self) -> PathBuf {
        self.data.join("library")
    }

    /// Saved characters (`<id>.json`).
    #[must_use]
    pub fn characters(&self) -> PathBuf {
        self.data.join("characters")
    }

    /// Log files (see `app_logs`).
    #[must_use]
    pub fn logs(&self) -> PathBuf {
        self.data.join(crate::app_logs::LOGS_DIR)
    }

    /// Content preferences (disabled packs, update indexes).
    #[must_use]
    pub fn prefs_file(&self) -> PathBuf {
        self.data.join("content-prefs.json")
    }

    /// The settings file.
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.data.join("settings.json")
    }
}

/// An immutable content snapshot: the store and the engine prepared for it.
#[derive(Debug)]
pub struct ContentHub {
    /// All loaded elements.
    pub store: ContentStore,
    /// Engine built for `store` with the settings in effect.
    pub engine: Engine,
    /// Builder holding every enabled pack (AI features load drafts on top of it).
    pub builder: ContentStoreBuilder,
    /// Problems found while loading user packs (bundled packs failing is fatal).
    pub load_warnings: Vec<String>,
}

impl ContentHub {
    /// Rulesets whose engine universes hold derived elements.
    const UNIVERSE_RULESETS: [Ruleset; 2] = [Ruleset::Dnd2024, Ruleset::Dnd2014];

    /// An element by id: the content store first, then the engine universes, which also hold
    /// elements the engine derives and the store never sees (one `Multiclass` entry per class).
    #[must_use]
    pub fn element(&self, id: &str) -> Option<&Element> {
        self.store.element(id).or_else(|| {
            Self::UNIVERSE_RULESETS
                .iter()
                .find_map(|r| self.engine.universe(*r).get(id).map(|u| &u.el))
        })
    }

    /// Engine-derived elements of a type (absent from the store), each id once.
    pub fn derived_elements<'a>(&'a self, ty: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        let mut seen = BTreeSet::new();
        Self::UNIVERSE_RULESETS
            .iter()
            .flat_map(move |r| self.engine.universe(*r).by_type(ty))
            .map(|u| &u.el)
            .filter(|el| self.store.element(&el.id).is_none())
            .filter(move |el| seen.insert(el.id.clone()))
    }

    /// Load the bundled packs and every pack folder in the user library.
    ///
    /// # Errors
    /// `content_error` if a bundled pack is missing or invalid.
    pub fn load(paths: &AppPaths, settings: &AppSettings) -> ApiResult<Self> {
        let prefs = ContentPrefs::load(&paths.prefs_file());
        Self::load_with(paths, settings, &prefs.disabled_packs)
    }

    /// Like [`ContentHub::load`] with an explicit set of disabled pack ids (`core` is never skipped).
    ///
    /// # Errors
    /// `content_error` if a bundled pack is missing or invalid.
    pub fn load_with(
        paths: &AppPaths,
        settings: &AppSettings,
        disabled: &BTreeSet<String>,
    ) -> ApiResult<Self> {
        Self::load_with_progress(paths, settings, disabled, &mut |_| {})
    }

    /// Like [`ContentHub::load_with`], reporting each pack and the engine build to `on_step`.
    ///
    /// # Errors
    /// `content_error` if a bundled pack is missing or invalid.
    pub fn load_with_progress(
        paths: &AppPaths,
        settings: &AppSettings,
        disabled: &BTreeSet<String>,
        on_step: &mut dyn FnMut(LoadStep),
    ) -> ApiResult<Self> {
        let bundled: Vec<(String, PathBuf)> = BUNDLED_PACKS
            .iter()
            .filter(|name| **name == CORE_PACK || !disabled.contains(**name))
            .map(|name| ((*name).to_string(), paths.content_root.join(name)))
            .collect();
        let custom: Vec<(String, PathBuf)> = user_pack_dirs(&paths.library())
            .into_iter()
            .filter(|dir| !pack_id_of(dir).is_some_and(|id| disabled.contains(&id)))
            .map(|dir| (dir_name(&dir), dir))
            .collect();
        let total = bundled.len() + custom.len();
        let mut builder = ContentStoreBuilder::new();
        for (i, (name, dir)) in bundled.iter().enumerate() {
            on_step(LoadStep::Pack {
                name: name.clone(),
                index: i,
                total,
                custom: false,
            });
            builder = builder.add_pack_dir(dir)?;
        }
        let mut load_warnings = Vec::new();
        for (i, (name, dir)) in custom.iter().enumerate() {
            on_step(LoadStep::Pack {
                name: name.clone(),
                index: bundled.len() + i,
                total,
                custom: true,
            });
            match builder.clone().add_pack_dir(dir) {
                Ok(next) => builder = next,
                Err(e) => {
                    tracing::warn!("skipping user pack {}: {e}", dir.display());
                    load_warnings.push(format!("A user content pack could not be loaded: {e}"));
                }
            }
        }
        on_step(LoadStep::Engine);
        let store = builder.clone().build();
        let engine = Engine::with_settings(&store, settings.engine_settings());
        Ok(Self {
            store,
            engine,
            builder,
            load_warnings,
        })
    }

    /// Rebuild the engine for new settings, reusing the store.
    #[must_use]
    pub fn with_settings(&self, settings: &AppSettings) -> Self {
        Self {
            store: self.store.clone(),
            engine: Engine::with_settings(&self.store, settings.engine_settings()),
            builder: self.builder.clone(),
            load_warnings: self.load_warnings.clone(),
        }
    }
}

/// A step of the content/state loading sequence, reported to a startup progress callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadStep {
    /// Creating the data folders.
    Directories,
    /// Reading the settings file.
    Settings,
    /// About to load one pack.
    Pack {
        /// Pack folder name.
        name: String,
        /// Zero-based position among the packs being loaded.
        index: usize,
        /// Number of packs being loaded.
        total: usize,
        /// A user-library pack rather than a bundled one.
        custom: bool,
    },
    /// Building the engine over the loaded content.
    Engine,
}

/// The always-on base pack.
pub const CORE_PACK: &str = "core";

/// The id declared by a pack folder's manifest.
pub(crate) fn pack_id_of(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("pack.json")).ok()?;
    PackManifest::from_json_str(&text).ok().map(|m| m.id)
}

fn dir_name(dir: &Path) -> String {
    dir.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

pub(crate) fn user_pack_dirs(library: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(library) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("pack.json").is_file())
        .collect();
    dirs.sort();
    dirs
}

/// An opened character with its editing state.
#[derive(Debug)]
pub struct Session {
    /// The character being edited.
    pub character: Character,
    /// Unsaved changes exist.
    pub dirty: bool,
    /// File the session saves to.
    pub path: PathBuf,
    /// Ability roll history (transient, not persisted).
    pub rolls: Vec<RollRecord>,
}

/// Everything the commands share.
#[derive(Debug)]
pub struct AppState {
    /// File locations.
    pub paths: AppPaths,
    content: RwLock<Arc<ContentHub>>,
    settings: RwLock<AppSettings>,
    sessions: Mutex<HashMap<String, Session>>,
    content_op: Mutex<()>,
    rng: Mutex<u64>,
    /// "Import anything" drafts (memory cache over the files in the data folder).
    #[cfg(desktop)]
    pub(crate) drafts: crate::import_drafts::DraftStore,
    /// Paths the user chose in backend-opened native dialogs, behind opaque tokens.
    pub picked: crate::picked_paths::PickedPaths,
    /// Outcome of the automatic content update at startup (Settings > "Automatic Updates").
    pub(crate) content_update: Mutex<crate::settings_dto::ContentUpdateStatus>,
}

impl AppState {
    /// Create the state: load settings and content, and ensure the data folders exist.
    ///
    /// # Errors
    /// `io_error` if the data folders cannot be created, `content_error` if bundled content fails.
    pub fn new(paths: AppPaths) -> ApiResult<Self> {
        Self::new_with_progress(paths, &mut |_| {})
    }

    /// Like [`AppState::new`], reporting each loading step to `on_step` (startup splash).
    ///
    /// # Errors
    /// As [`AppState::new`].
    pub fn new_with_progress(
        paths: AppPaths,
        on_step: &mut dyn FnMut(LoadStep),
    ) -> ApiResult<Self> {
        on_step(LoadStep::Directories);
        for dir in [paths.characters(), paths.library()] {
            std::fs::create_dir_all(&dir).map_err(|e| ApiError::io("create the data folder", e))?;
        }
        on_step(LoadStep::Settings);
        let settings = AppSettings::load(&paths.settings_file());
        let prefs = ContentPrefs::load(&paths.prefs_file());
        let hub =
            ContentHub::load_with_progress(&paths, &settings, &prefs.disabled_packs, on_step)?;
        Ok(Self::from_hub(paths, settings, hub))
    }

    /// Start with an empty content store when bundled content cannot be loaded; `reason` is kept as
    /// a load warning. Everything else (settings, characters folder) works as in [`AppState::new`].
    ///
    /// # Errors
    /// `io_error` if the data folders cannot be created.
    pub fn new_degraded(paths: AppPaths, reason: &str) -> ApiResult<Self> {
        for dir in [paths.characters(), paths.library()] {
            std::fs::create_dir_all(&dir).map_err(|e| ApiError::io("create the data folder", e))?;
        }
        let settings = AppSettings::load(&paths.settings_file());
        let store = ContentStoreBuilder::new();
        let hub = ContentHub {
            store: store.clone().build(),
            engine: Engine::with_settings(&store.clone().build(), settings.engine_settings()),
            builder: store,
            load_warnings: vec![reason.to_string()],
        };
        Ok(Self::from_hub(paths, settings, hub))
    }

    fn from_hub(paths: AppPaths, settings: AppSettings, hub: ContentHub) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |d| {
                u64::try_from(d.as_nanos() & u128::from(u64::MAX)).unwrap_or(1)
            });
        Self {
            paths,
            content: RwLock::new(Arc::new(hub)),
            settings: RwLock::new(settings),
            sessions: Mutex::new(HashMap::new()),
            content_op: Mutex::new(()),
            rng: Mutex::new(seed | 1),
            #[cfg(desktop)]
            drafts: crate::import_drafts::DraftStore::default(),
            picked: crate::picked_paths::PickedPaths::default(),
            content_update: Mutex::new(crate::settings_dto::ContentUpdateStatus::default()),
        }
    }

    /// The current content snapshot (cheap `Arc` clone).
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    pub fn hub(&self) -> ApiResult<Arc<ContentHub>> {
        let guard: RwLockReadGuard<'_, Arc<ContentHub>> = self.content.read()?;
        Ok(Arc::clone(&guard))
    }

    /// Replace the content snapshot.
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    pub fn set_hub(&self, hub: ContentHub) -> ApiResult<()> {
        let mut guard: RwLockWriteGuard<'_, Arc<ContentHub>> = self.content.write()?;
        *guard = Arc::new(hub);
        Ok(())
    }

    /// A copy of the current settings.
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    pub fn settings(&self) -> ApiResult<AppSettings> {
        Ok(self.settings.read()?.clone())
    }

    /// Replace the settings (callers validate and persist first).
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    pub fn set_settings(&self, settings: AppSettings) -> ApiResult<()> {
        *self.settings.write()? = settings;
        Ok(())
    }

    /// Lock the open-character map.
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    pub fn sessions(&self) -> ApiResult<MutexGuard<'_, HashMap<String, Session>>> {
        Ok(self.sessions.lock()?)
    }

    /// Serialise content-changing operations (imports, enable/disable, updates).
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    pub fn content_lock(&self) -> ApiResult<MutexGuard<'_, ()>> {
        Ok(self.content_op.lock()?)
    }

    /// Next pseudo-random value (xorshift64*; ability rolls do not need cryptographic quality).
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    pub fn next_random(&self) -> ApiResult<u64> {
        let mut state = self.rng.lock()?;
        let mut x = *state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        *state = x;
        Ok(x.wrapping_mul(0x2545_F491_4F6C_DD1D))
    }

    /// Roll one die with `sides` faces.
    ///
    /// # Errors
    /// `internal` if the lock is poisoned.
    pub fn roll_die(&self, sides: u32) -> ApiResult<u32> {
        let r = self.next_random()? >> 11;
        Ok(u32::try_from(r % u64::from(sides)).unwrap_or(0) + 1)
    }
}
