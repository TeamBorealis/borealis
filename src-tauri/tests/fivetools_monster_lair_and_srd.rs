//! Through the real app state, on invented data only: a 5etools creature takes the lair actions
//! and regional effects of its legendary group (`bestiary/legendarygroups.json`), and a Monster
//! Manual creature whose name is an SRD creature's is linked to the bundled SRD stat block of its
//! edition (`maps_to` kind `equivalent`), so the engine sees one creature, not a duplicate.

use borealis_content::{MapKind, MonsterData};
use borealis_lib::content_dto::FiveToolsImportRequest;
use borealis_lib::content_packs::NullSink;
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const MM_WOLF: &str = "ID_5ET_MM_MONSTER_WOLF";
const MM_WYRM: &str = "ID_5ET_MM_MONSTER_PROBE_MOSS_WYRM";
const SRD51_WOLF: &str = "ID_SRD51_MONSTER_WOLF";

fn app(data: &Path) -> AppState {
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    };
    AppState::new(paths).expect("state")
}

/// An invented stat block (not the printed one) under `name`.
fn creature(name: &str, extra: &Value) -> Value {
    let mut c = json!({"name": name, "source": "MM", "size": ["M"], "type": "beast",
        "alignment": ["U"], "ac": [12], "hp": {"average": 9, "formula": "2d8"},
        "speed": {"walk": 40}, "str": 12, "dex": 14, "con": 11, "int": 3, "wis": 12,
        "cha": 6, "passive": 11, "cr": "1/4",
        "action": [{"name": "Nip", "entries": ["A probe action."]}]});
    for (k, v) in extra.as_object().expect("object") {
        c[k] = v.clone();
    }
    c
}

fn folder(dir: &Path) {
    std::fs::create_dir_all(dir.join("bestiary")).unwrap();
    let monsters = json!({"monster": [
        creature("Wolf", &json!({})),
        creature("Probe Moss Wyrm", &json!({"type": "dragon",
            "legendaryGroup": {"name": "Probe Moss Wyrm", "source": "MM"}})),
    ]});
    std::fs::write(dir.join("bestiary/bestiary-mm.json"), monsters.to_string()).unwrap();
    let groups = json!({"legendaryGroup": [{"name": "Probe Moss Wyrm", "source": "MM",
        "lairActions": ["The probe wyrm picks one option.",
            {"type": "list", "items": ["Moss spreads.", "Spores drift."]}],
        "regionalEffects": ["The land changes.", {"type": "list", "items": ["Paths close."]},
            "The effects fade."]}]});
    std::fs::write(
        dir.join("bestiary/legendarygroups.json"),
        groups.to_string(),
    )
    .unwrap();
}

#[test]
fn lair_sections_and_srd_links_survive_the_app_import() {
    let data = tempfile::tempdir().unwrap();
    let five = tempfile::tempdir().unwrap();
    folder(five.path());
    let app = app(data.path());
    let token = app
        .picked
        .grant(five.path().to_path_buf(), PickPurpose::FivetoolsFolder)
        .expect("grant")
        .token;
    let request = FiveToolsImportRequest {
        folder: token,
        sources: vec!["MM".into()],
        include_fluff: false,
    };
    app.import_5etools(&request, &NullSink).expect("import");
    let hub = app.hub().expect("hub");

    let wyrm = MonsterData::from_element(hub.element(MM_WYRM).expect("wyrm")).unwrap();
    let lair = wyrm.lair_actions.expect("lair actions");
    assert_eq!(lair.preamble, "The probe wyrm picks one option.");
    assert_eq!(lair.options.len(), 2);
    let regional = wyrm.regional_effects.expect("regional effects");
    assert_eq!(
        (regional.options.len(), regional.closing.as_str()),
        (1, "The effects fade.")
    );

    let wolf = hub.element(MM_WOLF).expect("wolf");
    let maps = wolf.maps_to.as_ref().expect("maps_to");
    assert_eq!(
        (maps.dnd2014.as_deref(), maps.kind),
        (Some(SRD51_WOLF), Some(MapKind::Equivalent))
    );
    assert!(hub.element(MM_WYRM).unwrap().maps_to.is_none());
    let uni = hub.engine.universe(borealis_content::Ruleset::Dnd2014);
    assert_eq!(uni.equivalent_ids(MM_WOLF), [SRD51_WOLF]);
}
