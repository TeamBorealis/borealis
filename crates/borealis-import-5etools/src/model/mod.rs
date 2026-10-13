//! Typed, tolerant views of the 5etools JSON entities.
//!
//! Every struct deserialises from a raw entry (after `_copy` resolution) and keeps all keys
//! it does not know in `extra`, so nothing is lost and unknown fields never fail a parse.
//! Polymorphic fields stay [`serde_json::Value`] and are interpreted by the converters.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// Free-form leftovers of an entry.
pub type Extra = BTreeMap<String, Value>;

/// A race (`races.json` `race`) or subrace (`subrace`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Race {
    /// Name; absent for the unnamed base subrace of a race.
    pub name: Option<String>,
    /// Source abbreviation.
    pub source: String,
    /// Parent race name (subraces).
    #[serde(rename = "raceName")]
    pub race_name: Option<String>,
    /// Parent race source (subraces).
    #[serde(rename = "raceSource")]
    pub race_source: Option<String>,
    /// `classic` or `one`.
    pub edition: Option<String>,
    /// Reprints in other sources.
    #[serde(rename = "reprintedAs")]
    pub reprinted_as: Vec<Value>,
    /// Size codes (`T S M L H G V`).
    pub size: Vec<String>,
    /// Speed: integer, object or `"Varies"`.
    pub speed: Option<Value>,
    /// Ability bonuses.
    pub ability: Vec<Value>,
    /// Darkvision range in feet.
    pub darkvision: Option<i64>,
    /// Blindsight range in feet.
    pub blindsight: Option<i64>,
    /// Creature types (2024 species).
    #[serde(rename = "creatureTypes")]
    pub creature_types: Vec<Value>,
    /// Language proficiencies.
    #[serde(rename = "languageProficiencies")]
    pub language_proficiencies: Vec<Value>,
    /// Skill proficiencies.
    #[serde(rename = "skillProficiencies")]
    pub skill_proficiencies: Vec<Value>,
    /// Tool proficiencies.
    #[serde(rename = "toolProficiencies")]
    pub tool_proficiencies: Vec<Value>,
    /// Weapon proficiencies.
    #[serde(rename = "weaponProficiencies")]
    pub weapon_proficiencies: Vec<Value>,
    /// Armor proficiencies.
    #[serde(rename = "armorProficiencies")]
    pub armor_proficiencies: Vec<Value>,
    /// Skill/tool/language mixed proficiencies.
    #[serde(rename = "skillToolLanguageProficiencies")]
    pub skill_tool_language_proficiencies: Vec<Value>,
    /// Bonus feats.
    pub feats: Vec<Value>,
    /// Damage resistances.
    pub resist: Vec<Value>,
    /// Damage immunities.
    pub immune: Vec<Value>,
    /// Damage vulnerabilities.
    pub vulnerable: Vec<Value>,
    /// Condition immunities.
    #[serde(rename = "conditionImmune")]
    pub condition_immune: Vec<Value>,
    /// Innate and known spells.
    #[serde(rename = "additionalSpells")]
    pub additional_spells: Vec<Value>,
    /// Rich text.
    pub entries: Vec<Value>,
    /// Height and weight data.
    #[serde(rename = "heightAndWeight")]
    pub height_and_weight: Option<Value>,
    /// Properties a subrace replaces on its race.
    pub overwrite: Option<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A class (`class-*.json` `class`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Class {
    /// Name.
    pub name: String,
    /// Source abbreviation.
    pub source: String,
    /// `classic` or `one`.
    pub edition: Option<String>,
    /// Reprints in other sources.
    #[serde(rename = "reprintedAs")]
    pub reprinted_as: Vec<Value>,
    /// Hit die.
    pub hd: Option<HitDie>,
    /// Saving throw proficiencies (ability abbreviations).
    pub proficiency: Vec<String>,
    /// Primary ability (2024).
    #[serde(rename = "primaryAbility")]
    pub primary_ability: Vec<Value>,
    /// Per-level table groups.
    #[serde(rename = "classTableGroups")]
    pub class_table_groups: Vec<Value>,
    /// Starting proficiencies.
    #[serde(rename = "startingProficiencies")]
    pub starting_proficiencies: Option<Value>,
    /// Starting equipment.
    #[serde(rename = "startingEquipment")]
    pub starting_equipment: Option<Value>,
    /// Multiclassing requirements and proficiencies.
    pub multiclassing: Option<Value>,
    /// `full`, `1/2`, `1/3`, `pact` or `artificer`.
    #[serde(rename = "casterProgression")]
    pub caster_progression: Option<String>,
    /// Spellcasting ability abbreviation.
    #[serde(rename = "spellcastingAbility")]
    pub spellcasting_ability: Option<String>,
    /// Cantrips known per level (20 entries).
    #[serde(rename = "cantripProgression")]
    pub cantrip_progression: Vec<i64>,
    /// Prepared spell formula (2014).
    #[serde(rename = "preparedSpells")]
    pub prepared_spells: Option<String>,
    /// Prepared spells per level (2024, 20 entries).
    #[serde(rename = "preparedSpellsProgression")]
    pub prepared_spells_progression: Vec<i64>,
    /// Spells known per level (20 entries).
    #[serde(rename = "spellsKnownProgression")]
    pub spells_known_progression: Vec<i64>,
    /// Spellbook spells gained per level (20 entries).
    #[serde(rename = "spellsKnownProgressionFixed")]
    pub spells_known_progression_fixed: Vec<i64>,
    /// Optional feature picks per level.
    #[serde(rename = "optionalfeatureProgression")]
    pub optionalfeature_progression: Vec<Value>,
    /// Feat picks per level (2024).
    #[serde(rename = "featProgression")]
    pub feat_progression: Vec<Value>,
    /// References to class features (`name|class|classSource|level|source`).
    #[serde(rename = "classFeatures")]
    pub class_features: Vec<Value>,
    /// Name of the subclass choice.
    #[serde(rename = "subclassTitle")]
    pub subclass_title: Option<String>,
    /// Always-known / expanded spells.
    #[serde(rename = "additionalSpells")]
    pub additional_spells: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// Hit die as `{number, faces}`.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct HitDie {
    /// Number of dice.
    #[serde(default = "one")]
    pub number: u32,
    /// Faces per die.
    #[serde(default)]
    pub faces: u32,
}

fn one() -> u32 {
    1
}

/// A subclass (`class-*.json` `subclass`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Subclass {
    /// Name.
    pub name: String,
    /// Short name (used in feature references).
    #[serde(rename = "shortName")]
    pub short_name: String,
    /// Source abbreviation.
    pub source: String,
    /// Parent class name.
    #[serde(rename = "className")]
    pub class_name: String,
    /// Parent class source.
    #[serde(rename = "classSource")]
    pub class_source: String,
    /// `classic` or `one`.
    pub edition: Option<String>,
    /// Reprints in other sources.
    #[serde(rename = "reprintedAs")]
    pub reprinted_as: Vec<Value>,
    /// Subclass feature references.
    #[serde(rename = "subclassFeatures")]
    pub subclass_features: Vec<Value>,
    /// Subclass spellcasting progression.
    #[serde(rename = "casterProgression")]
    pub caster_progression: Option<String>,
    /// Spellcasting ability abbreviation.
    #[serde(rename = "spellcastingAbility")]
    pub spellcasting_ability: Option<String>,
    /// Cantrips known per level.
    #[serde(rename = "cantripProgression")]
    pub cantrip_progression: Vec<i64>,
    /// Spells known per level.
    #[serde(rename = "spellsKnownProgression")]
    pub spells_known_progression: Vec<i64>,
    /// Prepared spell formula.
    #[serde(rename = "preparedSpells")]
    pub prepared_spells: Option<String>,
    /// Table groups.
    #[serde(rename = "subclassTableGroups")]
    pub subclass_table_groups: Vec<Value>,
    /// Optional feature picks per level.
    #[serde(rename = "optionalfeatureProgression")]
    pub optionalfeature_progression: Vec<Value>,
    /// Feat picks per level.
    #[serde(rename = "featProgression")]
    pub feat_progression: Vec<Value>,
    /// Domain / expanded spells.
    #[serde(rename = "additionalSpells")]
    pub additional_spells: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A class feature or subclass feature.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Feature {
    /// Name.
    pub name: String,
    /// Source abbreviation.
    pub source: String,
    /// Class name.
    #[serde(rename = "className")]
    pub class_name: String,
    /// Class source.
    #[serde(rename = "classSource")]
    pub class_source: String,
    /// Subclass short name (subclass features).
    #[serde(rename = "subclassShortName")]
    pub subclass_short_name: Option<String>,
    /// Subclass source (subclass features).
    #[serde(rename = "subclassSource")]
    pub subclass_source: Option<String>,
    /// Level the feature is gained.
    pub level: u32,
    /// Rich text.
    pub entries: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A background.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Background {
    /// Name.
    pub name: String,
    /// Source abbreviation.
    pub source: String,
    /// `classic` or `one`.
    pub edition: Option<String>,
    /// Reprints.
    #[serde(rename = "reprintedAs")]
    pub reprinted_as: Vec<Value>,
    /// Ability scores (2024: weighted choices).
    pub ability: Vec<Value>,
    /// Origin feats.
    pub feats: Vec<Value>,
    /// Skill proficiencies.
    #[serde(rename = "skillProficiencies")]
    pub skill_proficiencies: Vec<Value>,
    /// Tool proficiencies.
    #[serde(rename = "toolProficiencies")]
    pub tool_proficiencies: Vec<Value>,
    /// Language proficiencies.
    #[serde(rename = "languageProficiencies")]
    pub language_proficiencies: Vec<Value>,
    /// Mixed skill/tool/language proficiencies.
    #[serde(rename = "skillToolLanguageProficiencies")]
    pub skill_tool_language_proficiencies: Vec<Value>,
    /// Weapon proficiencies.
    #[serde(rename = "weaponProficiencies")]
    pub weapon_proficiencies: Vec<Value>,
    /// Armor proficiencies.
    #[serde(rename = "armorProficiencies")]
    pub armor_proficiencies: Vec<Value>,
    /// Starting equipment.
    #[serde(rename = "startingEquipment")]
    pub starting_equipment: Vec<Value>,
    /// Innate spells.
    #[serde(rename = "additionalSpells")]
    pub additional_spells: Vec<Value>,
    /// Rich text.
    pub entries: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A feat.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Feat {
    /// Name.
    pub name: String,
    /// Source abbreviation.
    pub source: String,
    /// `classic` or `one`.
    pub edition: Option<String>,
    /// Reprints.
    #[serde(rename = "reprintedAs")]
    pub reprinted_as: Vec<Value>,
    /// Category code (`O`, `G`, `FS`, `EB`, ...).
    pub category: Option<String>,
    /// Prerequisites.
    pub prerequisite: Vec<Value>,
    /// Ability increases.
    pub ability: Vec<Value>,
    /// May be taken more than once.
    pub repeatable: Option<bool>,
    /// Innate and known spells.
    #[serde(rename = "additionalSpells")]
    pub additional_spells: Vec<Value>,
    /// Language proficiencies.
    #[serde(rename = "languageProficiencies")]
    pub language_proficiencies: Vec<Value>,
    /// Skill proficiencies.
    #[serde(rename = "skillProficiencies")]
    pub skill_proficiencies: Vec<Value>,
    /// Tool proficiencies.
    #[serde(rename = "toolProficiencies")]
    pub tool_proficiencies: Vec<Value>,
    /// Weapon proficiencies.
    #[serde(rename = "weaponProficiencies")]
    pub weapon_proficiencies: Vec<Value>,
    /// Armor proficiencies.
    #[serde(rename = "armorProficiencies")]
    pub armor_proficiencies: Vec<Value>,
    /// Mixed skill/tool/language proficiencies.
    #[serde(rename = "skillToolLanguageProficiencies")]
    pub skill_tool_language_proficiencies: Vec<Value>,
    /// Saving throw proficiencies.
    #[serde(rename = "savingThrowProficiencies")]
    pub saving_throw_proficiencies: Vec<Value>,
    /// Expertise.
    pub expertise: Vec<Value>,
    /// Damage resistances.
    pub resist: Vec<Value>,
    /// Damage immunities.
    pub immune: Vec<Value>,
    /// Damage vulnerabilities.
    pub vulnerable: Vec<Value>,
    /// Condition immunities.
    #[serde(rename = "conditionImmune")]
    pub condition_immune: Vec<Value>,
    /// Senses granted.
    pub senses: Vec<Value>,
    /// Optional feature picks.
    #[serde(rename = "optionalfeatureProgression")]
    pub optionalfeature_progression: Vec<Value>,
    /// Rich text.
    pub entries: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A spell.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Spell {
    /// Name.
    pub name: String,
    /// Source abbreviation.
    pub source: String,
    /// `classic` or `one`.
    pub edition: Option<String>,
    /// Reprints.
    #[serde(rename = "reprintedAs")]
    pub reprinted_as: Vec<Value>,
    /// Spell level, 0 for cantrips.
    pub level: u32,
    /// School code (`A C D E V I N T`).
    pub school: String,
    /// Casting times.
    pub time: Vec<Value>,
    /// Range.
    pub range: Option<Value>,
    /// Components.
    pub components: Option<Value>,
    /// Durations.
    pub duration: Vec<Value>,
    /// Ritual and technomagic flags.
    pub meta: Option<Value>,
    /// Rich text.
    pub entries: Vec<Value>,
    /// Higher level text.
    #[serde(rename = "entriesHigherLevel")]
    pub entries_higher_level: Vec<Value>,
    /// Damage types inflicted.
    #[serde(rename = "damageInflict")]
    pub damage_inflict: Vec<String>,
    /// Saving throws.
    #[serde(rename = "savingThrow")]
    pub saving_throw: Vec<String>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A base item, item or magic variant result.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Item {
    /// Name.
    pub name: String,
    /// Source abbreviation.
    pub source: String,
    /// `classic` or `one`.
    pub edition: Option<String>,
    /// Reprints.
    #[serde(rename = "reprintedAs")]
    pub reprinted_as: Vec<Value>,
    /// Item type code (`M`, `R`, `LA`, `MA`, `HA`, `S`, `A`, `G`, ...), maybe `|source`.
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    /// Rarity (`none`, `common`, ...).
    pub rarity: Option<String>,
    /// Weight in pounds.
    pub weight: Option<f64>,
    /// Value in copper pieces.
    pub value: Option<f64>,
    /// `simple` or `martial`.
    #[serde(rename = "weaponCategory")]
    pub weapon_category: Option<String>,
    /// Property codes.
    pub property: Vec<Value>,
    /// Weapon mastery references.
    pub mastery: Vec<Value>,
    /// Primary damage dice.
    pub dmg1: Option<String>,
    /// Versatile damage dice.
    pub dmg2: Option<String>,
    /// Damage type code.
    #[serde(rename = "dmgType")]
    pub dmg_type: Option<String>,
    /// Range text (`80/320`).
    pub range: Option<String>,
    /// Armor class.
    pub ac: Option<f64>,
    /// Strength requirement (string or number).
    pub strength: Option<Value>,
    /// Stealth disadvantage.
    pub stealth: Option<bool>,
    /// Attunement: `true` or a requirement text.
    #[serde(rename = "reqAttune")]
    pub req_attune: Option<Value>,
    /// Weapon enhancement bonus (`+1`).
    #[serde(rename = "bonusWeapon")]
    pub bonus_weapon: Option<String>,
    /// Armor class bonus.
    #[serde(rename = "bonusAc")]
    pub bonus_ac: Option<String>,
    /// Saving throw bonus.
    #[serde(rename = "bonusSavingThrow")]
    pub bonus_saving_throw: Option<String>,
    /// Pack contents.
    #[serde(rename = "packContents")]
    pub pack_contents: Vec<Value>,
    /// Rich text.
    pub entries: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// An item group (`itemGroup`): a named set of items.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ItemGroup {
    /// Name.
    pub name: String,
    /// Source abbreviation.
    pub source: String,
    /// Member item references (`name|source`).
    pub items: Vec<Value>,
    /// Rarity.
    pub rarity: Option<String>,
    /// Rich text.
    pub entries: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A magic variant (`magicvariants.json`): a template that makes new items from base items.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MagicVariant {
    /// Name of the variant.
    pub name: String,
    /// Requirements a base item must meet.
    pub requires: Vec<Value>,
    /// Requirements that exclude a base item.
    pub excludes: Option<Value>,
    /// What the produced item inherits.
    pub inherits: Option<Value>,
    /// Rich text of the variant.
    pub entries: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// An optional feature (invocations, metamagic, fighting styles, maneuvers, ...).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct OptionalFeature {
    /// Name.
    pub name: String,
    /// Source abbreviation.
    pub source: String,
    /// `classic` or `one`.
    pub edition: Option<String>,
    /// Reprints.
    #[serde(rename = "reprintedAs")]
    pub reprinted_as: Vec<Value>,
    /// Feature type codes (`EI`, `MM`, `FS:F`, ...).
    #[serde(rename = "featureType")]
    pub feature_type: Vec<String>,
    /// Prerequisites.
    pub prerequisite: Vec<Value>,
    /// Rich text.
    pub entries: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A language.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Language {
    /// Name.
    pub name: String,
    /// Source abbreviation.
    pub source: String,
    /// `classic` or `one`.
    pub edition: Option<String>,
    /// `standard`, `exotic`, `rare`, `secret`, ...
    #[serde(rename = "type")]
    pub language_type: Option<String>,
    /// Script.
    pub script: Option<String>,
    /// Rich text.
    pub entries: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A generic named reference entry (skills, conditions, senses, item properties, ...).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Reference {
    /// Name.
    pub name: String,
    /// Abbreviation (item properties and types).
    pub abbreviation: Option<String>,
    /// Source abbreviation.
    pub source: String,
    /// Key ability (skills).
    pub ability: Option<String>,
    /// Rich text.
    pub entries: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// A fluff entry: lore text for an entity of the same name and source.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Fluff {
    /// Name of the entity.
    pub name: String,
    /// Source of the entity.
    pub source: String,
    /// Lore text.
    pub entries: Vec<Value>,
    /// Everything else.
    #[serde(flatten)]
    pub extra: Extra,
}

/// Deserialise a typed entity from a raw entry.
///
/// # Errors
/// The serde error message when a known field has the wrong shape.
pub fn parse<T: for<'de> Deserialize<'de>>(raw: &Value) -> Result<T, String> {
    serde_json::from_value(without_null_fields(raw)).map_err(|e| e.to_string())
}

/// A copy of an entry without its `null` top-level fields. 5etools writes `"prop": null` in
/// `_copy` / `_versions` overrides (and on subraces) to remove an inherited property, so a
/// `null` field means "absent", never "wrong shape".
#[must_use]
pub fn without_null_fields(raw: &Value) -> Value {
    match raw {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn null_fields_mean_absent() {
        let r: Race = parse(&json!({
            "name": "Kobold; Defiance", "source": "ZZZ", "skillProficiencies": null,
            "additionalSpells": null, "resist": null, "speed": 30
        }))
        .unwrap();
        assert_eq!(r.name.as_deref(), Some("Kobold; Defiance"));
        assert_eq!(
            without_null_fields(&json!({"a": null, "b": 1})),
            json!({"b": 1})
        );
    }

    #[test]
    fn tolerant_parsing_keeps_unknown_fields() {
        let r: Race = parse(&json!({
            "name": "Dwarf", "source": "PHB", "size": ["M"], "speed": 25,
            "ability": [{"con": 2}], "mysteryField": {"a": 1}, "page": 18
        }))
        .unwrap();
        assert_eq!(r.name.as_deref(), Some("Dwarf"));
        assert_eq!(r.size, vec!["M"]);
        assert!(r.extra.contains_key("mysteryField"));
        assert!(r.extra.contains_key("page"));
    }

    #[test]
    fn wrong_shapes_report_errors() {
        let err = parse::<Spell>(&json!({"name": "x", "source": "y", "level": "three"}));
        assert!(err.is_err());
        let ok: Spell = parse(&json!({"name": "x", "source": "y"})).unwrap();
        assert_eq!(ok.level, 0);
    }

    #[test]
    fn class_shapes() {
        let c: Class = parse(&json!({
            "name": "Fighter", "source": "PHB", "hd": {"number": 1, "faces": 10},
            "proficiency": ["str", "con"], "classFeatures": ["A|Fighter||1", {"classFeature": "B|Fighter||2", "gainSubclassFeature": true}],
            "cantripProgression": [3, 3]
        }))
        .unwrap();
        assert_eq!(c.hd.unwrap().faces, 10);
        assert_eq!(c.class_features.len(), 2);
        let h: HitDie = parse(&json!({"faces": 8})).unwrap();
        assert_eq!(h.number, 1);
    }
}
