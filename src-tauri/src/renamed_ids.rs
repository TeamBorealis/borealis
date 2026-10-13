//! Characters keep their picks when an imported element gets a new id.
//!
//! 5etools imports made before 2026-10 gave the second version of a subclass printed for two
//! classes of the same name a hash-suffixed id (`ID_5ET_XGE_SUBCLASS_BARBARIAN_ZEALOT_F76807ED`).
//! Re-imported packs name those elements readably and list the old id as `renamed_from`; the
//! content store resolves it as an alias, and this module rewrites a character's stored ids
//! to the new ones when it is opened, so the engine (which looks ids up exactly) finds them.

use borealis_content::ContentStore;
use borealis_engine::CharacterState;
use serde_json::Value;

/// Only ids of imported 5etools packs are rewritten: bundled `renamed_from` links between
/// editions must never silently convert a character.
const PREFIX: &str = "ID_5ET_";

/// Rewrite every renamed 5etools id in `build`. Returns how many values changed.
#[must_use]
pub fn migrate(build: &mut CharacterState, store: &ContentStore) -> usize {
    let Ok(mut value) = serde_json::to_value(&*build) else {
        return 0;
    };
    let changed = rewrite(&mut value, store);
    if changed == 0 {
        return 0;
    }
    match serde_json::from_value::<CharacterState>(value) {
        Ok(next) => {
            *build = next;
            changed
        }
        Err(e) => {
            tracing::warn!("renamed id migration skipped: {e}");
            0
        }
    }
}

fn renamed(id: &str, store: &ContentStore) -> Option<String> {
    if !id.starts_with(PREFIX) {
        return None;
    }
    let current = &store.get(id)?.element.id;
    (current != id && current.starts_with(PREFIX)).then(|| current.clone())
}

fn rewrite(value: &mut Value, store: &ContentStore) -> usize {
    match value {
        Value::String(s) => match renamed(s, store) {
            Some(next) => {
                *s = next;
                1
            }
            None => 0,
        },
        Value::Array(items) => items.iter_mut().map(|v| rewrite(v, store)).sum(),
        Value::Object(map) => map.values_mut().map(|v| rewrite(v, store)).sum(),
        _ => 0,
    }
}
