//! Registry of the custom rule kinds (`Rule::Custom`) the engine reads, with the fields each
//! reader looks at.
//!
//! The JSON schema accepts any `kind` string and any field, so a misspelled kind or field would
//! otherwise be kept and silently ignored. Every reader of custom rules in `borealis-engine`
//! (`custom_rules::normalize`, `play_rules`, `spell_rules`, `attack_rules`, `combat`, `notes_d`,
//! `wild_shape`, `packages`) is listed here; the engine reports anything else as
//! `content.unknown_rule_kind` / `content.unknown_rule_field`, and the 5etools importer refuses
//! to emit it. Keep this table in sync with the readers: a new kind or field needs both a reader
//! and an entry (the engine tests diff this table against the bundled content).
//!
//! The registry lives in this crate so that producers of content (importers) can check their
//! output without depending on the engine; `borealis_engine::custom_rules::KNOWN_KINDS`
//! re-exports it.

use crate::rules::{CustomRule, Rule};

/// Fields read by every kind whose reader honours the `level` gate and a `requirements`
/// expression (`rule_view::live_rules`, `rule_view::rule_live`).
pub const GATE_FIELDS: &[&str] = &["level", "requirements"];

/// One custom rule kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindSpec {
    /// The `kind` string.
    pub kind: &'static str,
    /// The engine module that reads it.
    pub reader: &'static str,
    /// True when the reader honours [`GATE_FIELDS`].
    pub gated: bool,
    /// Other top-level fields the reader evaluates (or surfaces verbatim as note text).
    pub fields: &'static [&'static str],
    /// Fields bundled content carries that the engine knowingly does not evaluate, with the
    /// reason. They are accepted (no diagnostic); anything else is unknown.
    pub unmodelled: &'static [(&'static str, &'static str)],
    /// Fields the reader cannot do without (absent, null or an empty string is a problem).
    pub required: &'static [&'static str],
    /// Closed value sets as `(path, allowed)`: `field`, `field.sub` (a key of an object field)
    /// or `field[]` (every item of a list field).
    pub enums: &'static [(&'static str, &'static [&'static str])],
}

/// `max.type` values a `resource` understands (`rolled`: `dice` such as `1d6+1`, rolled once
/// when the item is gained and stored on the inventory line).
pub const RESOURCE_MAX_TYPES: &[&str] = &["fixed", "stat", "table", "formula", "rolled"];
/// `recharge` tokens of a `resource` (`short_rest`/`long_rest` drive rests).
pub const RESOURCE_RECHARGE: &[&str] = &[
    "short_rest",
    "long_rest",
    "turn_start",
    "initiative",
    "none",
];

/// `mode` values of a `grant_spell`: how the granted spell is cast. `always_prepared` and `known`
/// spells use the owner's slots as usual (a `free_cast` adds slotless uses per rest, and
/// `resource_cost` a slotless cast paid with resource points); `known_cantrip` is a cantrip;
/// `at_will_no_slot` is cast at will without a slot; `ritual_only` only as a ritual.
pub const GRANT_SPELL_MODES: &[&str] = &[
    "always_prepared",
    "known",
    "known_cantrip",
    "at_will_no_slot",
    "ritual_only",
];

impl KindSpec {
    /// Does the engine accept `field` on this kind?
    #[must_use]
    pub fn knows(&self, field: &str) -> bool {
        (self.gated && GATE_FIELDS.contains(&field))
            || self.fields.contains(&field)
            || self.unmodelled.iter().any(|(f, _)| *f == field)
    }

    /// Does the engine read `field` (gate fields included)?
    #[must_use]
    pub fn reads(&self, field: &str) -> bool {
        (self.gated && GATE_FIELDS.contains(&field)) || self.fields.contains(&field)
    }

    const fn shape(
        self,
        required: &'static [&'static str],
        enums: &'static [(&'static str, &'static [&'static str])],
    ) -> Self {
        Self {
            required,
            enums,
            ..self
        }
    }

    /// Missing required fields and values outside the closed sets, one message each.
    #[must_use]
    pub fn shape_problems(
        &self,
        fields: &serde_json::Map<String, serde_json::Value>,
    ) -> Vec<String> {
        use serde_json::Value;
        let mut out: Vec<String> = self
            .required
            .iter()
            .filter(|f| match fields.get(**f) {
                None | Some(Value::Null) => true,
                Some(Value::String(s)) => s.trim().is_empty(),
                Some(_) => false,
            })
            .map(|f| format!("has no `{f}`"))
            .collect();
        for (path, allowed) in self.enums {
            let ok = |v: &Value| v.as_str().is_some_and(|t| allowed.contains(&t));
            if let Some(list) = path.strip_suffix("[]") {
                if let Some(v) = fields.get(list) {
                    if !v.as_array().is_some_and(|a| a.iter().all(ok)) {
                        out.push(format!("`{list}` {v} is not a list of {allowed:?}"));
                    }
                }
            } else if let Some((field, key)) = path.split_once('.') {
                if let Some(v) = fields.get(field) {
                    if !v.get(key).is_some_and(ok) {
                        out.push(format!(
                            "`{field}` {v} is not {{\"{key}\": one of {allowed:?}, ...}}"
                        ));
                    }
                }
            } else if let Some(v) = fields.get(*path).filter(|v| !ok(v)) {
                out.push(format!("`{path}` {v} is not one of {allowed:?}"));
            }
        }
        out
    }
}

const fn k(
    kind: &'static str,
    reader: &'static str,
    gated: bool,
    fields: &'static [&'static str],
) -> KindSpec {
    KindSpec {
        kind,
        reader,
        gated,
        fields,
        unmodelled: &[],
        required: &[],
        enums: &[],
    }
}

const fn ku(
    kind: &'static str,
    reader: &'static str,
    gated: bool,
    fields: &'static [&'static str],
    unmodelled: &'static [(&'static str, &'static str)],
) -> KindSpec {
    KindSpec {
        kind,
        reader,
        gated,
        fields,
        unmodelled,
        required: &[],
        enums: &[],
    }
}

const NORMALIZE: &str = "custom_rules::normalize";
const NOTES: &str = "notes_d (note text)";
const NOTES_GENERIC: &str = "notes_d (fields shown verbatim)";
const COMBAT: &str = "combat";
const ATTACK: &str = "attack_rules";
const PLAY: &str = "play_rules";
const SPELLS: &str = "spell_rules";

/// Known engine gap: the field means something the engine does not evaluate yet. Removing an
/// entry once a reader exists is the fix; adding one needs review (the engine test
/// `unmodelled_fields_are_pinned` lists them all).
pub const GAP: &str = "not evaluated yet (known engine gap)";
/// Intentionally ignored: a rule applied at the table; the engine does not roll casts or damage
/// and the feature text tells the player.
pub const TABLE_TIME: &str = "resolved at the table: the engine does not roll casts or damage";
/// Intentionally ignored: when a pick may change or a pool recovers (a rest, a level) is not
/// modelled; picks stay editable at any time.
pub const REST_TIMING: &str = "rest/level timing of a re-pick or a recovery is not modelled";
/// Redundant: another field the engine reads, or the engine's own model, already says this.
pub const IMPLIED: &str = "implied by another field or by the engine model";

/// Every custom rule kind the engine reads, sorted by kind.
pub const KNOWN_KINDS: &[KindSpec] = &[
    k(
        "ability_distribution",
        NORMALIZE,
        true,
        &["name", "pool", "options", "cap"],
    ),
    k(
        "ability_increase",
        NORMALIZE,
        true,
        &["name", "options", "cap"],
    ),
    k(
        "at_will_spells",
        NOTES_GENERIC,
        true,
        &["levels", "casting_time", "swap_on"],
    ),
    k("attack_bonus", ATTACK, true, &["weapon", "value"]),
    ku(
        "aura",
        NOTES,
        true,
        &["radius_feet", "radius_by_level", "bonus_saving_throws"],
        &[("id", "a name other rules (`aura_immunity.aura`) refer to")],
    ),
    k("aura_immunity", NOTES, true, &["aura", "conditions"]),
    k(
        "beast_forms",
        "wild_shape",
        true,
        &["known", "duration_hours", "temp_hp", "change_on"],
    ),
    ku(
        "cantrip_damage_bonus",
        NOTES,
        true,
        &["spell_class", "value", "choose_cantrip", "cantrip_filter"],
        &[],
    ),
    ku(
        "cantrip_range_bonus",
        NOTES,
        true,
        &["spell_class", "feet_per_warlock_level", "cantrip_filter"],
        &[("choose_cantrip", IMPLIED)],
    ),
    ku(
        "cantrip_rider",
        NOTES,
        true,
        &["cantrip_filter", "effect", "spell_class"],
        &[("choose_cantrip", IMPLIED)],
    ),
    k(
        "cantrip_swap",
        NOTES_GENERIC,
        true,
        &["spell_list", "swap_on", "replaces"],
    ),
    k("carry_capacity_size_bonus", COMBAT, true, &["steps"]),
    k(
        "channel_divinity_option",
        NOTES,
        true,
        &[
            "name",
            "duration",
            "attack_bonus",
            "pool_hit_points",
            "max_fraction",
        ],
    ),
    k(
        "check_floor",
        COMBAT,
        true,
        &["applies_to", "floor", "d20_floor"],
    ),
    k("condition_immunity", COMBAT, true, &["conditions"]),
    k(
        "condition_immunity_while",
        COMBAT,
        true,
        &["toggle", "conditions"],
    ),
    ku(
        "convert",
        NOTES,
        true,
        &["from", "to", "table", "amount", "action"],
        &[("expires", REST_TIMING)],
    ),
    k(
        "critical_range",
        ATTACK,
        true,
        &["min", "applies_to", "item"],
    ),
    k("cunning_effect", NOTES, true, &["cost_dice", "save"]),
    k("cunning_strike", NOTES, true, &["save_dc", "max_effects"]),
    k(
        "damage_bonus_spell",
        COMBAT,
        true,
        &["value", "once_per", "damage_type_stat", "damage_type_param"],
    ),
    k("damage_die_floor", ATTACK, true, &["floor", "weapon"]),
    ku(
        "damage_reduction",
        COMBAT,
        true,
        &["dice", "plus", "damage_types"],
        &[("reaction", IMPLIED)],
    ),
    k("damage_reduction_all_types", COMBAT, true, &[]),
    ku(
        "damage_scaling",
        NOTES,
        true,
        &["name", "levels", "type_from"],
        &[("dice", IMPLIED), ("scale", IMPLIED)],
    ),
    k(
        "death_save_rules",
        COMBAT,
        true,
        &["advantage", "natural_20_threshold"],
    ),
    k("equipment_package", "packages", true, &["ref", "name"]),
    k("evasion", COMBAT, true, &[]),
    ku(
        "exhaustion",
        PLAY,
        false,
        &[
            "max_levels",
            "d20_penalty_per_level",
            "speed_penalty_ft_per_level",
        ],
        &[("dies_at_level", IMPLIED), ("reduce_on", REST_TIMING)],
    ),
    ku(
        "expertise",
        NORMALIZE,
        false,
        &["count", "level"],
        &[("from", IMPLIED)],
    ),
    ku(
        "extra_attack_damage_modifier",
        ATTACK,
        true,
        &[],
        &[("property", IMPLIED)],
    ),
    ku(
        "extra_damage",
        COMBAT,
        true,
        &[
            "name",
            "dice",
            "dice_table",
            "die",
            "dice_count",
            "damage_type",
            "once_per",
            "trigger",
            "requires",
            "cost",
            "replaces",
            "effects_choose",
            "per_slot_level",
        ],
        &[("first_target_only", TABLE_TIME), ("weapon", TABLE_TIME)],
    ),
    k(
        "free_cast_choice",
        NOTES_GENERIC,
        true,
        &[
            "spell_level",
            "count",
            "uses_each",
            "uses",
            "recharge",
            "from_feature",
        ],
    ),
    k(
        "grant_feat",
        NORMALIZE,
        false,
        &["feat", "params", "level", "requirements"],
    ),
    ku(
        "grant_spell",
        NORMALIZE,
        false,
        &[
            "spell",
            "mode",
            "spell_list",
            "free_cast",
            "also_with_slots",
            "requirements_param",
            "cast_level",
            "resource_cost",
            "level",
            "requirements",
        ],
        &[
            ("counts_against_prepared", IMPLIED),
            ("ability", GAP),
            ("target", TABLE_TIME),
            ("auto_succeed_save", TABLE_TIME),
            ("max_roll", TABLE_TIME),
            ("condition", TABLE_TIME),
            ("special_forms", TABLE_TIME),
            ("no_material", TABLE_TIME),
        ],
    )
    .shape(&[], &[("mode", GRANT_SPELL_MODES)]),
    k("healing", NOTES, true, &["name", "dice", "plus", "action"]),
    k("heroic_inspiration", PLAY, true, &["trigger"]),
    ku(
        "initiative_swap",
        COMBAT,
        true,
        &[],
        &[("with_ally", IMPLIED)],
    ),
    ku(
        "martial_arts",
        PLAY,
        true,
        &["die_table", "conditions", "bonus_unarmed_strike"],
        &[("dexterous_attacks", GAP), ("monk_weapons", GAP)],
    ),
    ku(
        "movement_mode",
        COMBAT,
        true,
        &["mode", "speed", "duration", "action"],
        &[("requires_resource", GAP)],
    ),
    k("mystic_arcanum", SPELLS, false, &["slots", "free_cast"]),
    ku(
        "pact_magic",
        SPELLS,
        false,
        &[],
        &[
            ("slots_recover", IMPLIED),
            ("slot_level_table", IMPLIED),
            ("slot_count_table", IMPLIED),
        ],
    ),
    k(
        "pact_weapon",
        NOTES_GENERIC,
        true,
        &[
            "action",
            "weapon_kinds",
            "ability",
            "damage_types",
            "is_spellcasting_focus",
        ],
    ),
    k("prepared_spell_change", SPELLS, false, &["when", "count"]),
    k(
        "proficiency_filter",
        NOTES_GENERIC,
        true,
        &["name", "weapon_category", "properties_any"],
    ),
    k(
        "regeneration",
        NOTES,
        true,
        &["condition", "amount", "timing"],
    ),
    k("replace_on_level", NOTES_GENERIC, true, &["scope", "count"]),
    ku(
        "resource",
        PLAY,
        true,
        &[
            "id",
            "name",
            "max",
            "recharge",
            "regain",
            "alt_regain",
            "replaces",
        ],
        &[
            ("counter", GAP),
            ("start", GAP),
            ("step", GAP),
            ("pool", GAP),
            ("cure_poison_cost", TABLE_TIME),
            ("kind_note", TABLE_TIME),
        ],
    )
    .shape(
        &["id", "max"],
        &[
            ("max.type", RESOURCE_MAX_TYPES),
            ("recharge[]", RESOURCE_RECHARGE),
        ],
    ),
    k("resource_cost", PLAY, true, &["resource", "cost"]).shape(&["resource"], &[]),
    k(
        "resource_regain",
        PLAY,
        true,
        &["resource", "on", "by", "up_to", "amount"],
    )
    .shape(&["resource"], &[]),
    ku(
        "ritual_casting",
        SPELLS,
        false,
        &["source"],
        &[("requires_prepared", GAP)],
    ),
    k(
        "roll_advantage",
        COMBAT,
        true,
        &["roll", "ability", "skill", "spell_class"],
    ),
    k(
        "save_advantage",
        COMBAT,
        true,
        &[
            "ability",
            "against_condition",
            "against_ability",
            "scope",
            "context",
            "unless_condition",
        ],
    ),
    k("save_advantage_check", COMBAT, true, &["context"]),
    k("save_dc", SPELLS, false, &["id", "formula"]),
    ku(
        "select_feat",
        NORMALIZE,
        false,
        &[
            "name",
            "category",
            "default",
            "alternatives",
            "distinct",
            "level",
            "requirements",
        ],
        &[("replace_on", REST_TIMING)],
    ),
    ku(
        "select_param",
        NORMALIZE,
        false,
        &[
            "name",
            "options",
            "distinct_per_instance",
            "level",
            "requirements",
        ],
        &[
            ("reselect_on", REST_TIMING),
            ("grants_resistance", GAP),
            ("note", TABLE_TIME),
        ],
    ),
    k(
        "select_size",
        NORMALIZE,
        false,
        &["name", "options", "level"],
    ),
    k("sense", COMBAT, true, &["sense", "range", "note"]),
    k("skill_bonus", ATTACK, true, &["skills", "value", "minimum"]),
    ku(
        "skill_bonus_unproficient",
        ATTACK,
        true,
        &["value"],
        &[("rounding", IMPLIED), ("applies_to", IMPLIED)],
    ),
    ku(
        "slot_recovery",
        NOTES,
        true,
        &[
            "pact_slots",
            "amount",
            "cast_time",
            "on",
            "max_combined_level",
            "max_slot_level",
            "uses",
            "recharge",
        ],
        &[("improved_at", GAP)],
    ),
    k(
        "spell_cast_alt_cost",
        NOTES_GENERIC,
        true,
        &["spell", "cost", "no_material"],
    ),
    k(
        "spell_free_cast",
        SPELLS,
        false,
        &["select", "uses", "recharge", "also_with_slots", "ability"],
    ),
    ku(
        "spell_list_extend",
        SPELLS,
        false,
        &["spell_class", "lists"],
        &[("for_prepared", GAP)],
    ),
    k(
        "spell_replace_on_level",
        NOTES_GENERIC,
        true,
        &["scope", "same_level"],
    ),
    ku(
        "spellbook",
        SPELLS,
        false,
        &[
            "initial_spells",
            "per_level",
            "copy_cost_gp_per_level",
            "copy_hours_per_level",
        ],
        &[("prepare_from", IMPLIED)],
    ),
    k(
        "toggle",
        PLAY,
        true,
        &[
            "id",
            "name",
            "activation",
            "duration",
            "effects",
            "requires_not_wearing",
            "blocks",
        ],
    ),
    ku(
        "weapon_mastery",
        NORMALIZE,
        false,
        &["name", "count", "filter", "level"],
        &[("change_on", REST_TIMING)],
    ),
];

/// The registry entry of a kind.
#[must_use]
pub fn spec(kind: &str) -> Option<&'static KindSpec> {
    KNOWN_KINDS.iter().find(|s| s.kind == kind)
}

/// Something the engine would ignore in a custom rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleProblem {
    /// No reader handles this kind.
    UnknownKind(String),
    /// The kind is read, but not this field.
    UnknownField {
        /// The rule kind.
        kind: String,
        /// The field name.
        field: String,
    },
}

impl RuleProblem {
    /// Stable diagnostic code (`content.unknown_rule_kind` / `content.unknown_rule_field`).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownKind(_) => "content.unknown_rule_kind",
            Self::UnknownField { .. } => "content.unknown_rule_field",
        }
    }
}

impl std::fmt::Display for RuleProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownKind(k) => write!(f, "rule kind {k:?} is not read by the engine"),
            Self::UnknownField { kind, field } => {
                write!(
                    f,
                    "field {field:?} of rule kind {kind:?} is not read by the engine"
                )
            }
        }
    }
}

/// What the engine would ignore in one custom rule (empty when everything is read).
#[must_use]
pub fn check_custom(c: &CustomRule) -> Vec<RuleProblem> {
    let Some(spec) = spec(&c.kind) else {
        return vec![RuleProblem::UnknownKind(c.kind.clone())];
    };
    c.fields
        .keys()
        .filter(|f| !spec.knows(f))
        .map(|f| RuleProblem::UnknownField {
            kind: c.kind.clone(),
            field: f.clone(),
        })
        .collect()
}

/// [`check_custom`] over the custom rules of a list (other rule kinds are typed and pass).
#[must_use]
pub fn check_rules(rules: &[Rule]) -> Vec<RuleProblem> {
    rules
        .iter()
        .filter_map(|r| match r {
            Rule::Custom(c) => Some(check_custom(c)),
            _ => None,
        })
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map, Value};

    fn custom(kind: &str, fields: Value) -> CustomRule {
        let Value::Object(fields) = fields else {
            unreachable!()
        };
        CustomRule {
            kind: kind.into(),
            fields,
        }
    }

    #[test]
    fn registry_is_sorted_and_unique() {
        for w in KNOWN_KINDS.windows(2) {
            assert!(w[0].kind < w[1].kind, "{} / {}", w[0].kind, w[1].kind);
        }
        for s in KNOWN_KINDS {
            for (f, _) in s.unmodelled {
                assert!(!s.reads(f), "{}.{f} is both read and unmodelled", s.kind);
            }
        }
    }

    #[test]
    fn unknown_kind_is_reported() {
        let c = custom("select.ability_distribution", json!({"pool": ["strength"]}));
        assert_eq!(
            check_custom(&c),
            vec![RuleProblem::UnknownKind(
                "select.ability_distribution".into()
            )]
        );
        assert_eq!(check_custom(&c)[0].code(), "content.unknown_rule_kind");
    }

    #[test]
    fn unknown_field_is_reported_and_known_ones_pass() {
        let ok = custom(
            "ability_distribution",
            json!({"pool": [], "cap": 20, "level": 4}),
        );
        assert!(check_custom(&ok).is_empty());
        let bad = custom("spell_free_cast", json!({"select": "S", "selection": "S"}));
        let problems = check_custom(&bad);
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].code(), "content.unknown_rule_field");
        assert!(problems[0].to_string().contains("selection"));
        let ungated = custom("spellbook", json!({"level": 2}));
        assert_eq!(check_custom(&ungated).len(), 1);
        let empty = CustomRule {
            kind: "evasion".into(),
            fields: Map::new(),
        };
        assert!(check_rules(&[Rule::Custom(empty)]).is_empty());
    }

    #[test]
    fn grant_spell_reads_slotless_casting_and_closes_its_modes() {
        let g = spec("grant_spell").unwrap();
        for f in [
            "free_cast",
            "also_with_slots",
            "cast_level",
            "resource_cost",
        ] {
            assert!(g.reads(f), "{f}");
        }
        assert!(!g.knows("cast"));
        let ok = serde_json::json!({"spell": "ID_X", "mode": "at_will_no_slot"});
        assert!(g.shape_problems(ok.as_object().unwrap()).is_empty());
        let bad = serde_json::json!({"spell": "ID_X", "mode": "innate"});
        let p = g.shape_problems(bad.as_object().unwrap());
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("`mode`"));
    }

    #[test]
    fn resource_shape_comes_from_the_registry() {
        let r = spec("resource").unwrap();
        let fields =
            serde_json::json!({"name": "Ember", "max": {"value": 2}, "recharge": ["dawn"]});
        let p = r.shape_problems(fields.as_object().unwrap());
        assert_eq!(p.len(), 3, "{p:?}");
        assert!(p[0].contains("has no `id`"));
        let ok = serde_json::json!({"id": "ember", "max": {"type": "fixed", "value": 2},
            "recharge": ["short_rest", "long_rest"]});
        assert!(r.shape_problems(ok.as_object().unwrap()).is_empty());
        let cost = spec("resource_cost").unwrap();
        assert_eq!(cost.shape_problems(&Map::new()), vec!["has no `resource`"]);
    }
}
