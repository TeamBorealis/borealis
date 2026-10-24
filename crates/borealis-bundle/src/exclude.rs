//! The exclusion list (device-sync-investigation sec. 1), enforced in code on both the writer and the
//! reader, plus the allow-list of portable settings (Q4).

use serde_json::{Map, Value};

/// Directory names (any depth, case-insensitive) that never travel: search indexes and extracted
/// text, embedding models, import drafts, the AI profile, sync state/trash, logs, and the device
/// link state (`link/`: paired-device registry; borealis-link `DATA_PATHS`).
const EXCLUDED_DIRS: &[&str] = &[
    "search",
    "models",
    "import-drafts",
    "ai-profile",
    "sync",
    "logs",
    "extracted",
    "link",
];

/// File names (case-insensitive) that never travel: device identity and secrets, device-local
/// settings, derived state.
const EXCLUDED_FILES: &[&str] = &[
    "ai-host-cert.der",
    "ai-host-key.der",
    "ai-host.json",
    "ai-setup.json",
    "settings.json",
    "content-prefs.json",
    "state.json",
];

/// Extensions (case-insensitive) that look like key material.
const EXCLUDED_EXTENSIONS: &[&str] = &["der", "pem", "key", "p12", "pfx", "pk8", "keystore"];

/// Settings keys that may travel (camelCase, as in `settings.json`). Everything else is device-local
/// (`documentsDir`, `secondaryContentDir`, `mobile`, density, tile and row size, panel states, `ai`)
/// and is dropped.
pub const PORTABLE_SETTINGS_KEYS: &[&str] = &[
    "playerName",
    "defaultRuleset",
    "abilityMethod",
    "pointBuyBudget",
    "abilityMinBase",
    "abilityMaxBase",
    "abilityCapEnabled",
    "confirmRemoveLevel",
    "confirmEquipWeaponNotInAttacks",
    "sheet",
    "accentColor",
];

/// True when a relative path (`/` separated) must never be bundled.
#[must_use]
pub fn is_excluded(rel_path: &str) -> bool {
    let segments: Vec<String> = rel_path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect();
    let Some(file) = segments.last() else {
        return false;
    };
    if segments.iter().any(|s| {
        EXCLUDED_DIRS.contains(&s.as_str()) || s.starts_with(".tmp-") || s.starts_with("ai-")
    }) {
        return true;
    }
    if EXCLUDED_FILES.contains(&file.as_str()) {
        return true;
    }
    file.rsplit_once('.')
        .is_some_and(|(_, ext)| EXCLUDED_EXTENSIONS.contains(&ext))
}

/// Keep only the portable subset of a full `settings.json` value. Non-objects yield an empty object.
#[must_use]
pub fn portable_settings(full: &Value) -> Value {
    let mut out = Map::new();
    if let Some(obj) = full.as_object() {
        for key in PORTABLE_SETTINGS_KEYS {
            if let Some(v) = obj.get(*key) {
                out.insert((*key).to_owned(), v.clone());
            }
        }
    }
    Value::Object(out)
}

/// True when `settings` is an object whose top-level keys are all on the allow-list.
#[must_use]
pub fn is_portable_only(settings: &Value) -> bool {
    settings.as_object().is_some_and(|o| {
        o.keys()
            .all(|k| PORTABLE_SETTINGS_KEYS.contains(&k.as_str()))
    })
}

/// True for a Windows device name (`CON`, `NUL`, `COM1`, `LPT9`, ... with or without an
/// extension, any case): writing `nul.xml` or a folder `CON` on Windows reaches a device, not a
/// file, so such names never travel.
#[must_use]
pub fn is_windows_reserved(segment: &str) -> bool {
    let stem = segment
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let mut chars = stem.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    let rest: Vec<char> = chars.collect();
    matches!(prefix.as_str(), "COM" | "LPT")
        && matches!(rest.as_slice(), [d] if d.is_ascii_digit() || matches!(d, '¹' | '²' | '³'))
}

/// A safe single path segment: non-empty, no separators or control characters, no leading dot,
/// not a Windows device name.
#[must_use]
pub fn is_safe_segment(s: &str) -> bool {
    const MAX_SEGMENT: usize = 128;
    !s.is_empty()
        && s.len() <= MAX_SEGMENT
        && !s.starts_with('.')
        && !s.ends_with(' ')
        && !s.ends_with('.')
        && s.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | ' ' | '(' | ')'))
        && !is_windows_reserved(s)
}

/// A safe `/`-separated relative path of at most `max_depth` segments.
#[must_use]
pub fn is_safe_rel_path(p: &str, max_depth: usize) -> bool {
    let parts: Vec<&str> = p.split('/').collect();
    parts.len() <= max_depth && parts.iter().all(|s| is_safe_segment(s))
}
