//! The ability a weapon attack line uses can be picked by the user (Aurora's "Ability (automatic
//! calculation)" of an attack, GitHub #27), and listing a weapon keeps its line (Aurora's "Add to
//! Attacks & Spellcasting", #28).

mod common;
use borealis_engine::commands;
use common::*;

const LONGSWORD: &str = "ID_WOTC_PHB_WEAPON_LONGSWORD";
const DAGGER: &str = "ID_WOTC_PHB_WEAPON_DAGGER";

#[test]
fn a_picked_ability_recalculates_the_attack_and_damage() {
    each_variant("attack_ability", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([16, 10, 12, 10, 10, 18]);
        let sword = h.equip(LONGSWORD);
        let before = h.snap();
        let line = before.attacks.iter().find(|a| a.uid == sword).unwrap();
        assert_eq!(line.ability, "strength");
        assert_eq!(line.damage, "1d8+3 slashing");
        // the line carries what every ability would give, for the editor's live preview
        assert_eq!(line.ability_lines.len(), 6);
        assert_eq!(line.ability_lines[0].attack, line.attack);
        assert_eq!(line.ability_lines[5].attack, line.attack - 3 + 4);
        assert_eq!(line.ability_lines[5].damage, "1d8+4 slashing");

        h.s = commands::set_attack_ability(e, &h.s, &sword, Some("charisma"))
            .unwrap()
            .state;
        let after = h.snap();
        let line2 = after.attacks.iter().find(|a| a.uid == sword).unwrap();
        assert_eq!(line2.ability, "charisma");
        assert_eq!(
            line2.attack,
            line.attack + 1,
            "+4 Charisma replaces +3 Strength"
        );
        assert_eq!(line2.damage, "1d8+4 slashing");

        // back to the rules' choice
        h.s = commands::set_attack_ability(e, &h.s, &sword, None)
            .unwrap()
            .state;
        assert!(h.s.inventory.attack_abilities.is_empty());
        let line3 = h.snap();
        let line3 = line3.attacks.iter().find(|a| a.uid == sword).unwrap();
        assert_eq!(line3.ability, "strength");
    });
}

#[test]
fn an_unknown_ability_or_line_is_refused_and_removing_the_item_forgets_the_pick() {
    each_variant("attack_ability_errors", |e, _| {
        let mut h = Hero::new(e, true);
        let sword = h.equip(LONGSWORD);
        assert!(commands::set_attack_ability(e, &h.s, &sword, Some("luck")).is_err());
        assert!(commands::set_attack_ability(e, &h.s, "item-999", Some("strength")).is_err());
        h.s = commands::set_attack_ability(e, &h.s, &sword, Some("dexterity"))
            .unwrap()
            .state;
        h.s = commands::remove_item(e, &h.s, &sword).unwrap().state;
        assert!(h.s.inventory.attack_abilities.is_empty());
    });
}

#[test]
fn listing_a_weapon_keeps_its_line_while_it_is_not_held() {
    each_variant("list_attack", |e, _| {
        let mut h = Hero::new(e, true);
        let dagger = h.give(DAGGER);
        assert!(!h.snap().attacks.iter().any(|a| a.uid == dagger));
        h.s = commands::list_attack(e, &h.s, &dagger, true).unwrap().state;
        assert!(h.snap().attacks.iter().any(|a| a.uid == dagger));
        // listing twice keeps one entry
        h.s = commands::list_attack(e, &h.s, &dagger, true).unwrap().state;
        assert_eq!(h.s.inventory.listed_attacks, vec![dagger.clone()]);
        h.s = commands::list_attack(e, &h.s, &dagger, false)
            .unwrap()
            .state;
        assert!(!h.snap().attacks.iter().any(|a| a.uid == dagger));
    });
}

#[test]
fn only_weapons_can_be_listed() {
    each_variant("list_attack_weapon_only", |e, _| {
        let mut h = Hero::new(e, true);
        let shield = h.give("ID_WOTC_GEAR_SHIELD");
        assert!(commands::list_attack(e, &h.s, &shield, true).is_err());
        assert!(commands::list_attack(e, &h.s, "item-999", true).is_err());
    });
}
