use super::*;
use borealis_content::monster::{RestKind, Usage};
use serde_json::json;

/// The SRD 5.1 Wolf as 5etools writes a creature.
fn wolf() -> Value {
    json!({
        "name": "Wolf", "source": "MM", "srd": true,
        "size": ["M"], "type": "beast", "alignment": ["U"],
        "ac": [{"ac": 13, "from": ["{@item natural armor|mm}"]}],
        "hp": {"average": 11, "formula": "2d8 + 2"},
        "speed": {"walk": 40},
        "str": 12, "dex": 15, "con": 12, "int": 3, "wis": 12, "cha": 6,
        "skill": {"perception": "+3", "stealth": "+4"},
        "passive": 13, "cr": "1/4",
        "trait": [
            {"name": "Keen Hearing and Smell", "entries": ["The wolf has advantage on Wisdom ({@skill Perception}) checks that rely on hearing or smell."]},
            {"name": "Pack Tactics", "entries": ["The wolf has advantage on attack rolls against a creature if at least one of the wolf's allies is within 5 feet of the creature and the ally isn't {@condition incapacitated}."]}
        ],
        "action": [{"name": "Bite", "entries": ["{@atk mw} {@hit 4} to hit, reach 5 ft., one target. {@h}7 ({@damage 2d4 + 2}) piercing damage. If the target is a creature, it must succeed on a {@dc 11} Strength saving throw or be knocked {@condition prone}."]}]
    })
}

#[test]
fn a_creature_reads_into_the_srd_stat_block_shape() {
    let (m, lost) = stat_block(&wolf(), Ruleset::Dnd2014).unwrap();
    assert!(lost.is_empty(), "{lost:?}");
    assert_eq!(m.size, vec![CreatureSize::Medium]);
    assert_eq!(m.creature_type, CreatureType::Beast);
    assert_eq!(m.alignment, "unaligned");
    assert_eq!(
        m.ac,
        vec![ArmorClass {
            value: 13,
            source: Some("natural armor".into()),
            condition: None
        }]
    );
    assert_eq!(
        m.hp,
        Some(HitPoints {
            average: 11,
            formula: Some("2d8+2".into())
        })
    );
    assert_eq!(m.speed.walk, Some(40));
    assert_eq!((m.abilities.dex, m.abilities.cha), (15, 6));
    assert_eq!(m.skills.get(&Skill::Perception), Some(&3));
    assert_eq!(m.senses.passive_perception, 13);
    assert_eq!(m.challenge_text(), "1/4");
    assert_eq!(
        (m.xp, m.proficiency_bonus, m.cr_note.as_deref()),
        (Some(50), Some(2), None)
    );
    assert!(m.initiative.is_none());
    let names: Vec<&str> = m.traits.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Keen Hearing and Smell", "Pack Tactics"]);
    assert!(m.actions[0].text.starts_with(
        "Melee Weapon Attack: +4 to hit, reach 5 ft., one target. Hit: 7 (2d4 + 2) piercing"
    ));
    // the element is the bundled packs' Monster element, and the engine reads it back
    let el = m.to_element("ID_X", "Wolf", "Monster Manual");
    assert_eq!(el.ty, "Monster");
    assert_eq!(MonsterData::from_element(&el).unwrap(), m);
}

#[test]
fn usage_legendary_tags_swarms_and_defences_are_read() {
    let raw = json!({
        "name": "Test Drake", "source": "X", "isNamedCreature": false,
        "size": ["H"], "type": {"type": "dragon", "tags": ["chromatic", {"tag": "shapechanger", "prefix": "any"}]},
        "alignment": ["L", "NX", "C", "E"],
        "ac": [19, {"ac": 21, "condition": "with {@spell shield}", "braces": true}],
        "hp": {"average": 200, "formula": "16d12 + 96"},
        "speed": {"walk": 40, "fly": {"number": 80, "condition": "(hover)"}},
        "initiative": {"proficiency": 2},
        "str": 23, "dex": 10, "con": 21, "int": 14, "wis": 11, "cha": 19,
        "save": {"dex": "+5", "wis": "+5"},
        "senses": ["blindsight 60 ft.", "darkvision 120 ft."], "passive": 21,
        "resist": ["cold", {"resist": ["bludgeoning", "piercing"], "note": "from nonmagical attacks", "cond": true}],
        "immune": ["fire"],
        "conditionImmune": ["frightened", {"conditionImmune": ["charmed"], "note": "while awake"}],
        "languages": ["Common", "Draconic", "telepathy 60 ft."],
        "cr": {"cr": "15", "xpLair": 15000},
        "trait": [{"name": "Legendary Resistance (3/Day, or 4/Day in Lair)", "entries": ["It succeeds instead."]}],
        "action": [{"name": "Fire Breath {@recharge 5}", "entries": ["{@actSave dex} {@dc 19}. {@actSaveFail} 63 ({@damage 18d6}) Fire damage."]}],
        "reaction": [{"name": "Tail Flick (Recharges after a Short or Long Rest)", "entries": ["It flicks."]}],
        "legendary": [{"name": "Wing Attack (Costs 2 Actions)", "entries": ["It beats its wings."]}],
        "legendaryActionsLair": 4,
        "spellcasting": [{"name": "Spellcasting", "displayAs": "action", "headerEntries": ["It casts:"], "will": ["{@spell light}"]}],
        "mythic": [{"name": "M", "entries": ["m"]}]
    });
    let (m, lost) = stat_block(&raw, Ruleset::Dnd2024).unwrap();
    assert_eq!(m.tags, ["chromatic", "any shapechanger"]);
    assert_eq!(m.alignment, "any evil alignment");
    assert_eq!(m.ac[1].condition.as_deref(), Some("with shield"));
    assert!(m.speed.hover && m.speed.fly == Some(80) && m.speed.note.is_none());
    assert_eq!(
        m.initiative,
        Some(Initiative {
            modifier: 10,
            score: 20
        })
    );
    assert_eq!(m.saves.get(&Ability::Dex), Some(&5));
    assert_eq!(m.damage_resistances.len(), 2);
    assert_eq!(
        m.damage_resistances[1].types,
        [DamageType::Bludgeoning, DamageType::Piercing]
    );
    assert_eq!(
        m.damage_resistances[1].text.as_deref(),
        Some("from nonmagical attacks")
    );
    assert_eq!(
        m.condition_immunities,
        ["frightened", "charmed (while awake)"]
    );
    assert_eq!(m.telepathy, Some(60));
    assert_eq!(
        (m.xp, m.xp_lair, m.proficiency_bonus),
        (Some(13_000), Some(15_000), Some(5))
    );
    assert_eq!(
        m.traits[0].usage,
        Some(Usage::PerDayInLair { normal: 3, lair: 4 })
    );
    assert_eq!(m.traits[0].name, "Legendary Resistance");
    assert_eq!(m.actions[0].usage, Some(Usage::Recharge { min: 5, max: 6 }));
    assert!(
        m.actions[0]
            .text
            .starts_with("Dexterity Saving Throw: DC 19. Failure: 63"),
        "{}",
        m.actions[0].text
    );
    assert_eq!(m.actions[1].name, "Spellcasting");
    assert_eq!(
        m.reactions[0].usage,
        Some(Usage::OnRest(RestKind::ShortOrLong))
    );
    let leg = m.legendary.as_ref().unwrap();
    assert_eq!(
        (leg.per_round, leg.lair_uses, leg.actions[0].cost),
        (Some(3), Some(4), Some(2))
    );
    assert!(leg
        .preamble
        .starts_with("Legendary Action Uses: 3 (4 in Lair)."));
    assert_eq!(lost, ["creature section: mythic actions not imported"]);
    let swarm = json!({"name": "Swarm", "source": "X", "size": ["M"],
        "type": {"type": "beast", "swarmSize": "T"}, "alignment": ["U"], "ac": [12],
        "hp": {"average": 22, "formula": "5d8"}, "speed": {"walk": 0, "fly": 60},
        "str": 5, "dex": 15, "con": 10, "int": 2, "wis": 12, "cha": 4, "passive": 11, "cr": "1/4"});
    assert_eq!(
        stat_block(&swarm, Ruleset::Dnd2014).unwrap().0.swarm_of,
        Some(CreatureSize::Tiny)
    );
}

#[test]
fn creatures_that_cannot_be_read_exactly_say_why() {
    let with = |k: &str, v: Value| {
        let mut w = wolf();
        w[k] = v;
        stat_block(&w, Ruleset::Dnd2014).unwrap_err()
    };
    assert!(with("hp", json!({"special": "half the summoner's"}))
        .starts_with("hit points are not a number"));
    assert!(with("cr", json!("Unknown")).contains("is not a number"));
    assert!(
        with("type", json!({"type": {"choose": ["celestial", "fiend"]}}))
            .starts_with("the creature type is a choice")
    );
    assert!(with("str", Value::Null).contains("str score"));
    assert_eq!(
        with("ac", json!([{"special": "varies"}])),
        "armor class is not a number"
    );
    let mut odd = wolf();
    odd["hp"]["formula"] = json!("2d8 + 2 + your level");
    let (m, lost) = stat_block(&odd, Ruleset::Dnd2014).unwrap();
    assert_eq!(m.hp.as_ref().unwrap().formula, None);
    assert_eq!(lost.len(), 1);
}

/// A synthetic summoned spirit as 5etools prints one: no `cr`, numbers that follow the spell.
fn ember_spirit() -> Value {
    json!({
        "name": "Synthetic Ember Spirit", "source": "SYN",
        "size": ["S"], "type": "elemental", "alignment": ["N"],
        "ac": [12], "hp": {"average": 20, "formula": "4d6 + 6"},
        "speed": {"walk": 30, "fly": 30},
        "str": 8, "dex": 14, "con": 12, "int": 6, "wis": 10, "cha": 8,
        "passive": 10,
        "summonedBySpell": "Synthetic Kindle Spirit|SYN", "summonedBySpellLevel": 3,
        "pbNote": "equals your bonus",
        "action": [{"name": "Sear", "entries": ["{@atk ms} {@hit 5} to hit, reach 5 ft., one target. {@h}5 ({@damage 1d6 + 2}) fire damage."]}]
    })
}

#[test]
fn a_creature_without_a_challenge_rating_imports_without_cr_or_xp() {
    let (m, _) = stat_block(&ember_spirit(), Ruleset::Dnd2014).unwrap();
    assert_eq!(
        (m.cr, m.xp, m.xp_lair, m.proficiency_bonus),
        (None, None, None, None)
    );
    assert_eq!(
        m.cr_note.as_deref(),
        Some("Scales with spell level (Synthetic Kindle Spirit)")
    );
    assert_eq!(
        m.challenge_text(),
        "Scales with spell level (Synthetic Kindle Spirit)"
    );
    assert!(m.validate().is_empty(), "{:?}", m.validate());
    assert!(m.warnings().is_empty());
}

#[test]
fn the_note_for_a_missing_challenge_rating_follows_what_5etools_prints() {
    let note = |edit: &dyn Fn(&mut Value)| {
        let mut c = ember_spirit();
        for k in ["summonedBySpell", "summonedBySpellLevel"] {
            c.as_object_mut().unwrap().remove(k);
        }
        edit(&mut c);
        stat_block(&c, Ruleset::Dnd2014).unwrap().0.challenge_text()
    };
    assert_eq!(
        note(&|c| c["summonedByClass"] = json!("Synthetic Warden|SYN")),
        "Scales with the summoner's level (Synthetic Warden)"
    );
    assert_eq!(
        note(&|c| c["summonedBySpellLevel"] = json!(2)),
        "Scales with spell level"
    );
    assert_eq!(note(&|c| c["level"] = json!(7)), "Level 7");
    assert_eq!(note(&|_| {}), borealis_content::NO_CHALLENGE_RATING);
    assert_eq!(note(&|c| c["cr"] = Value::Null), "\u{2014}");
}

#[test]
fn a_summon_keeps_a_formula_armor_class_and_hit_points_as_text() {
    let mut c = ember_spirit();
    c["ac"] = json!([{"special": "9 + the {@b kindling} level"}]);
    c["hp"] = json!({"special": "15 + 5 for each kindling level"});
    let (m, lost) = stat_block(&c, Ruleset::Dnd2014).unwrap();
    assert!(m.ac.is_empty() && m.hp.is_none());
    assert_eq!(m.ac_note.as_deref(), Some("9 + the kindling level"));
    assert_eq!(m.hp_note.as_deref(), Some("15 + 5 for each kindling level"));
    assert_eq!(
        (m.armor_class_text(), m.hit_points_text()),
        (
            "9 + the kindling level".to_string(),
            "15 + 5 for each kindling level".to_string()
        )
    );
    assert!(lost.is_empty(), "{lost:?}");
    assert!(m.validate().is_empty(), "{:?}", m.validate());

    // only one of them a formula: the other stays a number
    let mut hp_only = ember_spirit();
    hp_only["hp"] = json!({"special": "4 per kindling level"});
    let m = stat_block(&hp_only, Ruleset::Dnd2014).unwrap().0;
    assert_eq!((m.armor_class(), m.ac_note), (Some(12), None));
    assert_eq!(m.hp_note.as_deref(), Some("4 per kindling level"));
}

#[test]
fn printed_text_where_a_number_belongs_is_still_a_skip() {
    // a creature with a challenge rating needs its numbers
    let mut rated = ember_spirit();
    rated["cr"] = json!("1");
    rated["hp"] = json!({"special": "4 per kindling level"});
    assert!(stat_block(&rated, Ruleset::Dnd2014)
        .unwrap_err()
        .starts_with("hit points are not a number"));
    rated["hp"] = json!({"average": 20});
    rated["ac"] = json!([{"special": "9 + the kindling level"}]);
    assert_eq!(
        stat_block(&rated, Ruleset::Dnd2014).unwrap_err(),
        "armor class is not a number"
    );
    // without one: an armor class entry that is neither a number nor text, an ability score
    let mut summon = ember_spirit();
    summon["ac"] = json!([{"from": ["natural armor"]}]);
    assert_eq!(
        stat_block(&summon, Ruleset::Dnd2014).unwrap_err(),
        "armor class is not a number"
    );
    let mut summon = ember_spirit();
    summon["hp"] = json!({"special": "  "});
    assert!(stat_block(&summon, Ruleset::Dnd2014)
        .unwrap_err()
        .starts_with("hit points are not a number"));
    let mut summon = ember_spirit();
    summon["str"] = json!("as the kindling");
    assert!(stat_block(&summon, Ruleset::Dnd2014)
        .unwrap_err()
        .contains("str score"));
}

#[test]
fn a_formula_beside_a_number_keeps_the_number() {
    let mut c = ember_spirit();
    c["ac"] = json!([12, {"special": "14 while kindled"}]);
    let (m, lost) = stat_block(&c, Ruleset::Dnd2014).unwrap();
    assert_eq!((m.armor_class(), m.ac_note), (Some(12), None));
    assert_eq!(lost.len(), 1, "{lost:?}");
}

#[test]
fn a_2024_creature_without_a_challenge_rating_has_no_proficient_initiative() {
    let mut c = ember_spirit();
    c["initiative"] = json!({"proficiency": 1});
    assert_eq!(stat_block(&c, Ruleset::Dnd2024).unwrap().0.initiative, None);
    c["initiative"] = json!({"proficiency": 0});
    let m = stat_block(&c, Ruleset::Dnd2024).unwrap().0;
    assert_eq!(m.initiative.map(|i| i.modifier), Some(2));
}

fn ent(raw: Value, source: &str, ruleset: Ruleset) -> Ent {
    Ent {
        name: raw["name"].as_str().unwrap().to_string(),
        raw,
        file: "bestiary/bestiary-x.json".into(),
        source: source.into(),
        ruleset,
        is_srd: true,
    }
}

/// Convert `raw` as a creature of `source` with an SRD index holding a 2014 and a 2024 Wolf.
fn converted(raw: Value, source: &str, ruleset: Ruleset) -> borealis_content::Element {
    use crate::srd::SrdKind;
    let mut sh = Shared::default();
    sh.srd
        .insert(SrdKind::Monster, Ruleset::Dnd2014, "Wolf", "ID_SRD51_WOLF");
    sh.srd
        .insert(SrdKind::Monster, Ruleset::Dnd2024, "Wolf", "ID_SRD521_WOLF");
    sh.srd
        .insert(SrdKind::Spell, Ruleset::Dnd2014, "Pack Tactics", "ID_X");
    let mut st = State::default();
    let mut conv = convert(&sh, &mut st, &ent(raw, source, ruleset));
    assert_eq!(conv.elements.len(), 1);
    conv.elements.remove(0)
}

#[test]
fn a_monster_manual_creature_is_the_srd_creature_of_its_name_and_edition() {
    use borealis_content::MapKind;
    let mm = converted(wolf(), "MM", Ruleset::Dnd2014);
    let maps = mm.maps_to.expect("MM Wolf maps to the SRD 5.1 Wolf");
    assert_eq!(
        (maps.dnd2014.as_deref(), maps.dnd2024.as_deref(), maps.kind),
        (Some("ID_SRD51_WOLF"), None, Some(MapKind::Equivalent))
    );
    let mut xwolf = wolf();
    xwolf["source"] = json!("XMM");
    let xmm = converted(xwolf, "xmm", Ruleset::Dnd2024);
    let maps = xmm.maps_to.expect("XMM Wolf maps to the SRD 5.2.1 Wolf");
    assert_eq!(
        (maps.dnd2014.as_deref(), maps.dnd2024.as_deref(), maps.kind),
        (None, Some("ID_SRD521_WOLF"), Some(MapKind::Equivalent))
    );
}

#[test]
fn other_books_and_other_names_get_no_srd_link() {
    let mut adventure = wolf();
    adventure["source"] = json!("ZZZ");
    assert!(converted(adventure, "ZZZ", Ruleset::Dnd2014)
        .maps_to
        .is_none());
    let mut renamed = wolf();
    renamed["name"] = json!("Dire Wolf");
    assert!(converted(renamed, "MM", Ruleset::Dnd2014).maps_to.is_none());
    // a name that is an SRD element of another kind is no creature
    let mut spellish = wolf();
    spellish["name"] = json!("Pack Tactics");
    assert!(converted(spellish, "MM", Ruleset::Dnd2014)
        .maps_to
        .is_none());
}
