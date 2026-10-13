//! Golden vectors TV-07..TV-10, TV-19, TV-20 (multiclassing and cascades).

mod common;
use borealis_engine::commands;
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{Choice, ClassSlot};
use common::*;

fn mc(name: &str) -> String {
    format!("ID_WOTC_PHB_MULTICLASS_{}", name.to_uppercase())
}

fn sk(s: &CharacterSnapshot, n: &str) -> i32 {
    s.skill(n).unwrap().total
}

fn saves(s: &CharacterSnapshot) -> Vec<i32> {
    s.abilities.iter().map(|a| a.save).collect()
}

/// Take a new multiclass level and choose the class; returns the starting level.
fn start_multiclass(h: &mut Hero<'_>, name: &str) -> u32 {
    h.s = commands::level_up_new_multiclass(h.e, &h.s, None)
        .expect("new multiclass level")
        .state;
    let start = h.s.level();
    h.pick(
        &format!("ID_LEVEL_{start}"),
        &format!("Multiclass (Level {start})"),
        1,
        &mc(name),
    );
    start
}

fn more_levels(h: &mut Hero<'_>, start: u32, n: u32) {
    for _ in 0..n {
        h.s = commands::level_up_multiclass(h.e, &h.s, start, None)
            .expect("multiclass level")
            .state;
    }
}

#[test]
fn tv07_fighter_5_wizard_3() {
    each_variant("tv07", |e, v| {
        let mut h = Hero::new(e, true);
        h.base([15, 12, 14, 12, 10, 8])
            .race_class(RACE_HUMAN, &class("Fighter"))
            .background(ACOLYTE);
        h.pick_any(
            "Class Feature",
            "ID_WOTC_PHB_CLASS_FEATURE_FIGHTINGSTYLE_DEFENSE",
        );
        h.pick_any("Proficiency", &skill("Athletics"));
        h.pick_any("Proficiency", &skill("Perception"));
        h.level_to(3);
        h.pick_any("Archetype", "ID_WOTC_PHB_ARCHETYPE_CHAMPION");
        h.level_to(5);
        let start = start_multiclass(&mut h, "Wizard");
        assert_eq!(start, 6);
        more_levels(&mut h, start, 2);
        h.equip("ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL");
        h.equip("ID_WOTC_GEAR_SHIELD");
        let s = h.snap();
        assert_eq!(
            (s.level, s.experience, s.proficiency_bonus),
            (8, 34_000, 3),
            "{v:?}"
        );
        assert_eq!((s.hit_points, s.armor_class), (62, 19));
        assert_eq!(saves(&s), [6, 1, 5, 1, 0, -1]);
        assert_eq!(
            (
                sk(&s, "athletics"),
                sk(&s, "perception"),
                sk(&s, "insight"),
                sk(&s, "religion")
            ),
            (6, 3, 3, 4)
        );
        let w = s.caster("Wizard").expect("wizard casting");
        assert_eq!(
            (w.dc, w.attack, w.slots[0], w.slots[1], w.prepare_limit),
            (12, 4, 4, 2, Some(4))
        );
        assert!(
            s.multiclass_slots.is_none(),
            "one caster class: no pooled table"
        );
        assert_eq!(s.stat("extra attack:count"), 2);
        assert_eq!(s.classes.len(), 2);
        assert_eq!((s.classes[0].level, s.classes[1].level), (5, 3));
    });
}

#[test]
fn tv08_cleric_3_wizard_2() {
    each_variant("tv08", |e, _| {
        let mut h2 = Hero::new(e, true);
        h2.base([10, 12, 14, 13, 15, 8])
            .race_class(RACE_HUMAN, &class("Cleric"))
            .pick_any("Archetype", "ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN");
        h2.level_to(3);
        let start = start_multiclass(&mut h2, "Wizard");
        more_levels(&mut h2, start, 1);
        let s = h2.snap();
        assert_eq!(
            (s.proficiency_bonus, s.armor_class, s.hit_points),
            (3, 11, 36)
        );
        assert_eq!((saves(&s)[4], saves(&s)[5]), (6, 2));
        let (c, w) = (s.caster("Cleric").unwrap(), s.caster("Wizard").unwrap());
        assert_eq!(
            (c.dc, c.attack, c.slots[0], c.slots[1], c.prepare_limit),
            (14, 6, 4, 2, Some(6))
        );
        assert_eq!(
            (w.dc, w.attack, w.slots[0], w.prepare_limit),
            (13, 5, 3, Some(4))
        );
        let pool = s
            .multiclass_slots
            .expect("two full casters pool their slots");
        assert_eq!((pool.caster_level, &pool.slots[..3]), (5, &[4, 3, 2][..]));
    });
}

#[test]
fn tv09_paladin_4_sorcerer_3() {
    each_variant("tv09", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([15, 8, 14, 10, 12, 13])
            .race_class(RACE_HUMAN, &class("Paladin"));
        h.pick_any("Proficiency", &skill("Athletics"));
        h.pick_any("Proficiency", &skill("Religion"));
        h.level_to(4);
        let start = start_multiclass(&mut h, "Sorcerer");
        more_levels(&mut h, start, 2);
        let s = h.snap();
        assert_eq!(
            (s.proficiency_bonus, s.armor_class, s.hit_points),
            (3, 9, 54)
        );
        assert_eq!(
            (
                saves(&s)[4],
                saves(&s)[5],
                sk(&s, "athletics"),
                sk(&s, "religion")
            ),
            (4, 5, 6, 3)
        );
        let (p, so) = (s.caster("Paladin").unwrap(), s.caster("Sorcerer").unwrap());
        assert_eq!(
            (p.dc, p.attack, p.slots[0], p.prepare_limit),
            (13, 5, 3, Some(4))
        );
        assert_eq!((so.dc, so.attack, so.slots[0], so.slots[1]), (13, 5, 4, 2));
        let pool = s.multiclass_slots.unwrap();
        assert_eq!((pool.caster_level, &pool.slots[..3]), (5, &[4, 3, 2][..]));
    });
}

#[test]
fn tv10_warlock_3_wizard_3_pact_magic_is_separate() {
    each_variant("tv10", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([8, 14, 13, 15, 12, 13])
            .race_class(RACE_HUMAN, &class("Warlock"));
        h.pick_any("Proficiency", &skill("Arcana"));
        h.pick_any("Proficiency", &skill("Deception"));
        h.level_to(3);
        let start = start_multiclass(&mut h, "Wizard");
        more_levels(&mut h, start, 2);
        let s = h.snap();
        assert_eq!(
            (s.proficiency_bonus, s.armor_class, s.hit_points),
            (3, 12, 42)
        );
        let (wl, wz) = (s.caster("Warlock").unwrap(), s.caster("Wizard").unwrap());
        assert_eq!(
            (wl.dc, wl.attack, wl.slots, wl.pact),
            (13, 5, [0; 9], Some((2, 2))),
            "pact slots are a separate pool, never ordinary slots"
        );
        assert_eq!(
            (wz.dc, wz.attack, wz.slots[0], wz.slots[1], wz.prepare_limit),
            (14, 6, 4, 2, Some(6))
        );
        assert!(
            s.multiclass_slots.is_none(),
            "pact magic is not a slot source"
        );
    });
}

#[test]
fn tv19_multiclass_gating_chain() {
    each_variant("tv19", |e, _| {
        // Case A: STR 12 / DEX 12 final -> main-class prerequisite fails
        let mut a = Hero::new(e, true);
        a.base([11, 11, 14, 11, 10, 8])
            .race_class(RACE_HUMAN, &class("Fighter"));
        a.level_to(2);
        assert!(!a.snap().can_multiclass);
        assert!(commands::level_up_new_multiclass(e, &a.s, None).is_err());
        a.s.levels.push(ClassSlot::Multi { starting_level: 3 });
        let s = a.snap();
        assert!(
            s.slot("ID_LEVEL_3", "Multiclass (Level 3)").is_none(),
            "slot must not open"
        );
        // Case B: DEX 13 final
        let mut b = Hero::new(e, true);
        b.base([11, 12, 14, 11, 10, 8])
            .race_class(RACE_HUMAN, &class("Fighter"));
        b.level_to(2);
        assert!(b.snap().can_multiclass);
        b.s = commands::level_up_new_multiclass(e, &b.s, None)
            .unwrap()
            .state;
        let s = b.snap();
        let slot = s
            .slot("ID_LEVEL_3", "Multiclass (Level 3)")
            .expect("slot opens");
        let offered: Vec<&str> = slot.options.iter().map(|o| o.id.as_str()).collect();
        assert!(
            offered.contains(&mc("Rogue").as_str()),
            "Rogue needs DEX 13: {offered:?}"
        );
        for no in ["Wizard", "Cleric", "Fighter"] {
            assert!(
                !offered.contains(&mc(no).as_str()),
                "{no} must not be offered"
            );
        }
    });
}

#[test]
fn tv20_cascade_removal() {
    each_variant("tv20", |e, _| {
        let mut h = Hero::new(e, true);
        h.base([10, 10, 12, 8, 15, 12])
            .race_class(RACE_HUMAN, &class("Cleric"))
            .background(ACOLYTE);
        h.pick_any("Archetype", "ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN");
        h.pick_any("Proficiency", &skill("History"));
        h.pick_any("Proficiency", &skill("Medicine"));
        let before = h.snap();
        assert!(
            before.has_element("ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN")
                && before.skill("medicine").unwrap().proficient
        );
        // 1. clearing the class slot removes everything it caused
        let key = commands::slot_key(&before, "ID_LEVEL_1", "Class", 1).unwrap();
        h.s = commands::clear(e, &h.s, &key).unwrap().state;
        let after = h.snap();
        assert!(!after.has_element("ID_WOTC_PHB_ARCHETYPE_LIFEDOMAIN"));
        assert!(!after.skill("medicine").unwrap().proficient);
        assert!(
            after.skill("insight").unwrap().proficient,
            "background grants stay"
        );
        assert!(
            h.s.selections.iter().all(|s| s.slot.owner == "ID_LEVEL_1"),
            "orphan selections pruned: {:?}",
            h.s.selections
        );
        // 2. a feat that loses its ability prerequisite is rejected but stays stored
        let mut f = Hero::new(e, true);
        f.base([13, 12, 14, 8, 10, 8])
            .race_class(RACE_HUMAN, &class("Fighter"));
        f.level_to(4);
        f.pick(
            "ID_WOTC_PHB_CLASS_FEATURE_ABILITYSCOREIMPROVEMENT_FIGHTER",
            "Improvement Option (Fighter 4)",
            1,
            "ID_INTERNAL_CLASS_FEATURE_FEAT_4_FIGHTER",
        );
        f.pick_any("Feat", "ID_PHB_FEAT_GRAPPLER");
        assert!(f.snap().has_element("ID_PHB_FEAT_GRAPPLER"));
        let mut low = f.s.clone();
        low.abilities.base[0] = 8;
        let s = e.derive(&low);
        assert!(!s.has_element("ID_PHB_FEAT_GRAPPLER"));
        assert_eq!(s.invalid_selections.len(), 1);
        assert_eq!(
            s.invalid_selections[0].choice,
            Choice::Element("ID_PHB_FEAT_GRAPPLER".into())
        );
        let back = e.derive(&f.s);
        assert!(back.has_element("ID_PHB_FEAT_GRAPPLER"));
        // 3. removing the Feats option removes the feat alternative and its choice
        f.s = commands::set_option(e, &f.s, "ID_INTERNAL_OPTION_ALLOW_FEATS", false)
            .unwrap()
            .state;
        let s = f.snap();
        assert!(
            !s.has_element("ID_PHB_FEAT_GRAPPLER")
                && !s.has_element("ID_INTERNAL_CLASS_FEATURE_FEAT_4_FIGHTER")
        );
        assert!(f
            .s
            .selections
            .iter()
            .all(|x| x.slot.owner != "ID_INTERNAL_CLASS_FEATURE_FEAT_4_FIGHTER"));
    });
}
