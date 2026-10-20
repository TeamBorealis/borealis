//! Lair actions and regional effects from legendary groups. SYNTHETIC data only.

use super::*;
use crate::convert::monster::{stat_block, stat_block_in, SECTION};
use borealis_content::Ruleset;
use serde_json::json;

/// A synthetic legendary group in the 5etools shape.
fn moss_group() -> Value {
    json!({
        "name": "Moss Wyrm", "source": "ZZZ",
        "lairActions": [
            "On initiative count 20 the wyrm picks one option.",
            {"type": "list", "items": [
                "Moss covers {@dice 1d4} squares.",
                {"type": "item", "name": "Spore Cloud.", "entry": "Spores fill a cube ({@dc 12})."}
            ]}
        ],
        "regionalEffects": [
            "The land around the lair changes.",
            {"type": "list", "style": "list-hang-notitle", "items": [
                {"type": "item", "name": "Overgrowth", "entries": ["Paths overgrow within a day."]}
            ]},
            "The effects fade if the wyrm dies."
        ]
    })
}

fn creature(group: &str) -> Value {
    json!({
        "name": "Moss Wyrm", "source": "ZZZ",
        "size": ["H"], "type": "dragon", "alignment": ["N"],
        "ac": [15], "hp": {"average": 52, "formula": "7d12 + 7"},
        "speed": {"walk": 30},
        "str": 18, "dex": 10, "con": 13, "int": 8, "wis": 11, "cha": 9,
        "passive": 10, "cr": "4",
        "action": [{"name": "Bite", "entries": ["It bites."]}],
        "legendaryGroup": {"name": group, "source": "zzz"}
    })
}

fn groups(list: &[Value], templates: &[Value]) -> (LegendaryGroups, Vec<String>) {
    let mut notes = Vec::new();
    let g = LegendaryGroups::new(list, templates, &mut notes);
    (g, notes)
}

#[test]
fn a_creature_takes_the_lair_of_its_legendary_group() {
    let (g, notes) = groups(&[moss_group()], &[]);
    assert!(notes.is_empty(), "{notes:?}");
    let (m, lost) = stat_block_in(&creature("moss wyrm"), Ruleset::Dnd2014, &g).unwrap();
    assert!(lost.is_empty(), "{lost:?}");
    let lair = m.lair_actions.as_ref().expect("lair actions");
    assert_eq!(
        lair.preamble,
        "On initiative count 20 the wyrm picks one option."
    );
    assert_eq!(
        lair.options,
        vec![
            LairOption {
                name: None,
                text: "Moss covers 1d4 squares.".into()
            },
            LairOption {
                name: Some("Spore Cloud".into()),
                text: "Spores fill a cube (DC 12).".into()
            },
        ]
    );
    assert!(lair.closing.is_empty());
    let regional = m.regional_effects.as_ref().expect("regional effects");
    assert_eq!(regional.options[0].name.as_deref(), Some("Overgrowth"));
    assert_eq!(regional.closing, "The effects fade if the wyrm dies.");
    assert!(m.validate().is_empty(), "{:?}", m.validate());
}

#[test]
fn a_missing_group_and_a_mythic_encounter_stay_partial() {
    let (m, lost) = stat_block(&creature("Moss Wyrm"), Ruleset::Dnd2014).unwrap();
    assert!(m.lair_actions.is_none() && m.regional_effects.is_none());
    assert_eq!(
        lost,
        [format!(
            "{SECTION}lair actions and regional effects (legendary group) not imported"
        )]
    );
    let mut mythic = moss_group();
    mythic["mythicEncounter"] = json!(["A mythic encounter."]);
    let (g, _) = groups(&[mythic], &[]);
    let (m, lost) = stat_block_in(&creature("Moss Wyrm"), Ruleset::Dnd2014, &g).unwrap();
    assert!(m.lair_actions.is_some());
    assert_eq!(
        lost,
        [format!(
            "{SECTION}the mythic encounter (legendary group) not imported"
        )]
    );
}

#[test]
fn a_group_copy_and_a_templated_version_resolve() {
    let copy = json!({"name": "Elder Moss Wyrm", "source": "ZZZ",
        "_copy": {"name": "Moss Wyrm", "source": "ZZZ",
            "_mod": {"lairActions": {"mode": "appendArr", "items": "Elder wyrms add a tremor."}}}});
    let mut base = moss_group();
    base["_versions"] = json!([{"name": "Moss Wyrm (Ashen)", "source": "ZZZ",
        "_templates": [{"name": "Ashen", "source": "ZZZ"}]}]);
    let template = json!({"name": "Ashen", "source": "ZZZ",
        "apply": {"_mod": {"regionalEffects": {"mode": "appendArr", "items": "Ash falls."}}}});
    let (g, notes) = groups(&[base, copy], &[template]);
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(g.len(), 3);
    let (elder, _) = stat_block_in(&creature("Elder Moss Wyrm"), Ruleset::Dnd2014, &g).unwrap();
    assert_eq!(
        elder.lair_actions.unwrap().closing,
        "Elder wyrms add a tremor."
    );
    let (ashen, _) = stat_block_in(&creature("Moss Wyrm (Ashen)"), Ruleset::Dnd2014, &g).unwrap();
    let regional = ashen.regional_effects.unwrap();
    assert_eq!(
        regional.closing,
        "The effects fade if the wyrm dies.\n\nAsh falls."
    );
    assert!(ashen.lair_actions.is_some());
}

#[test]
fn a_version_naming_a_missing_template_is_left_out_with_a_note() {
    let mut base = moss_group();
    base["_versions"] = json!([{"name": "Moss Wyrm (Ashen)", "source": "ZZZ",
        "_templates": [{"name": "Ashen", "source": "ZZZ"}]}]);
    let (g, notes) = groups(&[base], &[]);
    assert_eq!(g.len(), 1);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(g
        .get(&json!({"name": "Moss Wyrm (Ashen)", "source": "ZZZ"}))
        .is_none());
}

#[test]
fn sections_without_a_list_or_without_text() {
    let only_text = section(&[json!("One paragraph."), json!("Two.")]).unwrap();
    assert_eq!(only_text.preamble, "One paragraph.\n\nTwo.");
    assert!(only_text.options.is_empty() && only_text.closing.is_empty());
    assert!(section(&[]).is_none());
    assert!(section(&[json!({"type": "list", "items": ["  "]})]).is_none());
}
