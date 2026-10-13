//! Command functions against the fixture SRD content (equipment, spells, abilities, money).

mod common;
use borealis_engine::commands::{self, slot_key};
use borealis_engine::money::Coin;
use borealis_engine::state::{AbilityMethod, EquipSlot};
use common::*;

const WIZARD_CASTING: &str = "ID_WOTC_PHB_CLASS_FEATURE_WIZARD_SPELLCASTING_WIZARD";

#[test]
fn point_buy_cannot_overspend_u1_and_standard_array_picks_u2() {
    each_variant("abilities", |e, _| {
        let mut h = Hero::new(e, true);
        h.s = commands::set_ability_method(&h.s, AbilityMethod::PointBuy);
        assert_eq!(h.s.abilities.base, [8; 6]);
        for i in 0..3 {
            h.s = commands::set_base_score(e, &h.s, i, 15).unwrap().state;
        }
        let snap = h.snap();
        let pb = snap.point_buy.expect("point buy summary");
        assert_eq!((pb.budget, pb.spent, pb.remaining), (27, 27, 0));
        let over = commands::set_base_score(e, &h.s, 3, 9);
        assert!(
            matches!(over, Err(commands::CommandError::Ability(_))),
            "The budget is enforced"
        );
        assert!(commands::set_base_score(e, &h.s, 9, 9).is_err());
        assert!(
            commands::set_base_score(e, &h.s, 0, 99).is_err(),
            "outside 3..=30"
        );
        let ok = commands::set_base_scores(e, &h.s, [15, 15, 15, 8, 8, 8])
            .unwrap()
            .state;
        assert_eq!(ok.abilities.base, [15, 15, 15, 8, 8, 8]);
        // house rule budget
        let mut rich = h.s.clone();
        rich.house_rules.point_buy_budget = Some(40);
        assert!(commands::set_base_score(e, &rich, 3, 15).is_ok());

        let mut s = commands::set_ability_method(&h.s, AbilityMethod::StandardArray);
        assert_eq!(s.abilities.base, [15, 14, 13, 12, 10, 8]);
        s = commands::pick_standard_array_value(&s, 5, 15)
            .unwrap()
            .state;
        assert_eq!(
            s.abilities.base,
            [8, 14, 13, 12, 10, 15],
            "swap keeps a permutation, no all-to-STR bug"
        );
        assert!(commands::pick_standard_array_value(&s, 0, 9).is_err());
        assert!(commands::set_base_scores(e, &s, [15, 15, 13, 12, 10, 8]).is_err());
        assert!(
            commands::set_base_score(e, &s, 0, 14).is_ok(),
            "set_base_score routes to the picker"
        );
        let manual = commands::set_ability_method(&s, AbilityMethod::Manual);
        assert_eq!(
            commands::set_base_scores(e, &manual, [3, 30, 12, 12, 12, 12])
                .unwrap()
                .state
                .abilities
                .base[1],
            30
        );
    });
}

#[test]
fn attunement_limit_equips_unattuned_e10() {
    each_variant("attune", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([10, 10, 10, 10, 10, 10]);
        let items = [
            "ID_WOTC_DMG_MAGIC_ITEM_GAUNTLETS_OF_OGRE_POWER",
            "ID_WOTC_DMG_MAGIC_ITEM_BELT_OF_FIRE_GIANT_STRENGTH",
            "ID_WOTC_DMG_MAGIC_ITEM_AMULET_OF_HEALTH",
        ];
        for i in items {
            let uid = h.give(i);
            let out = commands::equip(e, &h.s, &uid, None).unwrap();
            assert!(out.warnings.is_empty());
            h.s = out.state;
        }
        assert_eq!(h.snap().inventory.attuned, 3);
        let uid = h.give("ID_WOTC_DMG_MAGIC_ITEM_BELT_OF_HILL_GIANT_STRENGTH");
        let out = commands::equip(e, &h.s, &uid, None).unwrap();
        assert_eq!(
            out.warnings.len(),
            1,
            "UI warning for the 4th attunable item"
        );
        h.s = out.state;
        let snap = h.snap();
        assert_eq!(snap.inventory.attuned, 3);
        let line = snap.inventory.items.iter().find(|l| l.uid == uid).unwrap();
        assert!(line.equipped.is_some() && !line.attuned && line.requires_attunement);
        assert!(commands::attune(e, &h.s, &uid).is_err(), "limit reached");
        // house rule raises the limit
        h.s.house_rules.attunement_limit = Some(4);
        let s2 = commands::attune(e, &h.s, &uid).unwrap().state;
        assert_eq!(e.derive(&s2).inventory.attuned, 4);
        // and ending attunement frees a slot
        let s3 = commands::unattune(e, &s2, &uid).unwrap().state;
        assert_eq!(e.derive(&s3).inventory.attuned, 3);
        assert!(commands::attune(e, &s3, "nope").is_err());
        let torch_like = h.give("ID_WOTC_GEAR_SHIELD");
        assert!(
            commands::attune(e, &h.s, &torch_like).is_err(),
            "not equipped"
        );
    });
}

#[test]
fn hands_armor_and_storage() {
    each_variant("hands", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([16, 12, 12, 10, 10, 10]);
        let shield = h.equip("ID_WOTC_GEAR_SHIELD");
        assert_eq!(
            h.s.inventory.items[0].equipped,
            Some(EquipSlot::SecondaryHand)
        );
        let greatsword = h.equip("ID_WOTC_PHB_WEAPON_GREATSWORD");
        let s = &h.s.inventory;
        assert_eq!(
            s.items
                .iter()
                .find(|i| i.uid == greatsword)
                .unwrap()
                .equipped,
            Some(EquipSlot::PrimaryHand)
        );
        assert_eq!(
            s.items.iter().find(|i| i.uid == shield).unwrap().equipped,
            None,
            "a two-handed weapon frees the shield hand"
        );
        h.s = commands::equip(e, &h.s, &shield, None).unwrap().state;
        assert_eq!(
            h.s.inventory
                .items
                .iter()
                .find(|i| i.uid == greatsword)
                .unwrap()
                .equipped,
            None,
            "a shield cannot join a two-hander"
        );
        let longsword = h.equip("ID_WOTC_PHB_WEAPON_LONGSWORD");
        let dagger = h.equip("ID_WOTC_PHB_WEAPON_DAGGER");
        let find = |h: &Hero<'_>, u: &str| {
            h.s.inventory
                .items
                .iter()
                .find(|i| i.uid == u)
                .unwrap()
                .equipped
        };
        assert_eq!(find(&h, &longsword), Some(EquipSlot::PrimaryHand));
        assert_eq!(
            find(&h, &dagger),
            Some(EquipSlot::SecondaryHand),
            "second one-handed weapon takes the off-hand"
        );
        let snap = h.snap();
        assert_eq!(snap.attacks.len(), 2);
        let ls = snap.attacks.iter().find(|a| a.name == "Longsword").unwrap();
        assert_eq!(ls.ability, "strength");
        assert_eq!(ls.damage, "1d8+3 slashing");
        h.s = commands::unequip(e, &h.s, &longsword).unwrap().state;
        assert_eq!(find(&h, &longsword), None);
        // storage
        assert!(commands::move_to_storage(&h.s, &dagger, Some("Chest")).is_err());
        h.s.inventory.storages.push("Chest".into());
        h.s = commands::move_to_storage(&h.s, &dagger, Some("Chest"))
            .unwrap()
            .state;
        let it =
            h.s.inventory
                .items
                .iter()
                .find(|i| i.uid == dagger)
                .unwrap();
        assert_eq!((it.storage.as_deref(), it.equipped), (Some("Chest"), None));
        h.s = commands::move_to_storage(&h.s, &dagger, None)
            .unwrap()
            .state;
        // armor swap and adorner
        h.equip("ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL");
        h.equip("ID_WOTC_ARMOR_MEDIUM_BREASTPLATE");
        let armors =
            h.s.inventory
                .items
                .iter()
                .filter(|i| i.equipped == Some(EquipSlot::Armor))
                .count();
        assert_eq!(armors, 1);
        let uid =
            h.s.inventory
                .items
                .iter()
                .find(|i| i.equipped == Some(EquipSlot::Armor))
                .unwrap()
                .uid
                .clone();
        let base_ac = h.snap().armor_class;
        h.s = commands::set_adorner(e, &h.s, &uid, Some("ID_WOTC_DMG_MAGIC_ITEM_ARMOR_1"))
            .unwrap()
            .state;
        assert_eq!(h.snap().armor_class, base_ac + 1, "+1 armor adorner");
        assert!(commands::set_adorner(e, &h.s, &uid, Some("ID_NOPE")).is_err());
        assert!(commands::equip(e, &h.s, "nope", None).is_err());
        let rope = h.give("ID_WOTC_PHB_ITEM_ROPE_HEMPEN_50FEET");
        assert!(
            commands::equip(e, &h.s, &rope, None).is_err(),
            "rope has no slot"
        );
        h.s = commands::remove_item(e, &h.s, &rope).unwrap().state;
        assert!(commands::add_item(e, &h.s, "ID_NOT_AN_ITEM", 1).is_err());
    });
}

#[test]
fn encumbrance_and_money() {
    each_variant("money", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([16, 10, 10, 10, 10, 10])
            .race_class(RACE_HUMAN, &class("Fighter"));
        h.s = commands::deposit(&h.s, Coin::Gp, 100);
        h.s = commands::deposit(&h.s, Coin::Sp, 50);
        h.s = commands::deposit(&h.s, Coin::Cp, 10);
        h.equip("ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL");
        let inv = h.snap().inventory;
        assert!((inv.gold_value - 105.10).abs() < 1e-9);
        assert_eq!(inv.capacity, 17.0 * 15.0, "human medium, STR 16 + 1");
        assert!(
            (inv.weight - (55.0 + 3.2)).abs() < 1e-9,
            "armor + 160 coins at 50 per pound: {}",
            inv.weight
        );
        h.s = commands::withdraw(&h.s, Coin::Gp, 25).unwrap().state;
        assert_eq!(h.s.inventory.coins.gp, 75);
        assert!(commands::withdraw(&h.s, Coin::Pp, 100).is_err());
    });
}

#[test]
fn wizard_spells_levels_and_preparation() {
    each_variant("spells", |e, v| {
        let mut h = Hero::new(e, true);
        h.base([8, 14, 13, 15, 12, 10])
            .race_class(RACE_HUMAN, &class("Wizard"));
        h.level_to(4);
        let snap = h.snap();
        // the level-4 spellbook slot offers spell levels 1 and 2 only
        let slot4 = snap
            .slots
            .iter()
            .find(|s| s.name == "Spellbook (Wizard)" && s.level == 4)
            .expect("level 4 spellbook slot");
        let levels: std::collections::BTreeSet<u32> =
            slot4.options.iter().filter_map(|o| o.level).collect();
        assert_eq!(levels, [1, 2].into_iter().collect(), "{v:?}");
        let cantrips = snap
            .slots
            .iter()
            .find(|s| s.name == "Cantrip (Wizard)" && s.level == 1)
            .unwrap();
        assert!(
            cantrips.options.iter().all(|o| o.level == Some(0)) && !cantrips.options.is_empty()
        );
        h.level_to(5);
        let snap = h.snap();
        let slot5 = snap
            .slots
            .iter()
            .find(|s| s.name == "Spellbook (Wizard)" && s.level == 5)
            .unwrap();
        let l5: std::collections::BTreeSet<u32> =
            slot5.options.iter().filter_map(|o| o.level).collect();
        assert_eq!(l5, [1, 2, 3].into_iter().collect());
        // learn spells, prepare within the limit (INT 3 + 5 = 8)
        let book: Vec<_> = snap
            .slots
            .iter()
            .filter(|s| s.name == "Spellbook (Wizard)" && s.level == 1)
            .map(|s| s.key.clone())
            .collect();
        assert_eq!(book.len(), 6);
        let level1: Vec<String> = snap.slots[0].options.iter().map(|o| o.id.clone()).collect();
        let _ = level1;
        let first = &book[0];
        let opts = snap
            .slots
            .iter()
            .find(|s| &s.key == first)
            .unwrap()
            .options
            .clone();
        for (k, o) in book.iter().zip(opts.iter()) {
            h.pick_key(k, &o.id);
        }
        let snap = h.snap();
        let w = snap.caster("Wizard").unwrap();
        assert_eq!(
            w.spells.iter().filter(|s| s.known && s.level == 1).count(),
            6
        );
        assert_eq!(w.prepare_limit, Some(8));
        for sp in w.spells.iter().filter(|s| s.level == 1).take(6) {
            h.s = commands::set_prepared(e, &h.s, "Wizard", &sp.id, true)
                .unwrap()
                .state;
        }
        assert_eq!(h.snap().caster("Wizard").unwrap().prepared_count, 6);
        let one = h.snap().caster("Wizard").unwrap().spells[0].id.clone();
        h.s = commands::set_prepared(e, &h.s, "Wizard", &one, false)
            .unwrap()
            .state;
        assert_eq!(h.snap().caster("Wizard").unwrap().prepared_count, 5);
        assert!(commands::set_prepared(e, &h.s, "Wizard", "ID_NOT_A_SPELL", true).is_err());
        assert!(commands::set_prepared(e, &h.s, "Cleric", &one, true).is_err());
        let _ = WIZARD_CASTING;
    });
}

#[test]
fn cleric_prepares_from_the_whole_list_and_always_prepared_is_fixed() {
    each_variant("cleric", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([10, 10, 10, 10, 16, 10])
            .race_class(RACE_HUMAN, &class("Cleric"));
        h.pick_any("Archetype", "ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN");
        let c = h.snap().caster("Cleric").cloned().unwrap();
        assert!(c.prepare_from_list);
        let always = c
            .spells
            .iter()
            .find(|s| s.always_prepared)
            .expect("domain spell")
            .id
            .clone();
        assert!(commands::set_prepared(e, &h.s, "Cleric", &always, false).is_err());
        let list_spell = c
            .spells
            .iter()
            .find(|s| !s.known && s.level == 1)
            .expect("list spell")
            .id
            .clone();
        h.s = commands::set_prepared(e, &h.s, "Cleric", &list_spell, true)
            .unwrap()
            .state;
        assert_eq!(h.snap().caster("Cleric").unwrap().prepared_count, 1);
        let snap = h.snap();
        let cantrip = snap
            .caster("Cleric")
            .unwrap()
            .spells
            .iter()
            .find(|s| s.level == 0);
        if let Some(c0) = cantrip {
            assert!(commands::set_prepared(e, &h.s, "Cleric", &c0.id.clone(), true).is_err());
        }
    });
}

#[test]
fn imported_style_state_survives_unknown_content() {
    each_variant("defaults", |e, _| {
        let mut h = Hero::new(e, true);
        h.race_class(RACE_HUMAN, &class("Fighter"));
        let filled = commands::apply_defaults(e, &h.s);
        assert_eq!(
            filled.selections.len(),
            h.s.selections.len(),
            "SRD classes declare no defaults"
        );
        let key = slot_key(&h.snap(), "ID_LEVEL_1", "Alignment", 1).unwrap();
        h.pick_key(&key, "ID_ALIGNMENT_NEUTRAL_GOOD");
        let again = commands::select(
            e,
            &h.s,
            &key,
            borealis_engine::Choice::Element("ID_ALIGNMENT_NEUTRAL_GOOD".into()),
        )
        .unwrap();
        assert_eq!(
            again
                .state
                .selections
                .iter()
                .filter(|s| s.slot == key)
                .count(),
            1,
            "re-selecting is idempotent"
        );
    });
}

#[test]
fn retrained_spell_slots_offer_current_class_level_options() {
    each_variant("retrain", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([8, 14, 13, 15, 12, 10])
            .race_class(RACE_HUMAN, &class("Wizard"));
        h.level_to(5);
        let snap = h.snap();
        let slot = snap
            .slots
            .iter()
            .find(|s| s.name == "Spellbook (Wizard)" && s.level == 3)
            .unwrap();
        let before: std::collections::BTreeSet<u32> =
            slot.options.iter().filter_map(|o| o.level).collect();
        assert_eq!(before, [1, 2].into_iter().collect());
        let key = slot.key.clone();
        let first = slot.options[0].id.clone();
        h.pick_key(&key, &first);
        h.s = commands::set_replace_level(&h.s, &key, 5).unwrap().state;
        let snap = h.snap();
        let slot = snap.slots.iter().find(|s| s.key == key).unwrap();
        let after: std::collections::BTreeSet<u32> =
            slot.options.iter().filter_map(|o| o.level).collect();
        assert_eq!(
            after,
            [1, 2, 3].into_iter().collect(),
            "retraining uses the current class level"
        );
    });
}

#[test]
fn item_costs_are_decimal_gold() {
    each_variant("cost", |e, _| {
        let mut h = Hero::new(e, true);
        h.give("ID_WOTC_PHB_WEAPON_CLUB");
        let snap = h.snap();
        assert!(
            (snap.inventory.items[0].cost_gp - 0.1).abs() < 1e-12,
            "1 sp"
        );
    });
}

#[test]
fn improvement_command_applies_asi_and_feats() {
    each_variant("improvement", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([13, 12, 14, 10, 10, 8])
            .race_class(RACE_HUMAN, &class("Fighter"));
        h.level_to(4);
        h.s = commands::choose_improvement(
            e,
            &h.s,
            "Fighter",
            4,
            &commands::Improvement::PlusOneEach(0, 2),
        )
        .unwrap()
        .state;
        let s = h.snap();
        assert_eq!(
            (
                s.ability("strength").unwrap().score,
                s.ability("constitution").unwrap().score
            ),
            (15, 16)
        );
        h.s =
            commands::choose_improvement(e, &h.s, "Fighter", 4, &commands::Improvement::PlusTwo(1))
                .unwrap()
                .state;
        let s = h.snap();
        assert_eq!(
            (
                s.ability("strength").unwrap().score,
                s.ability("dexterity").unwrap().score
            ),
            (14, 15),
            "the earlier picks were replaced"
        );
        h.s = commands::choose_improvement(
            e,
            &h.s,
            "Fighter",
            4,
            &commands::Improvement::Feat("ID_PHB_FEAT_GRAPPLER".into()),
        )
        .unwrap()
        .state;
        assert!(h.snap().has_element("ID_PHB_FEAT_GRAPPLER"));
        assert!(commands::choose_improvement(
            e,
            &h.s,
            "Fighter",
            4,
            &commands::Improvement::PlusOneEach(1, 1)
        )
        .is_err());
        assert!(
            commands::choose_improvement(e, &h.s, "Fighter", 6, &commands::Improvement::PlusTwo(1))
                .is_err(),
            "level 6 slot is not open yet"
        );
        assert!(commands::choose_improvement(
            e,
            &h.s,
            "Fighter",
            4,
            &commands::Improvement::PlusTwo(9)
        )
        .is_err());
    });
}
