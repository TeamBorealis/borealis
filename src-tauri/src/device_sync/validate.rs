//! What a received item must pass before it is written (B3): characters through the real
//! character parser (migrations, `drop_unsafe_portrait`), packs through the content loader the app
//! itself uses, and ids that match the item key.

use borealis_bundle::ItemValidator;
use borealis_character::Character;
use borealis_content::ContentStoreBuilder;
use std::path::Path;

use crate::content_import::{is_reserved_pack_id, slug};

/// The app's receive-side validator.
#[derive(Debug, Clone, Copy, Default)]
pub struct AppValidator;

impl ItemValidator for AppValidator {
    fn character(&self, key: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "not UTF-8 text".to_string())?;
        let raw: serde_json::Value =
            serde_json::from_str(text).map_err(|_| "not valid JSON".to_string())?;
        let parsed =
            Character::from_json_str(text).map_err(|e| format!("not a Borealis character: {e}"))?;
        if !parsed.meta.id.is_empty() && parsed.meta.id != key {
            return Err("its id does not match the file name".into());
        }
        let portrait_dropped =
            raw.get("portrait").is_some_and(|p| !p.is_null()) && parsed.portrait.is_none();
        if portrait_dropped {
            // Write the sanitised character, never the unsafe portrait.
            return parsed
                .to_json_string()
                .map(String::into_bytes)
                .map_err(|e| e.to_string());
        }
        Ok(bytes.to_vec())
    }

    fn pack(&self, key: &str, dir: &Path) -> Result<(), String> {
        let builder = ContentStoreBuilder::new().add_pack_dir(dir).map_err(|e| {
            format!(
                "not a loadable content pack: {}",
                redact(&e.to_string(), dir)
            )
        })?;
        let store = builder.build();
        let Some(info) = store.packs().first() else {
            return Err("no pack manifest".into());
        };
        if is_reserved_pack_id(&info.id) {
            return Err("a bundled pack cannot be replaced".into());
        }
        if info.id != key && slug(&info.id) != key {
            return Err("its id does not match the folder name".into());
        }
        Ok(())
    }
}

/// Error texts from the loader carry the staging path; keep only the part inside the pack.
fn redact(message: &str, dir: &Path) -> String {
    let prefix = dir.display().to_string();
    message.replace(&prefix, "<pack>")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "c_0123456789abcdef0123456789abcdef";

    fn character(extra: &str) -> String {
        let mut c = Character::new("Mira");
        c.meta.id = ID.into();
        let mut v: serde_json::Value = serde_json::from_str(&c.to_json_string().unwrap()).unwrap();
        if !extra.is_empty() {
            let more: serde_json::Value = serde_json::from_str(extra).unwrap();
            v["portrait"] = more;
        }
        serde_json::to_string(&v).unwrap()
    }

    #[test]
    fn a_valid_character_passes_unchanged() {
        let text = character("");
        let out = AppValidator.character(ID, text.as_bytes());
        assert_eq!(out.as_deref(), Ok(text.as_bytes()), "{out:?}");
    }

    #[test]
    fn invalid_character_json_is_refused() {
        for bad in [
            "not json",
            r#"{"format":"something-else"}"#,
            r#"{"format":"borealis-character","format_version":99}"#,
            "[1,2,3]",
        ] {
            assert!(AppValidator.character(ID, bad.as_bytes()).is_err(), "{bad}");
        }
        assert!(AppValidator.character(ID, &[0xff, 0xfe]).is_err());
    }

    #[test]
    fn an_id_that_does_not_match_the_key_is_refused() {
        let text = character("");
        assert!(AppValidator
            .character("c_ffffffffffffffffffffffffffffffff", text.as_bytes())
            .is_err());
    }

    #[test]
    fn an_unsafe_portrait_is_dropped_before_writing() {
        let text = character(r#"{"file_name":"x.svg","data_base64":"PHN2Zz4="}"#);
        let out = AppValidator.character(ID, text.as_bytes()).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert!(v.get("portrait").is_none_or(serde_json::Value::is_null));
    }

    #[test]
    fn a_pack_must_load_and_match_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let pack = dir.path().join("homebrew");
        std::fs::create_dir_all(&pack).unwrap();
        assert!(
            AppValidator.pack("homebrew", &pack).is_err(),
            "no pack.json"
        );
        std::fs::write(
            pack.join("pack.json"),
            r#"{"format":"borealis-pack/1","id":"homebrew","name":"Homebrew","version":"1.0.0","origin":"custom","rulesets":["dnd2024"],"files":[]}"#,
        )
        .unwrap();
        let ok = AppValidator.pack("homebrew", &pack);
        assert_eq!(ok, Ok(()), "{ok:?}");
        assert!(AppValidator.pack("other", &pack).is_err());
    }
}
