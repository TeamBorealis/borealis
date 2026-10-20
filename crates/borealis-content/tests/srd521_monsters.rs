//! The bundled `srd-5.2.1-monsters` pack: schema, typed model, spot checks against the printed SRD 5.2.1 stat
//! blocks, the stat block count and the links to the SRD 5.1 monster pack.

use borealis_content::monster::{
    Ability, AttackKind, CreatureSize, CreatureType, DamageType, Initiative, LegendaryActions,
    Skill, SpellFrequency, Usage,
};
use borealis_content::schema::{validate_document, DocumentKind};
use borealis_content::{
    ContentStore, ContentStoreBuilder, MonsterData, PackFileKind, PackManifest, Ruleset,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const PACK_ID_PREFIX: &str = "ID_SRD521_MONSTER_";
/// Blocks whose printed Initiative score is not 10 + modifier in the SRD 5.2.1 text (see COVERAGE.md).
const PRINTED_INITIATIVE_TYPOS: [&str; 2] = ["Gray Ooze", "Invisible Stalker"];

fn content_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content")
        .join(name)
}

fn store() -> &'static ContentStore {
    static STORE: OnceLock<ContentStore> = OnceLock::new();
    STORE.get_or_init(|| {
        ContentStoreBuilder::new()
            .add_pack_dir(&content_dir("srd-5.2.1-monsters"))
            .unwrap()
            .build()
    })
}

fn monster(name: &str) -> MonsterData {
    let e = store()
        .by_type("Monster")
        .find(|s| s.element.name == name)
        .unwrap_or_else(|| panic!("{name} missing"))
        .element
        .clone();
    MonsterData::from_element(&e).unwrap()
}

fn all() -> Vec<(String, String, MonsterData)> {
    store()
        .by_type("Monster")
        .map(|s| {
            (
                s.element.id.clone(),
                s.element.name.clone(),
                MonsterData::from_element(&s.element).unwrap(),
            )
        })
        .collect()
}

fn scores(m: &MonsterData) -> [u8; 6] {
    let a = m.abilities;
    [a.str, a.dex, a.con, a.int, a.wis, a.cha]
}

#[test]
fn pack_and_every_file_are_schema_valid() {
    let dir = content_dir("srd-5.2.1-monsters");
    let text = fs::read_to_string(dir.join("pack.json")).unwrap();
    let manifest_json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(validate_document(DocumentKind::Pack, &manifest_json)
        .unwrap()
        .is_empty());
    let manifest = PackManifest::from_json_str(&text).unwrap();
    assert_eq!(manifest.license.as_deref(), Some("CC-BY-4.0"));
    let attribution = manifest.attribution.unwrap();
    assert!(attribution.contains("System Reference Document 5.2.1"));
    assert!(attribution.contains("https://creativecommons.org/licenses/by/4.0/legalcode"));
    assert_eq!(manifest.rulesets, vec![Ruleset::Dnd2024]);
    for f in &manifest.files {
        assert_eq!(f.kind, PackFileKind::Content);
        let doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(&f.path)).unwrap()).unwrap();
        let violations = validate_document(DocumentKind::Content, &doc).unwrap();
        assert!(
            violations.is_empty(),
            "{}: {:?}",
            f.path,
            violations.iter().take(5).collect::<Vec<_>>()
        );
    }
}

#[test]
fn pack_loads_with_zero_diagnostics() {
    let s = store();
    assert!(s.diagnostics().is_empty(), "{:#?}", s.diagnostics());
    let issues = s.validate();
    assert!(
        issues.is_empty(),
        "{:#?}",
        issues.iter().take(10).collect::<Vec<_>>()
    );
    assert_eq!(s.by_type("Monster").count(), 330);
    assert!(s.undefined_sources().is_empty());
}

#[test]
fn pack_loads_cleanly_next_to_the_other_srd_packs() {
    let s = ContentStoreBuilder::new()
        .add_pack_dir(&content_dir("srd-5.1-monsters"))
        .unwrap()
        .add_pack_dir(&content_dir("srd-5.2.1-monsters"))
        .unwrap()
        .build();
    assert!(s.diagnostics().is_empty(), "{:#?}", s.diagnostics());
    assert_eq!(s.by_type("Monster").count(), 317 + 330);
    // every maps_to target is a real SRD 5.1 monster
    let mut mapped = 0;
    for stored in s
        .by_type("Monster")
        .filter(|m| m.element.id.starts_with(PACK_ID_PREFIX))
    {
        if let Some(target) = stored
            .element
            .maps_to
            .as_ref()
            .and_then(|m| m.dnd2014.as_deref())
        {
            mapped += 1;
            assert!(s.contains_id(target), "{} -> {target}", stored.element.id);
        }
    }
    assert!(mapped > 300, "{mapped}");
}

/// The count equals the independent stat-block line counts recorded by the extraction (COVERAGE.md).
#[test]
fn count_matches_the_stat_block_line_counts() {
    let coverage =
        fs::read_to_string(content_dir("srd-5.2.1-monsters").join("COVERAGE.md")).unwrap();
    let cell = |label: &str| -> usize {
        let line = coverage
            .lines()
            .find(|l| l.contains(label))
            .unwrap_or_else(|| panic!("{label}"));
        line.rsplit('|').nth(1).unwrap().trim().parse().unwrap()
    };
    let in_store = store().by_type("Monster").count();
    assert_eq!(cell("`AC n Initiative` lines"), in_store);
    assert_eq!(cell("`CR` lines in the same pages"), in_store);
    assert_eq!(cell("Stat blocks in the pack"), in_store);
    assert_eq!(
        cell("Monsters A-Z chapter") + cell("Animals chapter"),
        in_store
    );
}

#[test]
fn every_monster_validates_and_round_trips() {
    for (id, name, m) in all() {
        assert!(id.starts_with(PACK_ID_PREFIX), "{id}");
        assert_eq!(m.ruleset, Ruleset::Dnd2024, "{id}");
        assert_eq!(m.validate(), vec![], "{id}");
        assert_eq!(m.warnings(), vec![], "{id}");
        assert!(m.initiative.is_some(), "{id} has no initiative");
        let e = m.to_element(&id, &name, "SRD 5.2.1 Monsters");
        assert_eq!(MonsterData::from_element(&e).unwrap(), m, "{id}");
    }
}

#[test]
fn soft_checks_hold_apart_from_the_printed_initiative_typos() {
    let mut flagged = BTreeSet::new();
    for (id, name, m) in all() {
        let issues = m.soft_checks();
        if !issues.is_empty() {
            assert!(
                issues.iter().all(|i| i.field == "initiative"),
                "{id}: {issues:?}"
            );
            flagged.insert(name);
        }
    }
    assert_eq!(
        flagged,
        PRINTED_INITIATIVE_TYPOS
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
    );
}

#[test]
fn spot_check_goblin_warrior() {
    let m = monster("Goblin Warrior");
    assert_eq!(m.size, vec![CreatureSize::Small]);
    assert_eq!(
        (m.creature_type, m.tags.as_slice()),
        (CreatureType::Fey, ["goblinoid".to_owned()].as_slice())
    );
    assert_eq!(m.alignment, "chaotic neutral");
    assert_eq!(m.ac[0].value, 15);
    assert_eq!(
        m.initiative,
        Some(Initiative {
            modifier: 2,
            score: 12
        })
    );
    assert_eq!((m.hp.average, m.hp.formula.as_deref()), (10, Some("3d6")));
    assert_eq!(m.speed.walk, Some(30));
    assert_eq!(scores(&m), [8, 15, 10, 10, 8, 8]);
    assert!(m.saves.is_empty());
    assert_eq!(m.skills, BTreeMap::from([(Skill::Stealth, 6)]));
    assert_eq!(
        m.gear,
        vec!["Leather Armor", "Scimitar", "Shield", "Shortbow"]
    );
    assert_eq!(
        (m.senses.darkvision, m.senses.passive_perception),
        (Some(60), 9)
    );
    assert_eq!(m.languages, vec!["Common", "Goblin"]);
    assert_eq!(
        (
            m.challenge_text().as_str(),
            m.xp.unwrap(),
            m.proficiency_bonus.unwrap()
        ),
        ("1/4", 50, 2)
    );
    let scimitar = m.actions[0].attack.as_ref().unwrap();
    assert_eq!(
        (scimitar.kind, scimitar.to_hit, scimitar.reach),
        (AttackKind::Melee, 4, Some(5))
    );
    assert_eq!(scimitar.damage.len(), 1);
    assert_eq!(scimitar.damage[0].dice.as_deref(), Some("1d6+2"));
    let bow = m.actions[1].attack.as_ref().unwrap();
    assert_eq!(
        (bow.kind, bow.range, bow.long_range),
        (AttackKind::Ranged, Some(80), Some(320))
    );
    assert_eq!(m.bonus_actions.len(), 1);
    assert_eq!(m.bonus_actions[0].name, "Nimble Escape");
}

#[test]
fn spot_check_owlbear() {
    let m = monster("Owlbear");
    assert_eq!(
        (m.size.clone(), m.creature_type),
        (vec![CreatureSize::Large], CreatureType::Monstrosity)
    );
    assert_eq!(m.alignment, "unaligned");
    assert_eq!(m.ac[0].value, 13);
    assert_eq!(
        m.initiative,
        Some(Initiative {
            modifier: 1,
            score: 11
        })
    );
    assert_eq!(
        (m.hp.average, m.hp.formula.as_deref()),
        (59, Some("7d10+21"))
    );
    assert_eq!((m.speed.walk, m.speed.climb), (Some(40), Some(40)));
    assert_eq!(scores(&m), [20, 12, 17, 3, 12, 7]);
    assert_eq!(m.skills, BTreeMap::from([(Skill::Perception, 5)]));
    assert_eq!(
        (m.senses.darkvision, m.senses.passive_perception),
        (Some(60), 15)
    );
    assert!(m.languages.is_empty());
    assert_eq!(
        (
            m.challenge_text().as_str(),
            m.xp.unwrap(),
            m.proficiency_bonus.unwrap()
        ),
        ("3", 700, 2)
    );
    assert_eq!(
        m.actions
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["Multiattack", "Rend"]
    );
    let rend = m.actions[1].attack.as_ref().unwrap();
    assert_eq!(
        (rend.to_hit, rend.reach, rend.damage[0].average),
        (7, Some(5), 14)
    );
    assert_eq!(rend.damage[0].dice.as_deref(), Some("2d8+5"));
}

#[test]
fn spot_check_adult_red_dragon() {
    let m = monster("Adult Red Dragon");
    assert_eq!(m.size, vec![CreatureSize::Huge]);
    assert_eq!(
        (m.creature_type, m.tags.as_slice()),
        (CreatureType::Dragon, ["chromatic".to_owned()].as_slice())
    );
    assert_eq!(m.alignment, "chaotic evil");
    assert_eq!(m.ac[0].value, 19);
    assert_eq!(
        m.initiative,
        Some(Initiative {
            modifier: 12,
            score: 22
        })
    );
    assert_eq!(
        (m.hp.average, m.hp.formula.as_deref()),
        (256, Some("19d12+133"))
    );
    assert_eq!(
        (m.speed.walk, m.speed.climb, m.speed.fly),
        (Some(40), Some(40), Some(80))
    );
    assert_eq!(scores(&m), [27, 10, 25, 16, 13, 23]);
    // the printed SAVE column: Str +8, Dex +6, Con +7, Int +3, Wis +7, Cha +6
    assert_eq!(
        m.saves,
        BTreeMap::from([(Ability::Dex, 6), (Ability::Wis, 7)])
    );
    assert_eq!(
        Ability::ALL
            .iter()
            .map(|a| m.save_bonus(*a))
            .collect::<Vec<_>>(),
        [8, 6, 7, 3, 7, 6]
    );
    assert_eq!(
        m.skills,
        BTreeMap::from([(Skill::Perception, 13), (Skill::Stealth, 6)])
    );
    assert_eq!(m.damage_immunities[0].types, vec![DamageType::Fire]);
    assert_eq!(
        (
            m.senses.blindsight,
            m.senses.darkvision,
            m.senses.passive_perception
        ),
        (Some(60), Some(120), 23)
    );
    assert_eq!(m.languages, vec!["Common", "Draconic"]);
    assert_eq!(
        (
            m.challenge_text().as_str(),
            m.xp,
            m.xp_lair,
            m.proficiency_bonus
        ),
        ("17", Some(18_000), Some(20_000), Some(6))
    );
    assert_eq!(m.traits[0].name, "Legendary Resistance");
    assert_eq!(
        m.traits[0].usage,
        Some(Usage::PerDayInLair { normal: 3, lair: 4 })
    );
    let breath = m.actions.iter().find(|a| a.name == "Fire Breath").unwrap();
    assert_eq!(breath.usage, Some(Usage::Recharge { min: 5, max: 6 }));
    let save = breath.save.as_ref().unwrap();
    assert_eq!(
        (save.ability, save.dc, save.half_on_success),
        (Ability::Dex, 21, true)
    );
    assert_eq!(
        (save.damage[0].average, save.damage[0].dice.as_deref()),
        (59, Some("17d6"))
    );
    let rend = m
        .actions
        .iter()
        .find(|a| a.name == "Rend")
        .unwrap()
        .attack
        .as_ref()
        .unwrap();
    assert_eq!(
        (rend.to_hit, rend.reach, rend.damage.len()),
        (14, Some(10), 2)
    );
    assert_eq!(rend.damage[1].damage_type, DamageType::Fire);
    let LegendaryActions {
        per_round,
        lair_uses,
        actions,
        ..
    } = m.legendary.clone().unwrap();
    assert_eq!((per_round, lair_uses), (Some(3), Some(4)));
    assert_eq!(
        actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["Commanding Presence", "Fiery Rays", "Pounce"]
    );
    let sc = m
        .actions
        .iter()
        .find(|a| a.name == "Spellcasting")
        .unwrap()
        .spellcasting
        .as_ref()
        .unwrap();
    assert_eq!(
        (sc.ability, sc.save_dc, sc.attack_bonus),
        (Some(Ability::Cha), Some(20), Some(12))
    );
    assert_eq!(sc.lists[0].frequency, SpellFrequency::AtWill);
    assert_eq!(
        sc.lists[1].frequency,
        SpellFrequency::PerDay {
            uses: 1,
            each: false
        }
    );
    assert_eq!(
        sc.lists[1].spells[0].id.as_deref(),
        Some("ID_2024_SPELL_FIREBALL")
    );
}

#[test]
fn spot_check_lich() {
    let m = monster("Lich");
    assert_eq!(
        (m.size.clone(), m.creature_type),
        (vec![CreatureSize::Medium], CreatureType::Undead)
    );
    assert_eq!(m.tags, vec!["wizard"]);
    assert_eq!(m.alignment, "neutral evil");
    assert_eq!(m.ac[0].value, 20);
    assert_eq!(
        m.initiative,
        Some(Initiative {
            modifier: 17,
            score: 27
        })
    );
    assert_eq!(
        (m.hp.average, m.hp.formula.as_deref()),
        (315, Some("42d8+126"))
    );
    assert_eq!(scores(&m), [11, 16, 16, 21, 14, 16]);
    assert_eq!(
        m.saves,
        BTreeMap::from([
            (Ability::Dex, 10),
            (Ability::Con, 10),
            (Ability::Int, 12),
            (Ability::Wis, 9)
        ])
    );
    assert_eq!(m.skills.len(), 4);
    assert_eq!(m.skills[&Skill::Arcana], 19);
    assert_eq!(
        m.damage_resistances[0].types,
        vec![DamageType::Cold, DamageType::Lightning]
    );
    assert_eq!(
        m.damage_immunities[0].types,
        vec![DamageType::Necrotic, DamageType::Poison]
    );
    assert_eq!(
        m.condition_immunities,
        vec![
            "charmed",
            "exhaustion",
            "frightened",
            "paralyzed",
            "poisoned"
        ]
    );
    assert_eq!(m.gear, vec!["Component Pouch"]);
    assert_eq!(
        (m.senses.truesight, m.senses.passive_perception),
        (Some(120), 19)
    );
    assert_eq!(m.languages, vec!["All"]);
    assert_eq!(
        (m.challenge_text().as_str(), m.xp.unwrap(), m.xp_lair),
        ("21", 33_000, Some(41_000))
    );
    assert_eq!(
        m.traits[0].usage,
        Some(Usage::PerDayInLair { normal: 4, lair: 5 })
    );
    let eldritch = m
        .actions
        .iter()
        .find(|a| a.name == "Eldritch Burst")
        .unwrap()
        .attack
        .as_ref()
        .unwrap();
    assert_eq!(
        (
            eldritch.kind,
            eldritch.to_hit,
            eldritch.reach,
            eldritch.range
        ),
        (AttackKind::MeleeOrRanged, 12, Some(5), Some(120))
    );
    let sc = m
        .actions
        .iter()
        .find(|a| a.name == "Spellcasting")
        .unwrap()
        .spellcasting
        .as_ref()
        .unwrap();
    assert_eq!(
        (sc.ability, sc.save_dc, sc.attack_bonus),
        (Some(Ability::Int), Some(20), None)
    );
    let names: Vec<_> = sc.lists.iter().map(|l| l.spells.len()).collect();
    assert_eq!(names, [8, 3, 4]);
    assert_eq!(
        sc.lists[2].frequency,
        SpellFrequency::PerDay {
            uses: 1,
            each: true
        }
    );
    assert_eq!(sc.lists[0].spells[3].name, "Fireball");
    assert_eq!(
        sc.lists[0].spells[3].note.as_deref(),
        Some("level 5 version")
    );
    assert_eq!(m.reactions[0].name, "Protective Magic");
    let leg = m.legendary.unwrap();
    assert_eq!(
        (leg.per_round, leg.lair_uses, leg.actions.len()),
        (Some(3), Some(4), 3)
    );
    let disrupt = leg
        .actions
        .iter()
        .find(|a| a.name == "Disrupt Life")
        .unwrap();
    assert_eq!(disrupt.save.as_ref().unwrap().dc, 20);
}

#[test]
fn spot_check_wolf_and_beasts_for_wild_shape() {
    let m = monster("Wolf");
    assert_eq!(
        (m.size.clone(), m.creature_type),
        (vec![CreatureSize::Medium], CreatureType::Beast)
    );
    assert_eq!(m.alignment, "unaligned");
    assert_eq!(m.ac[0].value, 12);
    assert_eq!(
        m.initiative,
        Some(Initiative {
            modifier: 2,
            score: 12
        })
    );
    assert_eq!((m.hp.average, m.hp.formula.as_deref()), (11, Some("2d8+2")));
    assert_eq!(m.speed.walk, Some(40));
    assert_eq!(scores(&m), [14, 15, 12, 3, 12, 6]);
    assert_eq!(
        m.skills,
        BTreeMap::from([(Skill::Perception, 5), (Skill::Stealth, 4)])
    );
    assert_eq!(
        (m.senses.darkvision, m.senses.passive_perception),
        (Some(60), 15)
    );
    assert!(m.languages.is_empty());
    assert_eq!((m.challenge_text().as_str(), m.xp), ("1/4", Some(50)));
    assert_eq!(m.traits[0].name, "Pack Tactics");
    let bite = m.actions[0].attack.as_ref().unwrap();
    assert_eq!(
        (bite.to_hit, bite.reach, bite.damage[0].dice.as_deref()),
        (4, Some(5), Some("1d6+2"))
    );
    // every Beast has a stat block usable as a Wild Shape form: no spellcasting, legendary actions or gear
    let beasts: Vec<_> = all()
        .into_iter()
        .filter(|(_, _, m)| m.creature_type == CreatureType::Beast)
        .collect();
    assert!(beasts.len() >= 70, "{}", beasts.len());
    for name in [
        "Wolf",
        "Brown Bear",
        "Giant Spider",
        "Dire Wolf",
        "Giant Wolf Spider",
        "Crocodile",
        "Riding Horse",
        "Swarm of Bats",
    ] {
        assert!(beasts.iter().any(|(_, n, _)| n == name), "{name}");
    }
    for (id, _, b) in &beasts {
        assert!(b.legendary.is_none() && b.gear.is_empty(), "{id}");
        assert!(
            b.traits
                .iter()
                .chain(&b.actions)
                .all(|f| f.spellcasting.is_none()),
            "{id}"
        );
        assert!(b.cr.unwrap().as_f64() <= 8.0, "{id}");
    }
}

#[test]
fn new_2024_features_are_structured() {
    let archmage = monster("Archmage");
    assert_eq!(
        (archmage.challenge_text().as_str(), archmage.xp.unwrap()),
        ("12", 8_400)
    );
    assert_eq!(
        archmage.size,
        vec![CreatureSize::Medium, CreatureSize::Small]
    );
    assert_eq!(
        archmage.condition_immunities,
        vec!["charmed (with mind blank)"]
    );
    let aboleth = monster("Aboleth");
    assert_eq!(
        (aboleth.speed.walk, aboleth.speed.swim),
        (Some(10), Some(40))
    );
    assert_eq!(aboleth.telepathy, Some(120));
    assert_eq!(aboleth.saves.get(&Ability::Con), Some(&6));
    let dominate = aboleth
        .actions
        .iter()
        .find(|a| a.name == "Dominate Mind")
        .unwrap();
    assert_eq!(dominate.usage, Some(Usage::PerDay(2)));
    assert_eq!(dominate.save.as_ref().unwrap().ability, Ability::Wis);
    let bats = monster("Swarm of Bats");
    assert_eq!(
        (bats.size, bats.swarm_of, bats.creature_type),
        (
            vec![CreatureSize::Large],
            Some(CreatureSize::Tiny),
            CreatureType::Beast
        )
    );
    let werebear = monster("Werebear");
    assert_eq!(werebear.speed.walk, Some(30));
    assert!(werebear
        .speed
        .note
        .as_deref()
        .unwrap()
        .contains("bear form only"));
    // the PDF lost the minus of this printed save; it is restored from the modifier
    let young = monster("Young White Dragon");
    assert_eq!(young.save_bonus(Ability::Int), -2);
    let spider = monster("Giant Wolf Spider");
    assert_eq!(spider.creature_type, CreatureType::Beast);
}

#[test]
fn renamed_creatures_map_to_their_srd51_counterparts() {
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&content_dir("srd-5.2.1-monsters"))
        .unwrap()
        .build();
    let el = |name: &str| {
        store
            .by_type("Monster")
            .find(|s| s.element.name == name)
            .unwrap()
            .element
            .clone()
    };
    let goblin = el("Goblin Warrior");
    assert_eq!(
        goblin.maps_to.unwrap().dnd2014.as_deref(),
        Some("ID_SRD51_MONSTER_GOBLIN")
    );
    assert_eq!(goblin.renamed_from.as_deref(), Some("Goblin"));
    let wolf = el("Wolf");
    assert_eq!(
        wolf.maps_to.unwrap().dnd2014.as_deref(),
        Some("ID_SRD51_MONSTER_WOLF")
    );
    assert!(wolf.renamed_from.is_none());
    assert!(el("Goblin Boss").maps_to.is_none());
}

#[test]
fn broken_2024_monsters_are_reported() {
    let good = monster("Adult Red Dragon");
    let mut bad = good.clone();
    bad.xp_lair = Some(10);
    bad.legendary.as_mut().unwrap().lair_uses = Some(1);
    bad.traits[0].usage = Some(Usage::PerDayInLair { normal: 4, lair: 3 });
    bad.gear = vec![" ".into()];
    let fields: Vec<String> = bad.validate().into_iter().map(|i| i.field).collect();
    for f in ["xp_lair", "legendary.lair_uses", "traits[0]", "gear"] {
        assert!(fields.iter().any(|x| x == f), "{f}: {fields:?}");
    }
    let mut value = serde_json::to_value(&good).unwrap();
    value["bogus"] = serde_json::json!(1);
    assert!(serde_json::from_value::<MonsterData>(value).is_err());
}
