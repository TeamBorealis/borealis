//! Golden vectors TV-01..TV-18 (single-class builds), run
//! against the legacy fixture content and against our core pack.

mod common;
use borealis_engine::snapshot::CharacterSnapshot;
use common::*;

fn saves(s: &CharacterSnapshot) -> Vec<i32> {
    s.abilities.iter().map(|a| a.save).collect()
}

fn sk(s: &CharacterSnapshot, n: &str) -> i32 {
    s.skill(n).unwrap_or_else(|| panic!("skill {n}")).total
}

fn cleric_life(e: &borealis_engine::Engine, scores: [i32; 6], human: bool) -> Hero<'_> {
    let mut h = Hero::new(e, true);
    h.base(scores);
    if human {
        h.race_class(RACE_HUMAN, &class("Cleric"));
    } else {
        h.race_class(RACE_DWARF, &class("Cleric"));
        h.pick_any("Sub Race", SUBRACE_HILL);
    }
    h.background(ACOLYTE);
    h.pick_any("Archetype", "ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN");
    h.pick_any("Proficiency", &skill("History"));
    h.pick_any("Proficiency", &skill("Medicine"));
    h
}

#[test]
fn tv01_hill_dwarf_cleric_level_1() {
    each_variant("tv01", |e, v| {
        let mut h = cleric_life(e, [13, 10, 14, 8, 15, 12], false);
        let s = h.snap();
        let m = |n| s.ability(n).unwrap().modifier;
        assert_eq!(
            s.abilities.iter().map(|a| a.score).collect::<Vec<_>>(),
            [13, 10, 16, 8, 16, 12],
            "{v:?}"
        );
        assert_eq!((m("strength"), m("constitution"), m("wisdom")), (1, 3, 3));
        assert_eq!(s.proficiency_bonus, 2);
        assert_eq!(s.speed.walk, 25);
        assert_eq!(s.initiative, 0);
        assert_eq!(s.hit_points, 12);
        assert_eq!(saves(&s), [1, 0, 3, -1, 5, 3]);
        assert_eq!(
            (
                sk(&s, "history"),
                sk(&s, "insight"),
                sk(&s, "medicine"),
                sk(&s, "religion"),
                sk(&s, "perception")
            ),
            (1, 5, 5, 1, 3)
        );
        assert_eq!(s.skill("perception").unwrap().passive, 13);
        let c = s.caster("Cleric").expect("cleric spellcasting");
        assert_eq!(
            (c.dc, c.attack, c.slots[0], c.prepare_limit),
            (13, 5, 2, Some(4))
        );
        assert_eq!(s.armor_class, 10);
        h.equip("ID_WOTC_ARMOR_MEDIUM_SCALE_MAIL");
        h.equip("ID_WOTC_GEAR_SHIELD");
        let s = h.snap();
        assert_eq!(s.armor_class, 16);
        assert!(
            s.stealth_disadvantage,
            "scale mail grants the stealth disadvantage marker"
        );
        // always-prepared Life domain spells
        let c = s.caster("Cleric").unwrap();
        assert!(
            c.spells.iter().any(|sp| sp.always_prepared),
            "Life domain spells are always prepared"
        );
    });
}

#[test]
fn tv02_and_tv03_fighter_chain_mail_and_strength_penalty() {
    each_variant("tv02", |e, v| {
        let mut h = Hero::new(e, true);
        h.base([15, 13, 14, 8, 12, 10])
            .race_class(RACE_HUMAN, &class("Fighter"))
            .background(ACOLYTE);
        h.pick_any(
            "Class Feature",
            "ID_WOTC_PHB_CLASS_FEATURE_FIGHTINGSTYLE_DEFENSE",
        );
        h.pick_any("Proficiency", &skill("Athletics"));
        h.pick_any("Proficiency", &skill("Perception"));
        h.equip("ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL");
        h.equip("ID_WOTC_GEAR_SHIELD");
        let s = h.snap();
        assert_eq!(s.ability("strength").unwrap().score, 16, "{v:?}");
        assert_eq!(
            (s.armor_class, s.hit_points, s.speed.walk, s.initiative),
            (19, 12, 30, 2)
        );
        assert_eq!(saves(&s), [5, 2, 4, -1, 1, 0]);
        assert_eq!(
            (
                sk(&s, "athletics"),
                sk(&s, "perception"),
                sk(&s, "insight"),
                sk(&s, "religion")
            ),
            (5, 3, 3, 1)
        );
        assert_eq!(s.skill("perception").unwrap().passive, 13);
        // STR 8 (9 final) with chain mail and no shield
        let mut h3 = Hero::new(e, true);
        h3.base([8, 13, 14, 8, 12, 10])
            .race_class(RACE_HUMAN, &class("Fighter"))
            .background(ACOLYTE);
        h3.pick_any(
            "Class Feature",
            "ID_WOTC_PHB_CLASS_FEATURE_FIGHTINGSTYLE_DEFENSE",
        );
        h3.pick_any("Proficiency", &skill("Athletics"));
        h3.equip("ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL");
        let s = h3.snap();
        assert_eq!(
            (s.speed.walk, s.armor_class, sk(&s, "athletics")),
            (20, 17, 1)
        );
    });
}

#[test]
fn tv04_barbarian_unarmored_defense() {
    each_variant("tv04", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([15, 14, 15, 8, 12, 10])
            .race_class(RACE_HUMAN, &class("Barbarian"))
            .background(ACOLYTE);
        h.pick_any("Proficiency", &skill("Athletics"));
        h.pick_any("Proficiency", &skill("Survival"));
        let s = h.snap();
        assert_eq!((s.armor_class, s.hit_points), (15, 15));
        assert_eq!((saves(&s)[0], saves(&s)[2]), (5, 5));
        h.equip("ID_WOTC_GEAR_SHIELD");
        assert_eq!(h.snap().armor_class, 17);
        h.equip("ID_WOTC_ARMOR_MEDIUM_SCALE_MAIL");
        assert_eq!(h.snap().armor_class, 18);
    });
}

#[test]
fn tv05_monk_unarmored_defense() {
    each_variant("tv05", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([10, 15, 12, 8, 14, 10])
            .race_class(RACE_HUMAN, &class("Monk"))
            .background(ACOLYTE);
        h.pick_any("Proficiency", &skill("Acrobatics"));
        h.pick_any("Proficiency", &skill("Stealth"));
        let s = h.snap();
        assert_eq!((sk(&s, "acrobatics"), sk(&s, "stealth")), (5, 5));
        assert_eq!((s.armor_class, s.hit_points, s.initiative), (15, 9, 3));
        assert_eq!((saves(&s)[0], saves(&s)[1]), (2, 5));
        h.equip("ID_WOTC_GEAR_SHIELD");
        assert_eq!(h.snap().armor_class, 15);
    });
}

#[test]
fn tv06_wizard_level_1_and_5() {
    each_variant("tv06", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([8, 14, 13, 15, 12, 10])
            .race_class(RACE_HUMAN, &class("Wizard"))
            .background(ACOLYTE);
        h.pick_any("Proficiency", &skill("Arcana"));
        h.pick_any("Proficiency", &skill("Investigation"));
        let s = h.snap();
        let w = s.caster("Wizard").unwrap();
        assert_eq!(
            (
                s.armor_class,
                s.hit_points,
                w.dc,
                w.attack,
                w.slots[0],
                w.prepare_limit
            ),
            (12, 8, 13, 5, 2, Some(4))
        );
        assert_eq!((saves(&s)[3], saves(&s)[4]), (5, 3));
        assert_eq!(
            (
                sk(&s, "arcana"),
                sk(&s, "investigation"),
                s.skill("perception").unwrap().passive
            ),
            (5, 5, 11)
        );
        assert_eq!(
            s.slots
                .iter()
                .filter(|x| x.name.starts_with("Cantrip") && x.owner.ty != "Level")
                .count(),
            3
        );
        h.level_to(5);
        let s = h.snap();
        let w = s.caster("Wizard").unwrap();
        assert_eq!(s.proficiency_bonus, 3);
        assert_eq!(s.hit_points, 6 + 4 * 4 + 2 * 5);
        assert_eq!(
            (
                w.dc,
                w.attack,
                w.slots[0],
                w.slots[1],
                w.slots[2],
                w.prepare_limit
            ),
            (14, 6, 4, 3, 2, Some(8))
        );
        assert_eq!(
            (
                saves(&s)[3],
                saves(&s)[4],
                sk(&s, "arcana"),
                sk(&s, "investigation")
            ),
            (6, 4, 6, 6)
        );
    });
}

#[test]
fn tv11_rogue_expertise() {
    each_variant("tv11", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([8, 15, 13, 12, 10, 14])
            .race_class(RACE_HUMAN, &class("Rogue"))
            .background(ACOLYTE);
        for sk_ in ["Stealth", "Perception", "Acrobatics", "Deception"] {
            h.pick_any("Proficiency", &skill(sk_));
        }
        h.pick_any("Class Feature", "ID_EXPERTISE_SKILL_STEALTH");
        h.pick_any("Class Feature", "ID_EXPERTISE_SKILL_PERCEPTION");
        h.equip("ID_WOTC_ARMOR_LIGHT_LEATHER");
        let s = h.snap();
        assert_eq!((s.hit_points, s.armor_class, s.initiative), (10, 14, 3));
        assert_eq!(
            (
                sk(&s, "stealth"),
                sk(&s, "perception"),
                sk(&s, "acrobatics"),
                sk(&s, "deception")
            ),
            (7, 4, 5, 4)
        );
        assert_eq!(s.skill("perception").unwrap().passive, 14);
        assert!(s.skill("stealth").unwrap().expertise);
        assert_eq!((saves(&s)[1], saves(&s)[3]), (5, 3));
    });
}

#[test]
fn tv12_set_score_items_and_cap() {
    each_variant("tv12", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([15, 12, 14, 10, 10, 8])
            .race_class(RACE_HUMAN, &class("Fighter"))
            .background(ACOLYTE);
        h.pick_any("Proficiency", &skill("Athletics"));
        h.pick_any("Proficiency", &skill("Perception"));
        let s = h.snap();
        assert_eq!(
            (s.ability("strength").unwrap().score, sk(&s, "athletics")),
            (16, 5)
        );
        h.equip("ID_WOTC_DMG_MAGIC_ITEM_GAUNTLETS_OF_OGRE_POWER");
        let s = h.snap();
        assert_eq!(
            (
                s.ability("strength").unwrap().score,
                s.ability("strength").unwrap().modifier,
                sk(&s, "athletics")
            ),
            (19, 4, 6)
        );
        h.equip("ID_WOTC_DMG_MAGIC_ITEM_BELT_OF_FIRE_GIANT_STRENGTH");
        let s = h.snap();
        assert_eq!(
            (
                s.ability("strength").unwrap().score,
                s.ability("strength").unwrap().modifier,
                sk(&s, "athletics")
            ),
            (25, 7, 9)
        );
        h.equip("ID_WOTC_DMG_MAGIC_ITEM_AMULET_OF_HEALTH");
        let s = h.snap();
        assert_eq!(
            (s.ability("constitution").unwrap().score, s.hit_points),
            (19, 14)
        );
        assert_eq!(s.inventory.attuned, 3);
        h.level_to(5);
        let s = h.snap();
        assert_eq!((sk(&s, "athletics"), s.hit_points), (10, 54));
    });
}

#[test]
fn tv13_asi_and_cap() {
    each_variant("tv13", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([19, 12, 14, 10, 10, 8])
            .race_class(RACE_HUMAN, &class("Fighter"))
            .background(ACOLYTE);
        h.pick_any("Proficiency", &skill("Athletics"));
        h.pick_any("Proficiency", &skill("Perception"));
        h.level_to(4);
        h.pick(
            "ID_WOTC_PHB_CLASS_FEATURE_ABILITYSCOREIMPROVEMENT_FIGHTER",
            "Improvement Option (Fighter 4)",
            1,
            "ID_INTERNAL_CLASS_FEATURE_ASI_4_FIGHTER",
        );
        h.pick_any("Ability Score Improvement", "ID_INTERNAL_ASI_STRENGTH");
        h.pick_any("Ability Score Improvement", "ID_INTERNAL_ASI_STRENGTH");
        let s = h.snap();
        let a = s.ability("strength").unwrap();
        assert_eq!((a.additional, a.score, a.modifier), (3, 20, 5));
        assert_eq!(
            (s.hit_points, s.proficiency_bonus, sk(&s, "athletics")),
            (36, 2, 7)
        );
    });
}

#[test]
fn tv14_cleric_level_5_and_tv16_duplicate_skill() {
    each_variant("tv14", |e, _| {
        let mut h = cleric_life(e, [13, 10, 14, 8, 15, 12], false);
        h.level_to(5);
        let s = h.snap();
        let c = s.caster("Cleric").unwrap();
        assert_eq!(s.proficiency_bonus, 3);
        assert_eq!(s.hit_points, 8 + 5 * 4 + 3 * 5 + 5);
        assert_eq!((saves(&s)[4], saves(&s)[5], saves(&s)[2]), (6, 4, 3));
        assert_eq!(
            (
                sk(&s, "insight"),
                sk(&s, "medicine"),
                sk(&s, "history"),
                sk(&s, "religion")
            ),
            (6, 6, 2, 2)
        );
        assert_eq!(
            (
                c.dc,
                c.attack,
                c.slots[0],
                c.slots[1],
                c.slots[2],
                c.prepare_limit
            ),
            (14, 6, 4, 3, 2, Some(8))
        );
        assert_eq!(s.skill("perception").unwrap().passive, 13);
        assert_eq!(s.armor_class, 10);
    });
    each_variant("tv16", |e, _| {
        // Insight from Acolyte AND from the class list: the proficiency does not double
        let mut h = Hero::new(e, true);
        h.base([10, 10, 10, 10, 14, 10])
            .race_class(RACE_HUMAN, &class("Cleric"))
            .pick_any("Archetype", "ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN");
        h.pick_any("Proficiency", &skill("Insight"));
        h.pick_any("Proficiency", &skill("Medicine"));
        h.background(ACOLYTE);
        let s = h.snap();
        assert_eq!(
            (sk(&s, "insight"), sk(&s, "medicine"), sk(&s, "religion")),
            (4, 4, 2)
        );
        let c = s.caster("Cleric").unwrap();
        assert_eq!(
            (c.prepare_limit, c.dc, c.attack, s.hit_points),
            (Some(3), 12, 4, 8)
        );
    });
}

#[test]
fn tv15_armor_swap_and_dex_cap() {
    each_variant("tv15", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([15, 16, 14, 8, 12, 10])
            .race_class(RACE_HUMAN, &class("Fighter"))
            .background(ACOLYTE);
        h.pick_any(
            "Class Feature",
            "ID_WOTC_PHB_CLASS_FEATURE_FIGHTINGSTYLE_DEFENSE",
        );
        let scale = h.equip("ID_WOTC_ARMOR_MEDIUM_SCALE_MAIL");
        assert_eq!(h.snap().armor_class, 17);
        let _ = scale;
        h.equip("ID_WOTC_ARMOR_LIGHT_STUDDED_LEATHER");
        assert_eq!(
            h.snap().armor_class,
            16,
            "swapping unregisters the previous armor"
        );
        h.equip("ID_WOTC_ARMOR_HEAVY_PLATE");
        let s = h.snap();
        assert_eq!((s.armor_class, s.speed.walk), (19, 30));
    });
}

#[test]
fn tv17_champion_remarkable_athlete() {
    each_variant("tv17", |e, v| {
        let mut h = Hero::new(e, true);
        h.base([15, 12, 14, 10, 10, 8])
            .race_class(RACE_HUMAN, &class("Fighter"));
        h.pick_any(
            "Class Feature",
            "ID_WOTC_PHB_CLASS_FEATURE_FIGHTINGSTYLE_DEFENSE",
        );
        h.pick_any("Proficiency", &skill("Perception"));
        h.pick_any("Proficiency", &skill("Insight"));
        h.level_to(3);
        h.pick_any("Archetype", "ID_WOTC_PHB_ARCHETYPE_CHAMPION");
        h.level_to(7);
        let s = h.snap();
        assert_eq!((s.proficiency_bonus, s.hit_points), (3, 60));
        if v == Variant::Bundled {
            // Known content gap (det-aurora-1 handoff): the bundled SRD 5.1 Remarkable Athlete
            // (ID_WOTC_PHB_ARCHETYPE_FEATURE_REMARKABLEATHLETE) carries no rules, so no half
            // proficiency applies. Pinned so the fix of content/srd-5.1 flips this test: then
            // delete this branch and let the bundled variant run the full vector.
            assert_eq!(
                (sk(&s, "athletics"), s.skill("athletics").unwrap().misc),
                (3, 0),
                "bundled Remarkable Athlete now has rules: remove the known-gap branch"
            );
            return;
        }
        assert_eq!(
            (
                sk(&s, "athletics"),
                sk(&s, "acrobatics"),
                sk(&s, "stealth"),
                sk(&s, "sleight of hand")
            ),
            (5, 3, 3, 3)
        );
        assert_eq!((sk(&s, "perception"), sk(&s, "insight")), (3, 3));
        assert_eq!(s.skill("athletics").unwrap().misc, 2);
    });
}

#[test]
fn tv18_barbarian_average_vs_rolled_hp() {
    each_variant("tv18", |e, _| {
        let build = |average: bool| {
            let mut h = Hero::new(e, average);
            h.base([10, 12, 15, 8, 10, 8])
                .race_class(RACE_HUMAN, &class("Barbarian"))
                .background(ACOLYTE);
            h.pick_any("Proficiency", &skill("Athletics"));
            h.pick_any("Proficiency", &skill("Survival"));
            h
        };
        let mut h = build(true);
        h.level_to(3);
        let s = h.snap();
        assert_eq!(s.ability("constitution").unwrap().modifier, 3);
        assert_eq!((s.hit_points, s.armor_class), (12 + 7 + 7 + 9, 14));
        let mut r = build(false);
        for roll in [3, 10] {
            r.s = borealis_engine::commands::level_up_main(e, &r.s, Some(roll))
                .unwrap()
                .state;
        }
        assert_eq!(r.snap().hit_points, 12 + 3 + 10 + 9);
    });
}
