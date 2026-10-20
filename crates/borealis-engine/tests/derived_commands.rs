//! The commands that reuse a derive the caller already has (`select_derived`, `prune_derived`)
//! give exactly what the plain commands give.

mod common;
use borealis_engine::commands;
use borealis_engine::state::{CharacterState, Choice, Selection, SlotKey};
use common::*;

/// Every open slot's first options and the first option of another slot (not offered here):
/// `select_derived` with the state's full derive equals `select`, result or error.
fn same_selects(e: &borealis_engine::Engine, s: &CharacterState) -> usize {
    let snap = e.derive(s);
    let mut compared = 0;
    let foreign: Vec<String> = snap
        .slots
        .iter()
        .flat_map(|x| x.options.iter().map(|o| o.id.clone()))
        .collect();
    for slot in snap.slots.iter().filter(|x| x.chosen.is_none()) {
        let ids = slot
            .options
            .iter()
            .take(3)
            .map(|o| o.id.clone())
            .chain(
                foreign
                    .iter()
                    .find(|f| !slot.options.iter().any(|o| &o.id == *f))
                    .cloned(),
            )
            .chain(["ID_NOT_AN_ELEMENT".to_string()]);
        for id in ids {
            let plain = commands::select(e, s, &slot.key, Choice::Element(id.clone()));
            let derived =
                commands::select_derived(e, s, &snap, &slot.key, Choice::Element(id.clone()));
            assert_eq!(
                plain
                    .as_ref()
                    .map(|o| &o.state)
                    .map_err(ToString::to_string),
                derived
                    .as_ref()
                    .map(|o| &o.state)
                    .map_err(ToString::to_string),
                "select {id} into {}",
                slot.name
            );
            same_derive(e, derived.ok());
            compared += 1;
        }
    }
    compared
}

/// A derive an outcome carries is the derive of its state.
fn same_derive(e: &borealis_engine::Engine, out: Option<commands::CommandOutcome>) {
    if let Some(d) = out.as_ref().and_then(|o| o.derived.as_ref()) {
        let state = &out.as_ref().expect("outcome").state;
        assert_eq!(**d, e.derive(state), "carried derive");
    }
}

/// `prune_derived` with a full or a list-free derive of the state equals `prune`.
fn same_prunes(e: &borealis_engine::Engine, s: &CharacterState) {
    let plain = commands::prune(e, s);
    assert_eq!(plain, commands::prune_derived(s, &e.derive(s)));
    assert_eq!(plain, commands::prune_derived(s, &e.derive_with(s, false)));
}

#[test]
fn derived_select_and_prune_match_the_plain_commands() {
    each_variant("derived commands", |e, v| {
        let mut h = Hero::new(e, true);
        h.base([8, 14, 13, 15, 12, 10]);
        assert!(same_selects(e, &h.s) > 0, "{v:?}: level 1 compares picks");
        h.race_class(RACE_HUMAN, &class("Wizard"));
        let mut compared = 0;
        let mut carried = 0;
        for level in [1, 3] {
            while h.s.level() < level {
                let plain = commands::level_up_main(e, &h.s, None).expect("level up");
                let derived = commands::level_up_main_derived(e, &h.s, None).expect("level up");
                assert_eq!(plain.state, derived.state, "{v:?}: level up");
                carried += usize::from(derived.derived.is_some());
                same_derive(e, Some(derived));
                h.s = plain.state;
            }
            same_prunes(e, &h.s);
            compared += same_selects(e, &h.s);
        }
        assert!(compared > 3, "{v:?}: wizard picks compared");
        assert!(carried > 0, "{v:?}: a level up carries its derive");
        // a selection for a slot the character does not have is pruned either way
        let mut stray = h.s.clone();
        stray.selections.push(Selection {
            slot: SlotKey {
                owner: "ID_NOT_AN_ELEMENT".into(),
                rule: 0,
                ordinal: 1,
                instance: None,
            },
            choice: Choice::Element("ID_NOT_AN_ELEMENT".into()),
            replace_level: None,
        });
        same_prunes(e, &stray);
        assert_eq!(commands::prune(e, &stray), commands::prune(e, &h.s));
    });
}
