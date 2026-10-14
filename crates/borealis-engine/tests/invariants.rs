//! The settle invariants (`borealis_engine::invariants`) hold on bundled characters and catch
//! each broken content shape they exist for.

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::invariants::{self, kind};
use borealis_engine::state::{CharacterState, Choice};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::OnceLock;

fn stat(name: &str, value: &str) -> Value {
    json!({"kind": "stat", "name": name, "value": value})
}

/// A synthetic race per broken shape (picked on a 2014 Fighter).
fn races() -> Vec<Value> {
    let race = |id: &str, extra: Value| {
        let mut r = json!({"id": id, "name": id, "type": "Race", "source": "Test"});
        if let (Some(r), Some(extra)) = (r.as_object_mut(), extra.as_object()) {
            r.extend(extra.clone());
        }
        r
    };
    vec![
        race(
            "ID_T_RACE_UNKNOWN_KIND",
            json!({"rules": [{"kind": "select.ability_distribution", "number": 3}]}),
        ),
        race(
            "ID_T_RACE_EMPTY_SELECT",
            json!({"rules": [{"kind": "select", "type": "Spell", "name": "Nothing",
                "supports": "ID_T_NO_SUCH_SPELL"}]}),
        ),
        race(
            "ID_T_RACE_MUTE_CASTER",
            json!({"spellcasting": {"name": "Mute", "ability": "Intelligence",
                "list": {"supports": "Wizard"}},
                "rules": [stat("mute:spellcasting:slots:1", "2")]}),
        ),
        race(
            "ID_T_RACE_PACT_NO_LEVEL",
            json!({"spellcasting": {"name": "Hollow", "ability": "Charisma",
                "list": {"supports": "Warlock"}},
                "rules": [stat("hollow:spellcasting:slots:count", "1"),
                    {"kind": "select", "type": "Spell", "name": "Spell Known (Hollow)",
                     "supports": "Warlock", "spellcasting": "Hollow"}]}),
        ),
    ]
}

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let file: ContentFile = serde_json::from_value(
            json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": races()}),
        )
        .expect("synthetic content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.2.1")))
            .expect("bundled packs")
            .add_file("synthetic.json", PackOrigin::Custom, file)
            .build();
        Engine::new(&store)
    })
}

fn pick(e: &Engine, s: &CharacterState, name: &str, id: &str) -> CharacterState {
    let key = slot_key(&e.derive(s), "ID_LEVEL_1", name, 1).expect("slot");
    commands::select(e, s, &key, Choice::Element(id.into()))
        .expect("select")
        .state
}

fn character(rs: Ruleset, race: Option<&str>, class: &str, level: u32) -> CharacterState {
    let e = engine();
    let mut s = commands::new_character(e, rs);
    if let Some(race) = race {
        s = pick(e, &s, "Race", race);
    }
    s = pick(e, &s, "Class", class);
    s = fill(e, s);
    while s.level() < level {
        s = commands::level_up_main(e, &s, None)
            .expect("level up")
            .state;
        s = fill(e, s);
    }
    s
}

/// Fill open selects with their first option or entry (skills before the Expertise that needs
/// them), like a user would.
fn fill(e: &Engine, mut s: CharacterState) -> CharacterState {
    let mut refused = std::collections::HashSet::new();
    for _ in 0..80 {
        let snap = e.derive_with(&s, true);
        let Some((key, choice)) = snap.slots.iter().find_map(|x| {
            if x.chosen.is_some() || x.list_choice.is_some() || refused.contains(&x.key) {
                return None;
            }
            let choice = match (x.options.first(), x.list_items.first()) {
                (Some(o), _) => Choice::Element(o.id.clone()),
                (None, Some((i, _))) => Choice::ListItem(*i),
                (None, None) => return None,
            };
            Some((x.key.clone(), choice))
        }) else {
            break;
        };
        match commands::select(e, &s, &key, choice) {
            Ok(o) => s = o.state,
            Err(_) => {
                refused.insert(key);
            }
        }
    }
    s
}

fn kinds(s: &CharacterState) -> Vec<&'static str> {
    let e = engine();
    let mut out: Vec<_> = invariants::check(e, s, &e.derive(s))
        .into_iter()
        .map(|v| v.kind)
        .collect();
    out.dedup();
    out
}

const FIGHTER: &str = "ID_WOTC_PHB_CLASS_FIGHTER";

/// The synthetic races carry rules the engine ignores; only their own characters may say so.
fn foreign(v: &&'static str) -> bool {
    *v != kind::UNKNOWN_RULE
}

#[test]
fn bundled_casters_and_martials_hold_every_invariant() {
    for (rs, class, level) in [
        (Ruleset::Dnd2014, "ID_WOTC_PHB_CLASS_WIZARD", 5),
        (Ruleset::Dnd2014, "ID_WOTC_PHB_CLASS_WARLOCK", 3),
        (Ruleset::Dnd2014, "ID_WOTC_PHB_CLASS_BARD", 10),
        (Ruleset::Dnd2014, FIGHTER, 4),
        (Ruleset::Dnd2024, "ID_2024_CLASS_WIZARD", 3),
        (Ruleset::Dnd2024, "ID_2024_CLASS_WARLOCK", 5),
    ] {
        let s = character(rs, None, class, level);
        let e = engine();
        let found: Vec<_> = invariants::check(e, &s, &e.derive(&s))
            .into_iter()
            .filter(|v| foreign(&v.kind))
            .map(|v| v.to_string())
            .collect();
        assert!(found.is_empty(), "{class} {level}: {found:?}");
    }
}

#[test]
fn an_ignored_rule_kind_is_a_violation() {
    let s = character(Ruleset::Dnd2014, Some("ID_T_RACE_UNKNOWN_KIND"), FIGHTER, 1);
    let e = engine();
    let found = invariants::check(e, &s, &e.derive(&s));
    assert!(
        found.iter().any(|v| v.kind == kind::UNKNOWN_RULE
            && v.element.as_deref() == Some("ID_T_RACE_UNKNOWN_KIND")),
        "{found:?}"
    );
}

#[test]
fn a_required_select_without_options_is_a_violation() {
    let s = character(Ruleset::Dnd2014, Some("ID_T_RACE_EMPTY_SELECT"), FIGHTER, 1);
    assert!(kinds(&s).contains(&kind::EMPTY_SELECT));
}

#[test]
fn a_caster_without_a_way_to_learn_spells_is_a_violation() {
    let s = character(Ruleset::Dnd2014, Some("ID_T_RACE_MUTE_CASTER"), FIGHTER, 1);
    assert!(kinds(&s).contains(&kind::NO_SPELL_ACQUISITION));
}

#[test]
fn pact_slots_without_a_level_are_a_violation() {
    let s = character(
        Ruleset::Dnd2014,
        Some("ID_T_RACE_PACT_NO_LEVEL"),
        FIGHTER,
        1,
    );
    assert!(kinds(&s).contains(&kind::PACT));
}

#[test]
fn the_thread_hook_panics_on_a_violation_and_only_when_enabled() {
    let s = character(Ruleset::Dnd2014, Some("ID_T_RACE_MUTE_CASTER"), FIGHTER, 1);
    let e = engine();
    let _ = e.derive(&s);
    invariants::enable_for_thread();
    let hooked = std::panic::catch_unwind(|| e.derive(&s));
    invariants::disable_for_thread();
    let err = hooked.expect_err("the hook panics");
    let text = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(text.contains(kind::NO_SPELL_ACQUISITION), "{text}");
    // derives without option lists (command internals) are not checked
    invariants::enable_for_thread();
    let _ = e.derive_with(&s, false);
    invariants::disable_for_thread();
}
