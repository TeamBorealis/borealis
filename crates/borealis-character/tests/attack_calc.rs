//! A hand-made attack can take its bonus and damage from an ability (GitHub #29): the ability
//! modifier plus the proficiency bonus, and the typed dice plus the modifier.

use borealis_character::{with_modifier, ManualAttack};

const MODS: [i32; 6] = [3, 1, 2, 0, -1, 4];

fn bite(ability: &str, calculated: bool) -> ManualAttack {
    ManualAttack {
        name: "Bite".into(),
        attack: "+9".into(),
        damage: "1d6 piercing".into(),
        displayed: true,
        ability: ability.into(),
        calculated,
        ..ManualAttack::default()
    }
}

#[test]
fn the_modifier_goes_after_the_leading_dice() {
    assert_eq!(with_modifier("1d6 piercing", 3), "1d6+3 piercing");
    assert_eq!(with_modifier("2d6 + 1 fire", 4), "2d6+4 fire");
    assert_eq!(with_modifier("1d8-1", -2), "1d8-2");
    assert_eq!(with_modifier("1d4+2 bludgeoning", 0), "1d4 bludgeoning");
    assert_eq!(with_modifier("see notes", 3), "see notes");
}

#[test]
fn a_calculated_row_follows_its_ability() {
    assert_eq!(
        bite("Strength", true).worked_out(&MODS, 2),
        ("+5".to_string(), "1d6+3 piercing".to_string())
    );
    assert_eq!(
        bite("Charisma", true).worked_out(&MODS, 3),
        ("+7".to_string(), "1d6+4 piercing".to_string())
    );
    assert_eq!(
        bite("Wisdom", true).worked_out(&MODS, 2),
        ("+1".to_string(), "1d6-1 piercing".to_string())
    );
}

#[test]
fn a_typed_row_keeps_its_texts() {
    // an Aurora row and a row saved before GitHub #29 name an ability without being calculated
    assert_eq!(
        bite("Strength", false).worked_out(&MODS, 2),
        ("+9".to_string(), "1d6 piercing".to_string())
    );
    assert_eq!(
        bite("", true).worked_out(&MODS, 2),
        ("+9".to_string(), "1d6 piercing".to_string()),
        "no ability, nothing to work out"
    );
}
