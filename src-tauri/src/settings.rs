//! Application settings: the `AppSettings` contract type, persistence and patch merging.

use crate::dto::AbilityMethodDto;
use crate::error::{ApiError, ApiResult};
use borealis_content::Ruleset;
use borealis_engine::EngineSettings;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// Sheet export preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SheetSettings {
    /// Fillable PDF form fields (default off: the PDF is flattened); on keeps the fields editable.
    pub form_fillable: bool,
    /// `colour` (default) or `ink` (monochrome export).
    pub theme: SheetThemeSetting,
    /// `auto` (default, follows the ruleset), `2014` or `2024`.
    pub edition: SheetEditionSetting,
    pub style_formatting: bool,
    pub swap_ability_positions: bool,
    pub include_non_prepared_spells: bool,
    pub cards: CardSettings,
    pub auto_generate: bool,
    pub open_after_save: bool,
    /// Which generated card groups start on a new page (the reference app sub-options).
    pub cards_new_page: CardsNewPage,
    /// Reference app "Legacy Spellcasting Page": the single-page basic spellcasting layout.
    pub legacy_spellcasting_page: bool,
    /// Print a feature and the later features that improve it as one entry (default off).
    pub fold_feature_upgrades: bool,
    /// Paper size (TV1, B-147). `None` until the user picks one: the sheet then uses the device
    /// region's paper, which the UI sends with each request (`SheetOptionsDto::default_paper`).
    pub paper: Option<SheetPaperSetting>,
}

/// Paper size of the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SheetPaperSetting {
    /// US Letter (8.5 x 11 in).
    Letter,
    /// ISO A4 (210 x 297 mm).
    A4,
}

/// Sheet colour treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SheetThemeSetting {
    #[default]
    Colour,
    Ink,
}

/// Sheet edition override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SheetEditionSetting {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "2014")]
    Y2014,
    #[serde(rename = "2024")]
    Y2024,
}

/// Reference app's "Start ... Cards on a New Page" sub-options (spell cards have none).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CardsNewPage {
    pub item: bool,
    pub attack: bool,
    pub feature: bool,
}

impl Default for CardsNewPage {
    fn default() -> Self {
        Self {
            item: true,
            attack: false,
            feature: true,
        }
    }
}

/// Which card types the sheet generates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CardSettings {
    pub spell: bool,
    pub item: bool,
    pub attack: bool,
    pub feature: bool,
}

/// AI provider preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    /// The optional AI assistant is switched on (off by default; always off on mobile).
    pub enabled: bool,
    /// Preferred CLI: `claude`, `codex` or `none` (= pick whichever is installed).
    pub provider: String,
    pub paired_host: Option<String>,
    /// Explicit path to the preferred provider's executable (wins over `PATH` detection).
    pub override_path: Option<String>,
}

/// All user settings (camelCase on the wire, same shape as the UI's `AppSettings`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub player_name: String,
    pub default_ruleset: Ruleset,
    pub ability_method: AbilityMethodDto,
    pub point_buy_budget: i32,
    pub ability_min_base: i32,
    pub ability_max_base: i32,
    pub ability_cap_enabled: bool,
    pub selection_flow: bool,
    pub confirm_remove_level: bool,
    pub confirm_equip_weapon_not_in_attacks: bool,
    pub density: String,
    pub theme: String,
    pub high_contrast: bool,
    pub tile_size: u32,
    pub autosave: bool,
    pub sheet: SheetSettings,
    pub ai: AiSettings,
    /// Read-only platform flag for the UI: this is an iOS/Android build (no AI there). Always
    /// overwritten from the build target on load and patch, never taken from the file or a patch.
    pub mobile: bool,
    /// Read-only platform flag for the UI: this build comes from an app store (the iOS app, the
    /// Google Play build), so it never looks for new versions and hides the Releases link
    /// ([`crate::distribution`]). Pinned like `mobile`.
    pub store_build: bool,
    /// Read-only platform flag for the UI: this build has the AI CLI integration
    /// ([`crate::distribution::HAS_AI_CLI`]; not on phones or in the Mac App Store build). Without it the
    /// UI hides every AI entry and `ai.enabled` stays off. Pinned like `mobile`.
    pub ai_available: bool,
    /// Reference app DATA FILES: where characters are stored (`<dir>/characters`); `None` = app default.
    /// Changed only through `set_documents_dir` (a backend-picked folder token).
    pub documents_dir: Option<String>,
    /// Reference app DATA FILES: extra folder whose content files load as user packs; `None` = unlinked.
    /// Changed only through `set_secondary_content_dir` / `unlink_secondary_content_dir`.
    pub secondary_content_dir: Option<String>,
    /// Settings > CONTENT: a folder of the user's own images shown in the portraits gallery (GitHub #32; the
    /// reference app's `<documents>/portraits`). `None` = no library (the gallery shows only images added on this
    /// device). Changed only through `set_portrait_library_dir` (a backend-picked folder token).
    pub portrait_library_dir: Option<String>,
    /// Settings > CONTENT: the Obsidian vault folder the notes view opens in place (GitHub #15). `None` = no vault.
    /// Changed only through `set_vault_dir` (a backend-picked folder token).
    pub vault_dir: Option<String>,
    /// Reference app "Automatic Updates": check the user's indexes at startup (desktop only).
    pub auto_content_update: bool,
    /// Reference app selection-picker ROW HEIGHT: `small`, `medium` or `large`.
    pub row_height: String,
    /// Reference app "Skills Panel Expanded" (character information panel).
    pub skills_panel_expanded: bool,
    /// Reference app "Saving Throws Expanded" (character information panel).
    pub saving_throws_expanded: bool,
    /// Reference app accent swatch id (see `ACCENT_COLORS`).
    pub accent_color: String,
    /// Reference app CHARACTER COLLECTION PORTRAIT SIZE in px: 125, 150 or 175.
    pub portrait_size: u32,
    /// Show the "Import only content you own" notice before every content import (default on). Turned off by
    /// "Don't show this again"; turned back on in Settings > Data files.
    pub show_import_notice: bool,
    /// "Check for new versions of Borealis": at startup, ask GitHub for the latest version number (default on).
    pub check_app_updates: bool,
    /// Local search (local-search-spec.md section 9).
    pub search: SearchSettings,
}

/// Device tier override for semantic search (S3; keyword search ignores it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchTier {
    #[default]
    Auto,
    T1,
    T2,
    T3,
}

/// Edition the search surface opens with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SearchEdition {
    /// The open character's edition, both without a character.
    #[default]
    #[serde(rename = "character")]
    Character,
    #[serde(rename = "both")]
    Both,
    #[serde(rename = "2014")]
    Y2014,
    #[serde(rename = "2024")]
    Y2024,
}

/// `AppSettings.search`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchSettings {
    /// Semantic search is on (needs the consented model download, S3; off by default).
    pub semantic: bool,
    /// Device tier override.
    pub tier_override: SearchTier,
    /// Mobile: build semantic indexes on battery too (default off).
    pub index_on_battery: bool,
    /// Edition toggle default.
    pub default_edition: SearchEdition,
}

impl Default for CardSettings {
    fn default() -> Self {
        Self {
            spell: true,
            item: true,
            attack: true,
            feature: true,
        }
    }
}

impl Default for SheetSettings {
    fn default() -> Self {
        Self {
            form_fillable: false,
            theme: SheetThemeSetting::default(),
            edition: SheetEditionSetting::default(),
            style_formatting: true,
            swap_ability_positions: false,
            include_non_prepared_spells: false,
            cards: CardSettings::default(),
            auto_generate: true,
            open_after_save: true,
            cards_new_page: CardsNewPage::default(),
            legacy_spellcasting_page: false,
            fold_feature_upgrades: false,
            paper: None,
        }
    }
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "none".into(),
            paired_host: None,
            override_path: None,
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            player_name: "Player One".into(),
            default_ruleset: Ruleset::Dnd2024, // new characters default to 2024
            ability_method: AbilityMethodDto::Roll4d6,
            point_buy_budget: 27,
            ability_min_base: 3,
            ability_max_base: 30,
            ability_cap_enabled: true,
            selection_flow: false,
            confirm_remove_level: true,
            confirm_equip_weapon_not_in_attacks: true,
            density: "comfortable".into(),
            theme: "system".into(),
            high_contrast: false,
            tile_size: 160,
            autosave: true,
            sheet: SheetSettings::default(),
            ai: AiSettings::default(),
            mobile: IS_MOBILE,
            store_build: crate::distribution::is_store_build(),
            ai_available: crate::distribution::HAS_AI_CLI,
            documents_dir: None,
            secondary_content_dir: None,
            portrait_library_dir: None,
            vault_dir: None,
            auto_content_update: true,
            row_height: "medium".into(),
            skills_panel_expanded: true,
            saving_throws_expanded: false,
            accent_color: "default".into(),
            portrait_size: 150,
            show_import_notice: true,
            check_app_updates: true,
            search: SearchSettings::default(),
        }
    }
}

/// This build runs on a phone or tablet (no claude/codex CLI there, so no AI at all).
pub const IS_MOBILE: bool = cfg!(any(target_os = "android", target_os = "ios"));

const THEMES: [&str; 3] = ["system", "light", "dark"];
const DENSITIES: [&str; 2] = ["compact", "comfortable"];
const AI_PROVIDERS: [&str; 3] = ["claude", "codex", "none"];
const ROW_HEIGHTS: [&str; 3] = ["small", "medium", "large"];
/// Reference app's accent swatches, in dialog order (`default` = the reference default).
pub const ACCENT_COLORS: [&str; 10] = [
    "black", "brown", "default", "purple", "green", "aqua", "blue", "mauve", "pink", "red",
];
/// Portrait size slider range (the reference app snaps to 125 / 150 / 175).
const PORTRAIT_SIZES: std::ops::RangeInclusive<u32> = 125..=175;

impl AppSettings {
    /// Pins the platform-owned fields: `mobile` comes from the build target, `store_build` from the
    /// build's distribution, `ai_available` from the build, and the AI assistant is always off in a
    /// build without the AI CLI integration (phones, the Mac App Store).
    #[must_use]
    fn normalized(mut self) -> Self {
        self.mobile = IS_MOBILE;
        self.store_build = crate::distribution::is_store_build();
        self.ai_available = crate::distribution::HAS_AI_CLI;
        if !self.ai_available {
            self.ai.enabled = false;
        }
        self
    }

    /// Reject values the rules engine or the UI cannot work with.
    ///
    /// # Errors
    /// [`ApiError`] with code `invalid_argument`.
    pub fn validate(&self) -> ApiResult<()> {
        if !(1..=100).contains(&self.point_buy_budget) {
            return Err(ApiError::invalid("The point buy budget must be 1 to 100."));
        }
        if self.ability_min_base < 1
            || self.ability_max_base > 30
            || self.ability_min_base > self.ability_max_base
        {
            return Err(ApiError::invalid(
                "The base score range must lie within 1 to 30.",
            ));
        }
        if !THEMES.contains(&self.theme.as_str())
            || !DENSITIES.contains(&self.density.as_str())
            || !AI_PROVIDERS.contains(&self.ai.provider.as_str())
        {
            return Err(ApiError::invalid("Unknown theme, density or AI provider."));
        }
        if !(80..=400).contains(&self.tile_size) {
            return Err(ApiError::invalid("The tile size must be 80 to 400."));
        }
        if !ROW_HEIGHTS.contains(&self.row_height.as_str())
            || !ACCENT_COLORS.contains(&self.accent_color.as_str())
        {
            return Err(ApiError::invalid("Unknown row height or accent colour."));
        }
        if !PORTRAIT_SIZES.contains(&self.portrait_size) {
            return Err(ApiError::invalid("The portrait size must be 125 to 175."));
        }
        Ok(())
    }

    /// The engine settings these preferences imply.
    #[must_use]
    pub fn engine_settings(&self) -> EngineSettings {
        EngineSettings {
            use_default_ability_maximum: self.ability_cap_enabled,
            point_buy_budget: Some(self.point_buy_budget),
            min_base_score: self.ability_min_base,
            max_base_score: self.ability_max_base,
            ..EngineSettings::default()
        }
    }

    /// Apply a partial JSON patch (objects merge recursively) and validate the result.
    ///
    /// # Errors
    /// [`ApiError`] if the patch has unknown types or invalid values.
    pub fn patched(&self, patch: &Value) -> ApiResult<Self> {
        if !patch.is_object() {
            return Err(ApiError::invalid("The settings patch must be an object."));
        }
        self.check_override_patch(patch)?;
        self.check_dir_patch(patch)?;
        let mut current = serde_json::to_value(self)?;
        merge(&mut current, patch);
        let next: Self = serde_json::from_value(current)?;
        next.validate()?;
        Ok(next.normalized())
    }

    /// `ai.overridePath` may only be cleared (null) or resent unchanged through a patch; a new
    /// value comes from `set_ai_override` with a backend-picked, user-confirmed file.
    fn check_override_patch(&self, patch: &Value) -> ApiResult<()> {
        let Some(v) = patch.pointer("/ai/overridePath") else {
            return Ok(());
        };
        if v.is_null() || v.as_str() == self.ai.override_path.as_deref() {
            return Ok(());
        }
        Err(ApiError::invalid(
            "Choose the AI program with the Choose button; it cannot be set directly.",
        ))
    }

    /// `documentsDir` / `secondaryContentDir` / `portraitLibraryDir` / `vaultDir` may only be resent unchanged: new
    /// folders come from backend-picked tokens (`set_documents_dir`, `set_secondary_content_dir`,
    /// `set_portrait_library_dir`, `set_vault_dir`),
    /// never a raw path.
    fn check_dir_patch(&self, patch: &Value) -> ApiResult<()> {
        let fields = [
            ("documentsDir", self.documents_dir.as_deref()),
            ("secondaryContentDir", self.secondary_content_dir.as_deref()),
            ("portraitLibraryDir", self.portrait_library_dir.as_deref()),
            ("vaultDir", self.vault_dir.as_deref()),
        ];
        for (key, current) in fields {
            let Some(v) = patch.get(key) else { continue };
            if v.as_str() != current {
                return Err(ApiError::invalid(
                    "Choose folders with the folder button; they cannot be set directly.",
                ));
            }
        }
        Ok(())
    }

    /// Factory values for everything the reference app's settings dialog owns (Restore Defaults); the
    /// Borealis-only group (AI, default ruleset, autosave, the Obsidian vault) is kept, and so is every folder
    /// (documents, secondary content, portrait library): folders are user data (B-226, decision FU1).
    #[must_use]
    pub fn restored_defaults(&self) -> Self {
        Self {
            ai: self.ai.clone(),
            default_ruleset: self.default_ruleset,
            autosave: self.autosave,
            documents_dir: self.documents_dir.clone(),
            secondary_content_dir: self.secondary_content_dir.clone(),
            portrait_library_dir: self.portrait_library_dir.clone(),
            vault_dir: self.vault_dir.clone(),
            ..Self::default()
        }
    }

    /// Read settings from disk; a missing or unreadable file yields the defaults.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(s) if s.validate().is_ok() => s.normalized(),
            _ => {
                tracing::warn!("settings file is invalid; using defaults");
                Self::default()
            }
        }
    }

    /// Write settings atomically.
    ///
    /// # Errors
    /// [`ApiError`] with code `io_error`.
    pub fn save(&self, path: &Path) -> ApiResult<()> {
        let text = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| ApiError::io("save settings", e))?;
        borealis_character::format::replace_file(&tmp, path)
            .map_err(|e| ApiError::io("save settings", e))
    }
}

fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                match t.get_mut(k) {
                    Some(existing) if existing.is_object() && v.is_object() => merge(existing, v),
                    _ => {
                        t.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (t, p) => *t = p.clone(),
    }
}

#[cfg(test)]
mod default_tests {
    use super::*;

    #[test]
    fn defaults_match_the_settings_spec() {
        let s = AppSettings::default();
        assert_eq!(s.player_name, "Player One");
        assert_eq!(s.ability_method, AbilityMethodDto::Roll4d6);
        assert!(!s.selection_flow);
    }

    #[test]
    fn ai_is_off_by_default_and_for_files_without_the_field() {
        assert!(!AppSettings::default().ai.enabled);
        let dir = std::env::temp_dir().join(format!("borealis-ai-off-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        std::fs::write(&file, r#"{"playerName":"Old","ai":{"provider":"claude"}}"#).unwrap();
        let loaded = AppSettings::load(&file);
        assert_eq!(loaded.player_name, "Old");
        assert!(!loaded.ai.enabled);
        assert_eq!(loaded.ai.provider, "claude");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ai_enabled_round_trips_through_a_patch_and_mobile_cannot_be_patched() {
        let on = AppSettings::default()
            .patched(&serde_json::json!({"ai": {"enabled": true}, "mobile": !IS_MOBILE}))
            .unwrap();
        assert_eq!(on.ai.enabled, crate::distribution::HAS_AI_CLI);
        assert_eq!(on.mobile, IS_MOBILE);
    }

    #[test]
    fn ai_available_comes_from_the_build_and_cannot_be_patched_or_loaded() {
        let available = crate::distribution::HAS_AI_CLI;
        assert_eq!(AppSettings::default().ai_available, available);
        let patched = AppSettings::default()
            .patched(&serde_json::json!({"aiAvailable": !available, "ai": {"enabled": true}}))
            .unwrap();
        assert_eq!(patched.ai_available, available);
        assert_eq!(
            patched.ai.enabled, available,
            "AI cannot be switched on without the CLI integration"
        );
        let dir = std::env::temp_dir().join(format!("borealis-ai-avail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        std::fs::write(
            &file,
            format!(
                r#"{{"aiAvailable":{},"ai":{{"enabled":true}}}}"#,
                !available
            ),
        )
        .unwrap();
        let loaded = AppSettings::load(&file);
        assert_eq!(loaded.ai_available, available);
        assert_eq!(loaded.ai.enabled, available);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn store_build_comes_from_the_build_and_cannot_be_patched_or_loaded() {
        let store = crate::distribution::is_store_build();
        assert_eq!(AppSettings::default().store_build, store);
        let patched = AppSettings::default()
            .patched(&serde_json::json!({"storeBuild": !store}))
            .unwrap();
        assert_eq!(patched.store_build, store);
        let dir = std::env::temp_dir().join(format!("borealis-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        std::fs::write(&file, format!(r#"{{"storeBuild":{}}}"#, !store)).unwrap();
        assert_eq!(AppSettings::load(&file).store_build, store);
        std::fs::remove_dir_all(&dir).ok();
    }
}
