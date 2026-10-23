//! Tests of the typed sheet model against real engine snapshots (SRD 5.1 and SRD 5.2.1).

use crate::model_fixtures::{
    cleric_2014, cleric_wizard_2014, fighter_wizard_2014, wizard_2014, wizard_2024,
};
use crate::model_ids::{self as id, FieldKind};
use crate::model_types::{
    AttackKind, Edition, EditionFields, FeatureGroup, SheetData, SpellFacts, SpellFactsMap,
};
use std::collections::HashSet;

type Pair = (
    borealis_character::Character,
    borealis_engine::CharacterSnapshot,
);

fn data(pair: &Pair) -> SheetData {
    crate::sheet_data(&pair.0, &pair.1, &SpellFactsMap::new())
}

fn all() -> Vec<SheetData> {
    [
        cleric_2014(),
        wizard_2014(),
        wizard_2024(),
        cleric_wizard_2014(),
        fighter_wizard_2014(),
    ]
    .iter()
    .map(data)
    .collect()
}

fn ids(d: &SheetData) -> Vec<String> {
    d.fields().into_iter().map(|f| f.id).collect()
}

#[test]
fn wizard_2014_has_the_common_core_and_the_2014_set() {
    let d = data(&wizard_2014());
    assert_eq!(d.edition, Edition::Dnd2014);
    assert_eq!(d.identity.class_label, "Wizard 5");
    assert_eq!(d.identity.level, 5);
    let keys: Vec<_> = d.abilities.iter().map(|a| a.key).collect();
    assert_eq!(keys, ["str", "dex", "con", "int", "wis", "cha"]);
    assert_eq!(d.skills.len(), 18);
    assert!(d.skills.iter().any(|s| s.key == "sleight_of_hand"));
    assert_eq!(d.combat.hit_dice.len(), 1);
    assert_eq!(
        (d.combat.hit_dice[0].die, d.combat.hit_dice[0].count),
        (6, 5)
    );
    assert!(matches!(&d.edition_fields, EditionFields::Y2014(f) if f.race == "Human"));
    assert_eq!(d.proficiency_bonus, 3);
    assert!(d.passives.iter().any(|p| p.key == "perception"));
    let all_ids = ids(&d);
    assert!(all_ids.contains(&id::INSPIRATION.to_string()));
    assert!(!all_ids.contains(&id::HEROIC_INSPIRATION.to_string()));
}

#[test]
fn wizard_2024_has_the_2024_set() {
    let d = data(&wizard_2024());
    assert_eq!(d.edition, Edition::Dnd2024);
    let EditionFields::Y2024(f) = &d.edition_fields else {
        panic!("2024 fields expected");
    };
    assert_eq!(f.species, "Human");
    assert_eq!(f.origin_feat.as_deref(), Some("Magic Initiate"));
    assert!(d.play.exhaustion.linear);
    let all_ids = ids(&d);
    for want in id::FIXED_2024 {
        assert!(all_ids.iter().any(|i| i == want), "missing {want}");
    }
    for banned in id::FIXED_2014 {
        assert!(
            !all_ids.iter().any(|i| i == banned),
            "2014 id {banned} on a 2024 sheet"
        );
    }
    assert!(d
        .features
        .iter()
        .any(|f| f.group == FeatureGroup::Species && f.name == "Skillful"));
}

#[test]
fn fighter_wizard_multiclass_casts_per_class_without_a_pool() {
    let d = data(&fighter_wizard_2014());
    assert_eq!(d.identity.class_label, "Fighter 4 / Wizard 3");
    let dice: Vec<_> = d.combat.hit_dice.iter().map(|h| (h.die, h.count)).collect();
    assert_eq!(dice, [(10, 4), (6, 3)]);
    assert_eq!(d.spellcasting.casters.len(), 1);
    let w = &d.spellcasting.casters[0];
    assert_eq!(w.class, "Wizard");
    assert_eq!(w.slots, [4, 2, 0, 0, 0, 0, 0, 0, 0]);
    assert!(!w.slots_pooled && d.spellcasting.pooled.is_none());
    assert_eq!(
        d.proficiencies.weapons,
        ["Simple Weapons", "Martial Weapons"]
    );
    assert!(d.proficiencies.armor.contains(&"Heavy Armor".to_string()));
}

#[test]
fn caster_multiclass_has_each_class_casting_block_and_pooled_slots() {
    let d = data(&cleric_wizard_2014());
    let [cleric, wizard] = &d.spellcasting.casters[..] else {
        panic!("two casters expected");
    };
    assert_eq!((cleric.ability, wizard.ability), ("wis", "int"));
    assert_ne!(cleric.dc, wizard.dc, "each class has its own DC");
    assert!(cleric.slots_pooled && wizard.slots_pooled);
    let pool = d.spellcasting.pooled.expect("pooled table");
    assert_eq!(
        (pool.caster_level, pool.slots),
        (5, [4, 3, 2, 0, 0, 0, 0, 0, 0])
    );
    let all_ids = ids(&d);
    for want in [
        id::spell_head(1, "dc"),
        id::spell_head(2, "dc"),
        id::pool_slot_total(3),
        id::pool_slot_used(1, 4),
        id::POOL_LEVEL.to_string(),
    ] {
        assert!(all_ids.contains(&want), "missing {want}");
    }
    assert!(!all_ids.contains(&id::pool_slot_used(1, 5)));
}

#[test]
fn spell_slots_cover_levels_one_to_nine_with_matching_tick_boxes() {
    let d = data(&wizard_2014());
    let c = &d.spellcasting.casters[0];
    assert_eq!(c.slots, [4, 3, 2, 0, 0, 0, 0, 0, 0]);
    let all_ids = ids(&d);
    for level in 1..=9 {
        assert!(
            all_ids.contains(&id::spell_slot_total(1, level)),
            "slot total {level}"
        );
        let used = all_ids
            .iter()
            .filter(|i| i.starts_with(&format!("spell.1.slot.{level}.used.")))
            .count();
        assert_eq!(
            used as i32,
            c.slots[level - 1],
            "tick boxes at level {level}"
        );
    }
}

#[test]
fn cantrips_are_listed_and_unprepared_spells_can_be_hidden() {
    let d = data(&wizard_2014());
    let c = &d.spellcasting.casters[0];
    assert!(c.cantrips.len() >= 3 && c.cantrips.iter().all(|s| s.level == 0));
    assert!(c.spells.iter().all(|s| s.level >= 1));
    assert!(c.listed(true).len() >= c.listed(false).len());
    assert!(c
        .listed(false)
        .iter()
        .all(|s| s.prepared || s.always_prepared));
    let sorted = c
        .spells
        .windows(2)
        .all(|w| (w[0].level, &w[0].name) <= (w[1].level, &w[1].name));
    assert!(sorted);
}

#[test]
fn damage_cantrips_become_attack_lines() {
    let d = data(&wizard_2024());
    let cantrip = d
        .combat
        .attacks
        .iter()
        .find(|a| a.kind == AttackKind::Cantrip)
        .expect("a damage cantrip");
    assert!(!cantrip.damage.is_empty() && cantrip.bonus.starts_with("DC"));
    assert!(!cantrip.notes.contains("DC"));
}

#[test]
fn spell_facts_from_content_reach_the_rows_and_stay_none_otherwise() {
    let pair = wizard_2014();
    let none = data(&pair);
    assert!(none.spellcasting.casters[0]
        .cantrips
        .iter()
        .all(|s| s.facts.is_none()));
    let target = none.spellcasting.casters[0].cantrips[0].id.clone();
    let mut facts = SpellFactsMap::new();
    facts.insert(
        target.clone(),
        SpellFacts {
            casting_time: Some("1 action".into()),
            range: Some("60 feet".into()),
            components: Some("V, S".into()),
            effect: Some("1d8 fire".into()),
            ..SpellFacts::default()
        },
    );
    let with = crate::sheet_data(&pair.0, &pair.1, &facts);
    let row = with.spellcasting.casters[0]
        .cantrips
        .iter()
        .find(|s| s.id == target)
        .unwrap();
    assert_eq!(
        row.facts.as_ref().and_then(|f| f.effect.as_deref()),
        Some("1d8 fire")
    );
}

#[test]
fn proficiencies_hide_internal_labels_and_repeats() {
    let d = data(&cleric_2014());
    let p = &d.proficiencies;
    for list in [&p.armor, &p.weapons, &p.tools, &p.languages] {
        assert!(
            list.iter().all(|s| !s.to_lowercase().contains("group")),
            "{list:?}"
        );
        let unique: HashSet<_> = list.iter().map(|s| s.to_lowercase()).collect();
        assert_eq!(unique.len(), list.len(), "duplicates in {list:?}");
    }
    assert_eq!(
        p.armor
            .iter()
            .filter(|a| a.to_lowercase().starts_with("shield"))
            .count(),
        1
    );
    assert!(p.weapons.contains(&"Simple Weapons".to_string()));
    assert!(
        !p.weapons.contains(&"Mace".to_string()),
        "members of a held category are dropped"
    );
    assert!(
        p.weapons.contains(&"Battleaxe".to_string()),
        "granted martial weapons stay"
    );
    assert!(p.armor.contains(&"Heavy Armor".to_string()));
    assert_eq!(p.languages, ["Common", "Dwarvish"]);
}

#[test]
fn features_are_unique_evaluated_and_ordered() {
    for d in all() {
        let names: Vec<_> = d.features.iter().map(|f| f.name.to_lowercase()).collect();
        let unique: HashSet<_> = names.iter().collect();
        assert_eq!(
            unique.len(),
            names.len(),
            "{}: repeated feature names",
            d.identity.name
        );
        for f in &d.features {
            assert!(!f.name.starts_with("Feature:"), "{}", f.name);
            assert_ne!(f.name, "Ability Score Improvement");
            let lower = f.text.to_lowercase();
            assert!(
                !lower.starts_with(&format!("{}.", f.name.to_lowercase())),
                "{} repeats its own name: {}",
                f.name,
                f.text
            );
            assert!(!f.text.contains("{{"), "{}: {}", f.name, f.text);
            let bytes = f.text.as_bytes();
            let raw_product = bytes
                .windows(3)
                .any(|w| w[0].is_ascii_digit() && w[1] == b'x' && w[2].is_ascii_digit());
            assert!(!raw_product, "{}: raw formula in {}", f.name, f.text);
        }
    }
    let cleric = data(&cleric_2014());
    let pos = |n: &str| cleric.features.iter().position(|f| f.name == n).unwrap();
    assert!(pos("Channel Divinity") < pos("Channel Divinity: Preserve Life"));
    let preserve = &cleric.features[pos("Channel Divinity: Preserve Life")];
    assert!(preserve.text.contains("25 hit points"), "{}", preserve.text);
    assert_eq!(preserve.group, FeatureGroup::Subclass);
    let stone = cleric
        .features
        .iter()
        .find(|f| f.name == "Stonecunning")
        .unwrap();
    assert!(
        stone.text.starts_with("Whenever you make"),
        "{}",
        stone.text
    );
    assert!(stone.summary.chars().count() <= 181);
}

#[test]
fn shared_class_features_merge_their_sources() {
    use borealis_engine::snapshot::FeatureDetail;
    let (c, mut snap) = fighter_wizard_2014();
    let mut dup: FeatureDetail = snap
        .features
        .iter()
        .find(|f| f.name == "Second Wind")
        .expect("fighter feature")
        .clone();
    dup.granted_by = Some("Paladin".into());
    snap.features.push(dup);
    let d = crate::sheet_data(&c, &snap, &SpellFactsMap::new());
    let first = "Second Wind";
    let merged: Vec<_> = d.features.iter().filter(|f| f.name == first).collect();
    assert_eq!(merged.len(), 1);
    assert!(merged[0].sources.contains(&"Paladin".to_string()));
}

#[test]
fn play_state_fields_exist_as_blank_write_ins() {
    let d = data(&wizard_2014());
    assert_eq!((d.play.current_hp, d.play.temp_hp), (None, None));
    assert_eq!(d.play.exhaustion.max_levels, 6);
    assert!(d.play.conditions.contains(&"Poisoned"));
    let fields = d.fields();
    let play: HashSet<_> = fields
        .iter()
        .filter(|f| f.play)
        .map(|f| f.id.as_str())
        .collect();
    for want in [
        "hp.current",
        "hp.temp",
        "insp",
        "death.success.3",
        "death.failure.1",
        "cond.poisoned",
        "exh.6",
        "hd.d6.spent.5",
    ] {
        assert!(play.contains(want), "{want} is a play field");
    }
    assert!(!play.contains("hd.d6.spent.6") && !play.contains("ac"));
    let kind = |i: &str| fields.iter().find(|f| f.id == i).unwrap().kind;
    assert_eq!(kind("save.int.prof"), FieldKind::Check);
    assert_eq!(kind("feat.1.text"), FieldKind::Multiline);
    assert_eq!(kind("abil.str.score"), FieldKind::Text);
}

#[test]
fn story_page_carries_personality_and_reports_content() {
    let (mut c, snap) = wizard_2014();
    assert!(!crate::sheet_data(&c, &snap, &SpellFactsMap::new())
        .story
        .has_content());
    c.details.personality.traits = "I quote sacred texts.".into();
    c.details.personality.ideals = "Knowledge.".into();
    c.details.personality.bonds = "My tower.".into();
    c.details.personality.flaws = "Overconfident.".into();
    c.details.notes.left = "left".into();
    c.details.notes.right = "right".into();
    let d = crate::sheet_data(&c, &snap, &SpellFactsMap::new());
    assert!(d.story.has_content());
    assert_eq!(
        (
            d.story.traits.as_str(),
            d.story.ideals.as_str(),
            d.story.bonds.as_str(),
            d.story.flaws.as_str()
        ),
        (
            "I quote sacred texts.",
            "Knowledge.",
            "My tower.",
            "Overconfident."
        )
    );
    assert_eq!(d.story.notes, "left\n\nright");
}

#[test]
fn field_ids_are_unique_well_formed_and_cover_the_fixed_lists() {
    for d in all() {
        let all_ids = ids(&d);
        let unique: HashSet<_> = all_ids.iter().collect();
        assert_eq!(
            unique.len(),
            all_ids.len(),
            "{}: duplicate field ids",
            d.identity.name
        );
        for i in &all_ids {
            assert!(
                !i.is_empty()
                    && i.split('.').all(|seg| !seg.is_empty()
                        && seg
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')),
                "malformed id {i}"
            );
        }
        for want in id::FIXED_COMMON.iter().chain(&id::STORY) {
            assert!(all_ids.iter().any(|i| i == want), "missing fixed id {want}");
        }
        for a in &d.abilities {
            assert!(all_ids.contains(&id::abil_score(a.key)));
            assert!(all_ids.contains(&id::save(a.key)));
        }
        for s in &d.skills {
            assert!(all_ids.contains(&id::skill(&s.key)));
        }
    }
}

#[test]
fn fixed_id_lists_do_not_overlap_and_are_pinned() {
    let all_fixed: Vec<&str> = id::FIXED_COMMON
        .iter()
        .chain(id::FIXED_2014)
        .chain(id::FIXED_2024)
        .chain(&id::STORY)
        .copied()
        .collect();
    let unique: HashSet<_> = all_fixed.iter().collect();
    assert_eq!(unique.len(), all_fixed.len());
    assert_eq!(id::abil_score("str"), "abil.str.score");
    assert_eq!(id::spell_slot_used(1, 3, 2), "spell.1.slot.3.used.2");
    assert_eq!(id::spell_row(1, 17, "name"), "spell.1.list.17.name");
    assert_eq!(id::hd_spent(6, 1), "hd.d6.spent.1");
    assert_eq!(id::cond("Poisoned"), "cond.poisoned");
    assert_eq!(id::feat(12, "text"), "feat.12.text");
}

#[test]
fn negative_bonuses_use_a_true_minus_sign() {
    assert_eq!(crate::model_common::signed_text(-1), "\u{2212}1");
    assert_eq!(crate::model_common::signed_text(3), "+3");
}

#[test]
fn damage_cantrips_of_the_2014_pack_become_attack_lines() {
    let d = data(&wizard_2014());
    let lines: Vec<_> = d
        .combat
        .attacks
        .iter()
        .filter(|a| a.kind == AttackKind::Cantrip)
        .collect();
    let fire = lines
        .iter()
        .find(|a| a.name == "Fire Bolt")
        .expect("Fire Bolt");
    assert_eq!(fire.damage, "2d10 fire");
    // The engine knows the roll without sheet facts: a spell attack shows the attack bonus.
    let caster = &d.spellcasting.casters[0];
    assert_eq!(fire.bonus, format!("{:+}", caster.attack));
    assert!(!fire.notes.contains("DC"));
    let acid = lines
        .iter()
        .find(|a| a.name == "Acid Splash")
        .expect("Acid");
    assert_eq!(acid.damage, "2d6 acid");
    assert_eq!(acid.bonus, format!("DC {} DEX", caster.dc));
    assert!(!lines.iter().any(|a| a.name == "Dancing Lights"));
}

#[test]
fn level_5_cantrip_rows_of_the_2014_pack_take_the_engine_damage() {
    // The dice come from the spell text through the engine (`SpellEntry.damage`), as in 2024.
    let d = data(&wizard_2014());
    let row = d.spellcasting.casters[0]
        .cantrips
        .iter()
        .find(|r| r.name == "Fire Bolt")
        .expect("Fire Bolt row");
    assert_eq!(row.damage.as_deref(), Some("2d10 fire"));
    let utility = d.spellcasting.casters[0]
        .cantrips
        .iter()
        .find(|r| r.name == "Dancing Lights");
    assert!(utility.map_or(true, |r| r.damage.is_none()));
}

#[test]
fn sheet_usage_pools_become_limited_uses_in_2014() {
    // SRD 5.1 Arcane Recovery has `usage="1/Day"`: the engine's resource, not the summary text.
    let d = data(&wizard_2014());
    let u = d
        .limited_uses
        .iter()
        .find(|u| u.name == "Arcane Recovery")
        .expect("Arcane Recovery");
    assert_eq!((u.max, u.recharge.as_str()), (1, "Long Rest"));
    let d24 = data(&wizard_2024());
    assert_eq!(
        d24.limited_uses
            .iter()
            .filter(|u| u.name == "Arcane Recovery")
            .count(),
        1
    );
}

#[test]
fn inverted_weapon_names_stay_one_item() {
    let d = data(&wizard_2014());
    assert!(d
        .proficiencies
        .weapons
        .contains(&"Light Crossbow".to_string()));
    for p in all().iter().flat_map(|d| &d.proficiencies.weapons) {
        assert!(!p.contains(','), "{p}");
    }
}

#[test]
fn size_comes_from_the_species_text_when_unambiguous() {
    assert_eq!(
        data(&wizard_2014()).identity.size.as_deref(),
        Some("Medium")
    );
    // The 2024 human offers Small or Medium and the fixture has not chosen: unknown, not guessed.
    assert_eq!(data(&wizard_2024()).identity.size, None);
}

#[test]
fn a_forced_edition_picks_that_field_set() {
    use crate::SheetEdition;
    let (c, s) = wizard_2024();
    let m = SpellFactsMap::new();
    let auto = crate::sheet_data_for(&c, &s, &m, SheetEdition::Auto);
    assert_eq!(auto.edition, Edition::Dnd2024);
    let forced = crate::sheet_data_for(&c, &s, &m, SheetEdition::Y2014);
    assert_eq!(forced.edition, Edition::Dnd2014);
    assert!(matches!(forced.edition_fields, EditionFields::Y2014(_)));
    let back = crate::sheet_data_for(&c, &s, &m, SheetEdition::Y2024);
    assert!(matches!(back.edition_fields, EditionFields::Y2024(_)));
}

#[test]
fn caster_modifier_matches_the_ability_and_the_save_dc() {
    for pair in [
        wizard_2014(),
        wizard_2024(),
        cleric_2014(),
        cleric_wizard_2014(),
        fighter_wizard_2014(),
    ] {
        let d = data(&pair);
        for c in &d.spellcasting.casters {
            let ability = d
                .abilities
                .iter()
                .find(|a| a.key == c.ability)
                .expect("caster ability on the sheet");
            assert_eq!(c.ability_modifier, ability.modifier, "{}", c.class);
            assert_eq!(c.dc, 8 + d.proficiency_bonus + c.ability_modifier);
        }
    }
    let w = data(&wizard_2024());
    assert_eq!(w.spellcasting.casters[0].ability_modifier, 2);
}

#[test]
fn pooled_casters_emit_only_pooled_slot_ids() {
    let d = data(&cleric_wizard_2014());
    let all_ids = ids(&d);
    assert!(
        !all_ids
            .iter()
            .any(|i| i.starts_with("spell.1.slot.") || i.starts_with("spell.2.slot.")),
        "per-class slot ids leaked"
    );
    assert!(all_ids.contains(&id::pool_slot_used(1, 4)));
}

#[test]
fn human_versatile_text_does_not_carry_the_tiefling_legacies_table() {
    let st = crate::model_fixtures::store("srd-5.2.1");
    let el = st
        .element("ID_2024_TRAIT_HUMAN_VERSATILE")
        .expect("Versatile trait in the pack");
    assert!(el.description.contains("Skilled is recommended"));
    assert!(!el.description.contains("Fiendish"), "{}", el.description);
    assert!(!el.description.contains("Abyssal"));
}

#[test]
fn cantrip_hit_dc_cell_follows_the_spell_facts() {
    let pair = wizard_2014();
    let base = data(&pair);
    let caster = &base.spellcasting.casters[0];
    let (dc, atk) = (caster.dc, caster.attack);
    let id_of = |name: &str| {
        caster
            .cantrips
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.id.clone())
            .expect(name)
    };
    let mut facts = SpellFactsMap::new();
    facts.insert(
        id_of("Fire Bolt"),
        SpellFacts {
            attack_roll: true,
            ..SpellFacts::default()
        },
    );
    facts.insert(
        id_of("Acid Splash"),
        SpellFacts {
            save: Some("DEX".into()),
            ..SpellFacts::default()
        },
    );
    let d = crate::sheet_data(&pair.0, &pair.1, &facts);
    let cell = |name: &str| {
        d.combat
            .attacks
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.bonus.clone())
            .expect(name)
    };
    assert_eq!(
        cell("Fire Bolt"),
        format!("{atk:+}").replace('-', "\u{2212}")
    );
    assert_eq!(cell("Acid Splash"), format!("DC {dc} DEX"));
}

#[test]
fn rolled_charges_show_their_dice_as_the_app_does() {
    use borealis_engine::snapshot::{ResourceSnapshot, RolledMaxSnapshot};
    let (c, mut snap) = wizard_2014();
    for (name, max, rolled) in [
        ("Wand of Lives charges", 7, true),
        ("Ring charges", 6, false),
    ] {
        snap.resources.push(ResourceSnapshot {
            id: format!("ID_T_{max}"),
            name: name.into(),
            max,
            current: max,
            source: name.into(),
            recharge: vec!["dawn".into()],
            short_rest_regain: None,
            long_rest_regain_all: false,
            other_regain: Vec::new(),
            rolled: Some(RolledMaxSnapshot {
                dice: "1d6 + 3".into(),
                item_uid: Some("item-1".into()),
                rolled,
                min: 4,
                max: 9,
            }),
        });
    }
    let d = data(&(c.clone(), snap.clone()));
    let note = |n: &str| d.limited_uses.iter().find(|u| u.name == n).unwrap().note();
    assert_eq!(note("Wand of Lives charges"), "Dawn; max 7 (1d6 + 3)");
    assert_eq!(note("Ring charges"), "Dawn; max 6 (1d6 + 3, average)");
    // a fixed maximum keeps the plain recharge
    let arcane = d.limited_uses.iter().find(|u| u.name == "Arcane Recovery");
    assert_eq!(arcane.unwrap().note(), "Long Rest");
    let html = crate::render_html(&c, &snap, &crate::SheetOptions::default()).expect("html");
    assert!(html.contains("Dawn; max 7 (1d6 + 3)"), "printed sheet");
}
