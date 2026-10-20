//! Synthetic content over the core pack for the proxy-item tests (`specs/proxy-items.md`).

#![allow(dead_code)] // each test target uses part of it

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands;
use borealis_engine::state::CharacterState;
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

pub const BOOK: &str = "Test Book";

pub fn elements() -> Vec<Value> {
    vec![
        json!({"id":"ID_TB_SOURCE","name":BOOK,"type":"Source","source":"Core",
               "setters":[{"name":"abbreviation","value":"TB"}]}),
        json!({"id":"ID_TB_FEAT_LOOKOUT","name":"Lookout","type":"Feat","source":BOOK,
               "description":"<p>Lookout feat text.</p>",
               "setters":[{"name":"keywords","value":"watch, sentry"}]}),
        // same kept part and source: the same proxy id, the first element wins
        json!({"id":"ID_TB_X_FEAT_LOOKOUT","name":"Lookout Again","type":"Feat","source":BOOK}),
        // no marker: no proxy
        json!({"id":"ID_TB_TALENT_ODD","name":"Odd","type":"Feat","source":BOOK}),
        json!({"id":"ID_TB_LANGUAGE_OLDTONGUE","name":"Old Tongue","type":"Language","source":BOOK}),
        json!({"id":"ID_TB_PROFICIENCY_SKILL_DELVING","name":"Delving","type":"Proficiency",
               "source":BOOK,"supports":["Skill"]}),
        json!({"id":"ID_TB_SPELL_SPARK","name":"Spark","type":"Spell","source":BOOK,
               "supports":["Tester Mage"],
               "setters":[{"name":"level","value":"0"},{"name":"school","value":"Evocation"}]}),
        json!({"id":"ID_TB_CLASS_FEATURE_TESTER_CASTING","name":"Tester Casting","type":"Class Feature",
               "source":BOOK,
               "spellcasting":{"name":"Tester Mage","ability":"Intelligence","list":{"supports":"Tester Mage"}}}),
        json!({"id":"ID_TB_ITEM_VEIL","name":"Veil","type":"Item","source":BOOK,
               "setters":[{"name":"inventory-hidden","value":"true"},{"name":"slot","value":"head"}]}),
        json!({"id":"ID_TB_ITEM_ROPE","name":"Rope","type":"Item","source":BOOK}),
    ]
}

pub fn engine() -> &'static Engine {
    static ENGINE: std::sync::OnceLock<Engine> = std::sync::OnceLock::new();
    ENGINE.get_or_init(|| {
        let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
        let file: ContentFile = serde_json::from_value(
            json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": elements()}),
        )
        .expect("synthetic content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&core)
            .expect("core pack")
            .add_file("synthetic.json", PackOrigin::Core, file)
            .build();
        Engine::new(&store)
    })
}

pub fn hero() -> CharacterState {
    commands::new_character(engine(), Ruleset::Dnd2014)
}
