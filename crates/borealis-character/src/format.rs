//! Load and save with a versioned, migratable JSON format.

use crate::model::{Character, FORMAT, FORMAT_VERSION};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

/// Errors from reading or writing character files.
#[derive(Debug, Error)]
pub enum FormatError {
    /// File system problem.
    #[error("{path}: {source}")]
    Io {
        /// The path.
        path: String,
        /// Cause.
        source: std::io::Error,
    },
    /// Not valid JSON for the model.
    #[error("invalid character file: {0}")]
    Json(#[from] serde_json::Error),
    /// Wrong `format` tag.
    #[error("not a {FORMAT} file")]
    WrongFormat,
    /// Missing or invalid `format_version`.
    #[error("missing or invalid format_version")]
    BadVersion,
    /// Written by a newer application.
    #[error(
        "the file uses format version {found}, this application understands up to {supported}"
    )]
    TooNew {
        /// Version in the file.
        found: u32,
        /// Highest supported version.
        supported: u32,
    },
    /// A migration failed.
    #[error("migration from version {from} failed: {reason}")]
    Migration {
        /// Source version.
        from: u32,
        /// Why.
        reason: String,
    },
}

/// One upgrade step from version `n` to `n + 1`, operating on the raw JSON.
pub type Migration = fn(&mut Value) -> Result<(), String>;

/// Registered migrations: `MIGRATIONS[i]` upgrades version `i + 1` to `i + 2`.
pub const MIGRATIONS: &[Migration] = &[migrate_v1_to_v2];

const LEGACY_RUNTIME_KEY: &str = "#state";
const LEGACY_SPELLBOOK_SUFFIX: &str = "#spellbook";

/// v1 -> v2: move the magic `prepared` entries (`<Class>#spellbook`, `#state`) into the typed
/// `spellbooks` and `play` fields.
fn migrate_v1_to_v2(value: &mut Value) -> Result<(), String> {
    strip_legacy_keys(value);
    Ok(())
}

/// Move any leftover magic-key entries of `build.prepared` into typed fields (idempotent).
fn strip_legacy_keys(value: &mut Value) {
    let Some(build) = value.get_mut("build").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(Value::Array(prepared)) = build.remove("prepared") else {
        return;
    };
    let mut kept = Vec::new();
    let mut books = serde_json::Map::new();
    let mut play = serde_json::Map::new();
    for entry in prepared {
        let class = entry.get("class").and_then(Value::as_str).unwrap_or("");
        let spells = entry.get("spells").cloned().unwrap_or(Value::Array(vec![]));
        if class == LEGACY_RUNTIME_KEY {
            decode_play(&spells, &mut play);
        } else if let Some(name) = class.strip_suffix(LEGACY_SPELLBOOK_SUFFIX) {
            books.insert(name.to_string(), spells);
        } else {
            kept.push(entry);
        }
    }
    build.insert("prepared".into(), Value::Array(kept));
    if !books.is_empty() {
        let slot = build
            .entry("spellbooks")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Some(obj) = slot.as_object_mut() {
            for (k, v) in books {
                obj.entry(k).or_insert(v);
            }
        }
    }
    if !play.is_empty() {
        let slot = build
            .entry("play")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Some(obj) = slot.as_object_mut() {
            for (k, v) in play {
                obj.entry(k).or_insert(v);
            }
        }
    }
}

/// Decode the v1 short strings (`used:<id>=<n>`, `toggle:<id>`, `exhaustion=<n>`, `inspiration`).
fn decode_play(entries: &Value, play: &mut serde_json::Map<String, Value>) {
    let mut used = serde_json::Map::new();
    let mut toggles = Vec::new();
    for e in entries
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if let Some((id, n)) = e.strip_prefix("used:").and_then(|r| r.rsplit_once('=')) {
            if let Ok(n) = n.parse::<u32>() {
                used.insert(id.to_string(), Value::from(n));
            }
        } else if let Some(id) = e.strip_prefix("toggle:") {
            toggles.push(Value::from(id));
        } else if let Some(n) = e.strip_prefix("exhaustion=") {
            play.insert(
                "exhaustion".into(),
                Value::from(n.parse::<u32>().unwrap_or(0)),
            );
        } else if e == "inspiration" {
            play.insert("heroic_inspiration".into(), Value::from(true));
        }
    }
    if !used.is_empty() {
        play.insert("resources_used".into(), Value::Object(used));
    }
    if !toggles.is_empty() {
        play.insert("toggles".into(), Value::Array(toggles));
    }
}

/// Upgrade raw JSON to `target` using `steps` (exposed for tests).
///
/// # Errors
/// Format, version and migration errors.
pub fn upgrade_with(
    mut value: Value,
    target: u32,
    steps: &[Migration],
) -> Result<Value, FormatError> {
    if value.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(FormatError::WrongFormat);
    }
    let found = value
        .get("format_version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v >= 1)
        .ok_or(FormatError::BadVersion)?;
    if found > target {
        return Err(FormatError::TooNew {
            found,
            supported: target,
        });
    }
    for v in found..target {
        let step = steps
            .get((v - 1) as usize)
            .ok_or_else(|| FormatError::Migration {
                from: v,
                reason: "no migration registered".into(),
            })?;
        step(&mut value).map_err(|reason| FormatError::Migration { from: v, reason })?;
        value["format_version"] = Value::from(v + 1);
    }
    Ok(value)
}

impl Character {
    /// Parse a character from JSON text, applying migrations.
    ///
    /// # Errors
    /// [`FormatError`].
    pub fn from_json_str(text: &str) -> Result<Self, FormatError> {
        let raw: Value = serde_json::from_str(text)?;
        let mut upgraded = upgrade_with(raw, FORMAT_VERSION, MIGRATIONS)?;
        strip_legacy_keys(&mut upgraded);
        let mut character: Self = serde_json::from_value(upgraded)?;
        character.drop_unsafe_portrait();
        Ok(character)
    }

    /// Strip a portrait whose file extension is not an allowed image type or whose data is not
    /// clean base64, so a hostile file can never smuggle markup into exports.
    fn drop_unsafe_portrait(&mut self) {
        let Some(p) = &self.portrait else { return };
        let ext_ok = p
            .file_name
            .rsplit_once('.')
            .is_some_and(|(_, e)| PORTRAIT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
        if !ext_ok || !is_clean_base64(&p.data_base64) {
            tracing::warn!("dropping portrait with an unsupported type or invalid base64 data");
            self.portrait = None;
        }
    }

    /// Pretty JSON text.
    ///
    /// # Errors
    /// Serialization errors (not expected).
    pub fn to_json_string(&self) -> Result<String, FormatError> {
        let mut s = serde_json::to_string_pretty(self)?;
        s.push('\n');
        Ok(s)
    }

    /// Read a file.
    ///
    /// # Errors
    /// [`FormatError`].
    pub fn load(path: &Path) -> Result<Self, FormatError> {
        let text = std::fs::read_to_string(path).map_err(|source| FormatError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_json_str(&text)
    }

    /// Write a file atomically (temporary file in the same directory, then rename).
    ///
    /// # Errors
    /// [`FormatError`].
    pub fn save(&self, path: &Path) -> Result<(), FormatError> {
        let text = self.to_json_string()?;
        let io = |source| FormatError::Io {
            path: path.display().to_string(),
            source,
        };
        let tmp = unique_tmp_path(path);
        let written = (|| {
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            drop(file);
            replace_file(&tmp, path)
        })();
        if let Err(source) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(io(source));
        }
        Ok(())
    }
}

/// Attempts and pause for [`replace_file`] when the target is briefly held open.
const REPLACE_ATTEMPTS: u32 = 5;
const REPLACE_BACKOFF: std::time::Duration = std::time::Duration::from_millis(40);

/// Whether a failed rename is worth retrying: on Windows an antivirus scanner, the search
/// indexer or Explorer's preview pane can hold the target open for a moment, which shows up
/// as access denied (5), sharing violation (32) or lock violation (33).
fn is_transient_replace_error(e: &std::io::Error) -> bool {
    cfg!(windows) && matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

/// Run `op` up to `attempts` times while it fails with a transient error.
fn retry_transient<T>(
    attempts: u32,
    pause: std::time::Duration,
    is_transient: impl Fn(&std::io::Error) -> bool,
    mut op: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let mut left = attempts.max(1);
    loop {
        left -= 1;
        match op() {
            Err(e) if left > 0 && is_transient(&e) => std::thread::sleep(pause),
            other => return other,
        }
    }
}

/// Atomically replace `to` with `from` (same directory). `rename` replaces an existing file on
/// every platform; on Windows a target briefly opened by another process is retried.
///
/// # Errors
/// The last rename error.
pub fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    retry_transient(
        REPLACE_ATTEMPTS,
        REPLACE_BACKOFF,
        is_transient_replace_error,
        || std::fs::rename(from, to),
    )
}

/// Allowed portrait file extensions (lowercase).
const PORTRAIT_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A tmp path in the same directory as `path`, unique per process and call.
fn unique_tmp_path(path: &Path) -> PathBuf {
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()))
}

/// Whether `s` is standard base64 (whitespace ignored) with padding only at the end.
fn is_clean_base64(s: &str) -> bool {
    let mut pad = 0usize;
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' if pad == 0 => {}
            b'=' if pad < 2 => pad += 1,
            b' ' | b'\n' | b'\r' | b'\t' => {}
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::Ruleset;

    #[test]
    fn replace_file_overwrites_an_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("a.tmp"), dir.path().join("b.json"));
        std::fs::write(&from, "new").unwrap();
        std::fs::write(&to, "old").unwrap();
        replace_file(&from, &to).unwrap();
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "new");
        assert!(!from.exists());
    }

    #[test]
    fn transient_errors_are_retried_until_success_or_attempts_run_out() {
        let busy = || std::io::Error::from_raw_os_error(32);
        let mut calls = 0;
        let ok = retry_transient(
            5,
            std::time::Duration::ZERO,
            |_| true,
            || {
                calls += 1;
                if calls < 3 {
                    Err(busy())
                } else {
                    Ok(calls)
                }
            },
        );
        assert_eq!(ok.unwrap(), 3);
        let mut calls = 0;
        let err = retry_transient(
            4,
            std::time::Duration::ZERO,
            |_| true,
            || {
                calls += 1;
                Err::<(), _>(busy())
            },
        );
        assert!(err.is_err());
        assert_eq!(calls, 4);
    }

    #[test]
    fn permanent_errors_fail_at_once() {
        let mut calls = 0;
        let err = retry_transient(
            5,
            std::time::Duration::ZERO,
            |_| false,
            || {
                calls += 1;
                Err::<(), _>(std::io::Error::from_raw_os_error(2))
            },
        );
        assert!(err.is_err());
        assert_eq!(calls, 1);
    }

    #[test]
    fn only_windows_lock_errors_count_as_transient() {
        let busy = std::io::Error::from_raw_os_error(32);
        assert_eq!(is_transient_replace_error(&busy), cfg!(windows));
        let other = std::io::Error::other("x");
        assert!(!is_transient_replace_error(&other));
    }

    #[test]
    fn hostile_portrait_is_stripped_on_load() {
        let mut c = Character::new("Mira");
        for (name, data) in [
            ("p.png", "iVBORw0KGgo=\" onload=\"x"),
            ("p.png", "iVBORw0KGgo=\"><script>"),
            ("p.svg", "iVBORw0KGgo="),
            ("a/b", "iVBORw0KGgo="),
        ] {
            c.portrait = Some(crate::Portrait {
                file_name: name.into(),
                data_base64: data.into(),
            });
            let back = Character::from_json_str(&c.to_json_string().unwrap()).unwrap();
            assert!(back.portrait.is_none(), "{name} {data}");
        }
        c.portrait = Some(crate::Portrait {
            file_name: "p.png".into(),
            data_base64: "iVBORw0KGgo=".into(),
        });
        let back = Character::from_json_str(&c.to_json_string().unwrap()).unwrap();
        assert!(back.portrait.is_some());
    }

    #[test]
    fn save_leaves_no_tmp_files_and_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.json");
        let c = Character::new("Mira");
        c.save(&path).unwrap();
        c.save(&path).unwrap();
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1);
        assert_eq!(Character::load(&path).unwrap(), c);
    }

    #[test]
    fn round_trips_and_keeps_unknown_keys() {
        let mut c = Character::new("Mira");
        c.details.backstory = "Born under a lucky star.".into();
        c.extra
            .insert("future_field".into(), serde_json::json!({"a": 1}));
        let text = c.to_json_string().unwrap();
        assert!(text.contains("\"format\": \"borealis-character\""));
        assert!(text.contains("\"format_version\": 2"));
        let back = Character::from_json_str(&text).unwrap();
        assert_eq!(back, c);
        assert_eq!(back.extra["future_field"]["a"], 1);
    }

    #[test]
    fn allow_cross_edition_persists_and_is_optional_on_disk() {
        let mut c = Character::new("Mira");
        assert_eq!(c.build.allow_cross_edition, None);
        let text = c.to_json_string().unwrap();
        assert!(!text.contains("allow_cross_edition"), "None is not written");
        // a version-1 file written before the field existed loads as "profile default"
        assert_eq!(
            Character::from_json_str(&text)
                .unwrap()
                .build
                .allow_cross_edition,
            None
        );
        c.build.allow_cross_edition = Some(false);
        let text = c.to_json_string().unwrap();
        assert!(text.contains("\"allow_cross_edition\": false"));
        let back = Character::from_json_str(&text).unwrap();
        assert_eq!(back.build.allow_cross_edition, Some(false));
        assert_eq!(back, c);
    }

    fn v1_file() -> String {
        r##"{"format":"borealis-character","format_version":1,"ruleset":"dnd2024","build":{
        "ruleset":"dnd2024","levels":[{"kind":"main"}],"prepared":[
          {"class":"Wizard","spells":["ID_A"]},
          {"class":"Wizard#spellbook","spells":["ID_A","ID_B"]},
          {"class":"#state","spells":["used:rage=2","toggle:rage","exhaustion=3","inspiration"]}]}}"##
            .to_string()
    }

    #[test]
    fn migrates_v1_magic_keys_into_typed_fields() {
        let c = Character::from_json_str(&v1_file()).unwrap();
        assert_eq!(c.build.spellbook_for("Wizard").len(), 2);
        assert_eq!(c.build.spellbook_for("Wizard")[1].0, "ID_B");
        assert_eq!(c.build.play.resources_used["rage"], 2);
        assert!(c.build.play.toggles.contains("rage"));
        assert_eq!(c.build.play.exhaustion, 3);
        assert!(c.build.play.heroic_inspiration);
        assert_eq!(c.build.prepared.len(), 1);
        let text = c.to_json_string().unwrap();
        assert!(!text.contains("#spellbook") && !text.contains("#state"));
        assert!(text.contains("\"format_version\": 2"));
        assert_eq!(Character::from_json_str(&text).unwrap(), c);
    }

    #[test]
    fn load_strips_leftover_magic_keys_from_v2_files() {
        let text = v1_file().replace("\"format_version\":1", "\"format_version\":2");
        let c = Character::from_json_str(&text).unwrap();
        assert!(c.build.prepared.iter().all(|p| !p.class.contains('#')));
        assert_eq!(c.build.spellbook_for("Wizard").len(), 2);
        assert_eq!(c.build.play.exhaustion, 3);
    }

    #[test]
    fn rejects_foreign_and_newer_files() {
        assert!(matches!(
            Character::from_json_str(r#"{"format":"x","format_version":1}"#),
            Err(FormatError::WrongFormat)
        ));
        assert!(matches!(
            Character::from_json_str(r#"{"format":"borealis-character"}"#),
            Err(FormatError::BadVersion)
        ));
        assert!(matches!(
            Character::from_json_str(r#"{"format":"borealis-character","format_version":99}"#),
            Err(FormatError::TooNew {
                found: 99,
                supported: FORMAT_VERSION
            })
        ));
        assert!(Character::from_json_str("not json").is_err());
    }

    #[test]
    fn migration_chain_runs_in_order() {
        fn v1_to_v2(v: &mut Value) -> Result<(), String> {
            v["step"] = Value::from("a");
            Ok(())
        }
        fn v2_to_v3(v: &mut Value) -> Result<(), String> {
            let prev = v["step"].as_str().unwrap_or("").to_string();
            v["step"] = Value::from(format!("{prev}b"));
            Ok(())
        }
        let raw = serde_json::json!({"format": FORMAT, "format_version": 1});
        let out = upgrade_with(raw, 3, &[v1_to_v2, v2_to_v3]).unwrap();
        assert_eq!(out["step"], "ab");
        assert_eq!(out["format_version"], 3);
        fn boom(_: &mut Value) -> Result<(), String> {
            Err("nope".into())
        }
        let raw = serde_json::json!({"format": FORMAT, "format_version": 1});
        assert!(matches!(
            upgrade_with(raw.clone(), 2, &[boom]),
            Err(FormatError::Migration { from: 1, .. })
        ));
        assert!(matches!(
            upgrade_with(raw, 2, &[]),
            Err(FormatError::Migration { .. })
        ));
    }

    #[test]
    fn save_and_load_through_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mira.json");
        let c = Character::with_ruleset("Mira", Ruleset::Dnd2024);
        c.save(&path).unwrap();
        assert!(!dir.path().join("mira.json.tmp").exists());
        let back = Character::load(&path).unwrap();
        assert_eq!(back.ruleset(), Ruleset::Dnd2024);
        assert!(Character::load(&dir.path().join("missing.json")).is_err());
    }
}
