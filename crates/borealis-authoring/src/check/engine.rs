//! `load_into_engine`: the element must load into a content store with no dangling references,
//! and a test character must be able to take it.

use borealis_content::file::{ContentFile, PackOrigin};
use borealis_content::issue::IssueCode;
use borealis_content::{ContentStoreBuilder, Ruleset, Severity as ContentSeverity};
use borealis_engine::commands::{self, slot_key, Improvement};
use borealis_engine::{CharacterState, Choice, Engine, Severity as EngineSeverity};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Test class used for feat checks (2014 SRD fighter).
const TEST_CLASS_ID: &str = "ID_WOTC_PHB_CLASS_FIGHTER";
const TEST_CLASS_NAME: &str = "Fighter";
/// Fighter level with an ability score improvement slot.
const FEAT_LEVEL: u32 = 4;
/// Scores that satisfy typical feat prerequisites.
const TEST_SCORES: [i32; 6] = [15; 6];

/// What happened when a test character tried to take the element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", content = "detail", rename_all = "snake_case")]
pub enum CharacterCheck {
    /// The character took the element and the engine derived a sheet.
    Taken,
    /// The element has no character-level hook; only a plain derive was run and succeeded.
    DeriveOnly,
    /// The engine refused (reason from the engine).
    Refused(String),
    /// Not exercised (reason).
    Skipped(String),
}

/// Outcome of loading an element into the engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCheck {
    /// The document parsed as a content file.
    pub loaded: bool,
    /// Messages for references to ids that do not exist.
    pub dangling_refs: Vec<String>,
    /// Other store validation errors on the new elements.
    pub store_errors: Vec<String>,
    /// Test-character outcome per element id.
    pub characters: Vec<(String, CharacterCheck)>,
}

impl EngineCheck {
    /// True when it loaded, has no dangling refs or errors, and nothing was refused.
    pub fn passes(&self) -> bool {
        self.loaded
            && self.dangling_refs.is_empty()
            && self.store_errors.is_empty()
            && !self
                .characters
                .iter()
                .any(|(_, c)| matches!(c, CharacterCheck::Refused(_)))
    }

    fn failed(msg: String) -> Self {
        Self {
            loaded: false,
            dangling_refs: Vec::new(),
            store_errors: vec![msg],
            characters: Vec::new(),
        }
    }
}

/// Load `content` on top of the packs in `base`, validate, and try a test character.
pub fn load_into_engine(
    base: &ContentStoreBuilder,
    ruleset: Ruleset,
    content: &Value,
) -> EngineCheck {
    let file = match ContentFile::from_json_str(&content.to_string()) {
        Ok(f) => f,
        Err(e) => return EngineCheck::failed(format!("content does not parse: {e}")),
    };
    let new_ids: Vec<(String, String)> = file
        .elements
        .iter()
        .map(|e| (e.id.clone(), e.ty.clone()))
        .collect();
    let store = base
        .clone()
        .add_file("homebrew", PackOrigin::Custom, file)
        .build();
    let mut dangling_refs = Vec::new();
    let mut store_errors = Vec::new();
    for issue in store.validate() {
        let Some(id) = issue.element.as_deref() else {
            continue;
        };
        if !new_ids.iter().any(|(n, _)| n == id) {
            continue;
        }
        let msg = format!("{id}: {}", issue.message);
        if issue.code == IssueCode::DanglingReference {
            dangling_refs.push(msg);
        } else if issue.severity == ContentSeverity::Error {
            store_errors.push(msg);
        }
    }
    let engine = Engine::new(&store);
    let characters = new_ids
        .into_iter()
        .map(|(id, ty)| {
            let c = try_character(&engine, ruleset, &id, &ty);
            (id, c)
        })
        .collect();
    EngineCheck {
        loaded: true,
        dangling_refs,
        store_errors,
        characters,
    }
}

fn try_character(engine: &Engine, ruleset: Ruleset, id: &str, ty: &str) -> CharacterCheck {
    let base = commands::new_character(engine, ruleset);
    match ty {
        "Race" | "Species" => take_species(engine, base, id),
        "Feat" => take_feat(engine, ruleset, id),
        "Magic Item" | "Item" | "Weapon" | "Armor" => {
            match commands::add_item(engine, &base, id, 1) {
                Ok(o) => derived(engine, &o.state, id),
                Err(e) => CharacterCheck::Refused(e.to_string()),
            }
        }
        _ => match derived(engine, &base, id) {
            CharacterCheck::Taken => CharacterCheck::DeriveOnly,
            other => other,
        },
    }
}

fn take_species(engine: &Engine, base: CharacterState, id: &str) -> CharacterCheck {
    let snap = engine.derive(&base);
    let Some(key) = slot_key(&snap, "ID_LEVEL_1", "Race", 1) else {
        return CharacterCheck::Skipped("no race slot in this ruleset".into());
    };
    match commands::select(engine, &base, &key, Choice::Element(id.to_string())) {
        Ok(o) => derived(engine, &o.state, id),
        Err(e) => CharacterCheck::Refused(e.to_string()),
    }
}

fn take_feat(engine: &Engine, ruleset: Ruleset, id: &str) -> CharacterCheck {
    let mut s = commands::new_character(engine, ruleset);
    let snap = engine.derive(&s);
    let Some(key) = slot_key(&snap, "ID_LEVEL_1", "Class", 1) else {
        return CharacterCheck::Skipped("no class slot".into());
    };
    let steps = (|| {
        s = commands::select(engine, &s, &key, Choice::Element(TEST_CLASS_ID.into()))?.state;
        s = commands::set_base_scores(engine, &s, TEST_SCORES)?.state;
        for _ in 1..FEAT_LEVEL {
            s = commands::level_up_main(engine, &s, None)?.state;
        }
        commands::choose_improvement(
            engine,
            &s,
            TEST_CLASS_NAME,
            FEAT_LEVEL,
            &Improvement::Feat(id.to_string()),
        )
    })();
    match steps {
        Ok(o) => derived(engine, &o.state, id),
        Err(e) => CharacterCheck::Refused(e.to_string()),
    }
}

/// Derive the sheet; engine errors that name the element make the check fail.
fn derived(engine: &Engine, state: &CharacterState, id: &str) -> CharacterCheck {
    let errors: Vec<String> = engine
        .derive(state)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == EngineSeverity::Error && d.element.as_deref() == Some(id))
        .map(|d| d.message)
        .collect();
    if errors.is_empty() {
        CharacterCheck::Taken
    } else {
        CharacterCheck::Refused(errors.join("; "))
    }
}
