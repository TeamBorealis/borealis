//! The homebrew creator's spellcasting section (spec 4.6): the rules each part of the section writes, in exactly the
//! shapes the bundled SRD classes store them (2014: SRD 5.1 as Aurora writes it; 2024: SRD 5.2.1).
//!
//! * **Spell slots** ([`SlotTable`]): full, half (rounded up: 2024; rounded down, no slots at level 1: 2014), third
//!   and pact magic progressions, or a custom 20 x 9 table, as level-gated `<name>:spellcasting:slots:N` stats (2014
//!   pact magic: the `slots:count` / `slot` stats; 2024 pact magic: the slot stats and the `pact_magic` rule).
//! * **Multiclass slots**: the `ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_*` grant, gated on
//!   `ID_INTERNAL_GRANT_MULTICLASS`.
//! * **Counts per level**: prepared spells (2024: `<name>:spellcasting:prepare` stats per level; 2014: the ability
//!   modifier plus the class level or half of it), cantrips known (the cantrip choices, and 2014's `cantrips known`
//!   stats), spells known (the spell choices and the `spells known` stats).
//! * **Changing prepared spells** (2024): the `prepared_spell_change` rule.
//!
//! The spelling the SRD packs differ in (a space after the comma of a spell filter, the name of the spell choices)
//! travels in [`Spelling`], so a stored class writes back byte for byte; a new section is written in its edition's
//! SRD spelling. [`super::build_caster`] (the import pipeline's builder) is unchanged.

use super::{columns, deltas, increments, max_slot_level, slot_rows, spell_levels, stat, N};
use super::{CasterKind, PACT_COUNT, PACT_LEVEL};
use crate::Edition;
use serde_json::{json, Value};

/// Slots per class level (index 0 = level 1) for each slot level 1..9.
pub type SlotGrid = [[u32; 9]; 20];

/// How a section's spell slots progress (the caster type, spec 4.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotTable {
    /// Full caster (Wizard).
    Full,
    /// Half caster, rounded up: slots from level 1 (2024 Paladin, Ranger).
    HalfUp,
    /// Half caster, rounded down: slots from level 2 (2014 Paladin, Ranger).
    HalfDown,
    /// Third caster: slots from level 3.
    Third,
    /// Pact magic (Warlock): a few slots, all of one level, back on a short rest.
    Pact,
    /// Any table.
    Custom(Box<SlotGrid>),
}

/// The caster types a stored section is checked against, in order.
pub const STANDARD_TABLES: [SlotTable; 5] = [
    SlotTable::Full,
    SlotTable::HalfUp,
    SlotTable::HalfDown,
    SlotTable::Third,
    SlotTable::Pact,
];

impl SlotTable {
    /// The form value (`full`, `halfUp`, `halfDown`, `third`, `pact`, `custom`).
    #[must_use]
    pub fn key(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::HalfUp => "halfUp",
            Self::HalfDown => "halfDown",
            Self::Third => "third",
            Self::Pact => "pact",
            Self::Custom(_) => "custom",
        }
    }

    /// The table for a form value (`custom` takes `grid`).
    #[must_use]
    pub fn from_key(key: &str, grid: Option<SlotGrid>) -> Option<Self> {
        Some(match key {
            "full" => Self::Full,
            "halfUp" => Self::HalfUp,
            "halfDown" => Self::HalfDown,
            "third" => Self::Third,
            "pact" => Self::Pact,
            "custom" => Self::Custom(Box::new(grid?)),
            _ => return None,
        })
    }

    /// Slots per class level. Pact magic gives its pool: the slots at the one level they are cast at.
    #[must_use]
    pub fn grid(&self) -> SlotGrid {
        match self {
            Self::Full => slot_rows(CasterKind::Full, Edition::Y2024),
            Self::HalfUp => slot_rows(CasterKind::Half, Edition::Y2024),
            Self::HalfDown => slot_rows(CasterKind::Half, Edition::Y2014),
            Self::Third => slot_rows(CasterKind::Third, Edition::Y2024),
            Self::Pact => slot_rows(CasterKind::Pact, Edition::Y2024),
            Self::Custom(g) => **g,
        }
    }

    /// The multiclass slot grant this caster type counts as (`ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_<x>`):
    /// pact magic and a custom table keep their own slots (`SOLO`).
    #[must_use]
    pub fn multiclass(&self) -> &'static str {
        match self {
            Self::Full => "FULL",
            Self::HalfUp => "HALF_UP",
            Self::HalfDown => "HALF",
            Self::Third => "THIRD",
            Self::Pact | Self::Custom(_) => "SOLO",
        }
    }
}

/// The multiclass slot grants of the core rules (`content/core/multiclass.json`), as `(suffix, form value)`.
pub const MULTICLASS_GRANTS: [(&str, &str); 9] = [
    ("FULL", "full"),
    ("HALF_UP", "halfUp"),
    ("HALF", "half"),
    ("THIRD_UP", "thirdUp"),
    ("THIRD", "third"),
    ("FOURTH_UP", "fourthUp"),
    ("FOURTH", "fourth"),
    ("FIFTH", "fifth"),
    ("SOLO", "solo"),
];

/// Prefix of the multiclass slot grants' ids.
pub const MULTICLASS_GRANT_PREFIX: &str = "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_";
/// The grant every multiclass section gives (the slot grant is gated on it).
pub const MULTICLASS_GATE: &str = "ID_INTERNAL_GRANT_MULTICLASS";

/// The spelling of the section's choices, as stored (the SRD packs differ in it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spelling {
    /// `"$(spellcasting:list), 0"` rather than `"$(spellcasting:list),0"`.
    pub spaced: bool,
    /// The spell choices' name before the class (`Spell`, `Spell Known`, `Spells Known`).
    pub word: String,
    /// The spell choices offer `$(spellcasting:slots)` (the levels the class has slots for) rather than the levels
    /// written out (`(1||2)`).
    pub dynamic: bool,
}

impl Spelling {
    /// A new section's spelling: the edition's SRD one.
    #[must_use]
    pub fn new(edition: Edition) -> Self {
        Self {
            spaced: edition == Edition::Y2024,
            word: "Spell".into(),
            dynamic: false,
        }
    }

    fn sep(&self) -> &'static str {
        if self.spaced {
            ", "
        } else {
            ","
        }
    }
}

/// The stat names of the section, for the spellcasting name `name` (`Wizard` -> `wizard:spellcasting:...`).
#[must_use]
pub fn stat_name(name: &str, part: &str) -> String {
    format!("{}:spellcasting:{part}", name.to_lowercase())
}

/// The slot rules of a caster type.
#[must_use]
pub fn slot_rules(table: &SlotTable, name: &str, edition: Edition) -> Vec<Value> {
    let lc = name.to_lowercase();
    let mut rules = Vec::new();
    let is_2014 = edition == Edition::Y2014;
    if *table == SlotTable::Pact && is_2014 {
        let count = format!("{lc}:spellcasting:slots:count");
        deltas(&count, &PACT_COUNT, true, &mut rules);
        for lvl in [1, 3, 5, 7, 9] {
            rules.push(stat(&format!("{lc}:spellcasting:slot"), 1, lvl, true));
        }
        for k in 1..=5u32 {
            let on = 2 * k - 1;
            let slot = format!("{lc}:spellcasting:slots:{k}");
            rules.push(stat(&slot, &count, on, true));
            if k < 5 {
                rules.push(stat(&slot, format!("-{count}"), on + 2, true));
            }
        }
        return rules;
    }
    for (k, col) in columns(&table.grid()).iter().enumerate() {
        deltas(
            &format!("{lc}:spellcasting:slots:{}", k + 1),
            col,
            is_2014,
            &mut rules,
        );
    }
    if *table == SlotTable::Pact {
        rules.push(pact_magic_rule());
    }
    rules
}

/// The 2024 `pact_magic` rule (its slots are the class table's `pact_slots` and `pact_slot_level` columns).
#[must_use]
pub fn pact_magic_rule() -> Value {
    json!({"kind": "pact_magic", "slots_recover": ["short_rest", "long_rest"],
        "slot_level_table": "pact_slot_level", "slot_count_table": "pact_slots"})
}

/// The multiclass slot grant with the id suffix `suffix` (`FULL`, `HALF_UP`...).
#[must_use]
pub fn multiclass_rule(suffix: &str) -> Value {
    json!({"kind": "grant", "id": format!("{MULTICLASS_GRANT_PREFIX}{suffix}"), "type": "Grants",
        "requirements": MULTICLASS_GATE})
}

/// A count per level written as level-gated stats that add up to it (2014 gates level 1 too).
#[must_use]
pub fn count_rules(stat: &str, values: &[u32; N], edition: Edition) -> Vec<Value> {
    let mut out = Vec::new();
    deltas(stat, values, edition == Edition::Y2014, &mut out);
    out
}

/// 2014 prepared spells: the ability modifier plus the class level (`half`: half of it, rounded down).
#[must_use]
pub fn prepare_formula_rules(name: &str, ability: &str, half: bool) -> Vec<Value> {
    let lc = name.to_lowercase();
    let group = format!("{lc}:spellcasting:prepare");
    let level = if half {
        format!("level:{lc}:half")
    } else {
        format!("level:{lc}")
    };
    vec![
        json!({"kind": "stat", "name": group, "value": format!("{}:modifier", ability.to_lowercase())}),
        json!({"kind": "stat", "name": group, "value": level}),
    ]
}

/// The name of a choice of the section: 2014 `"<word> (<Name>)"`, 2024 `"<word> (<Name> <level>)"`.
fn choice_name(word: &str, name: &str, level: u32, edition: Edition) -> String {
    match edition {
        Edition::Y2014 => format!("{word} ({name})"),
        Edition::Y2024 => format!("{word} ({name} {level})"),
    }
}

/// A spell choice of the section (2014 writes `number` and `level` always; 2024 leaves out 1 and level 1).
fn spell_select(
    name: String,
    n: u32,
    level: u32,
    supports: String,
    class: &str,
    edition: Edition,
) -> Value {
    let mut sel = json!({"kind": "select", "type": "Spell", "name": name});
    let is_2014 = edition == Edition::Y2014;
    if n > 1 || is_2014 {
        sel["number"] = json!(n);
    }
    if level > 1 || is_2014 {
        sel["level"] = json!(level);
    }
    sel["supports"] = json!(supports);
    sel["spellcasting"] = json!(class);
    sel
}

/// Cantrips known per level: a cantrip choice at each level the count goes up (2024: replaceable), and with
/// `statistic` the `cantrips known` stats.
#[must_use]
pub fn cantrip_rules(
    name: &str,
    values: &[u32; N],
    statistic: bool,
    spelling: &Spelling,
    edition: Edition,
) -> Vec<Value> {
    let mut rules = Vec::new();
    if statistic {
        rules.extend(count_rules(
            &stat_name(name, "cantrips known"),
            values,
            edition,
        ));
    }
    for (level, n) in increments(values) {
        let supports = format!("$(spellcasting:list){}0", spelling.sep());
        let mut sel = spell_select(
            choice_name("Cantrip", name, level, edition),
            n,
            level,
            supports,
            name,
            edition,
        );
        if edition == Edition::Y2024 {
            sel["allow_replace"] = json!(true);
        }
        rules.push(sel);
    }
    rules
}

/// Spells known per level: the `spells known` stats (with `statistic`) and a spell choice at each level the count goes
/// up, of the levels the class has slots for (`grid`); `replace`: a pick can be swapped on a later level.
#[must_use]
pub fn known_rules(
    name: &str,
    values: &[u32; N],
    statistic: bool,
    replace: bool,
    spelling: &Spelling,
    grid: &SlotGrid,
    edition: Edition,
) -> Vec<Value> {
    let mut rules = Vec::new();
    if statistic {
        rules.extend(count_rules(
            &stat_name(name, "spells known"),
            values,
            edition,
        ));
    }
    for (level, n) in increments(values) {
        let levels = if spelling.dynamic {
            "$(spellcasting:slots)".to_string()
        } else {
            spell_levels(max_slot_level(&grid[level as usize - 1]).max(1))
        };
        let supports = format!("$(spellcasting:list){}{levels}", spelling.sep());
        let mut sel = spell_select(
            choice_name(&spelling.word, name, level, edition),
            n,
            level,
            supports,
            name,
            edition,
        );
        if replace {
            sel["allow_replace"] = json!(true);
        }
        rules.push(sel);
    }
    rules
}

/// When a 2024 caster changes its prepared spells: after a long rest (any of them, or one) or on gaining a level (one).
pub const PREPARED_CHANGES: [(&str, &str, &str); 3] = [
    ("longRestAny", "long_rest", "any"),
    ("longRestOne", "long_rest", "one"),
    ("levelOne", "gain_level", "one"),
];

/// The `prepared_spell_change` rule of a form value (`longRestAny`...).
#[must_use]
pub fn prepared_change_rule(key: &str) -> Option<Value> {
    PREPARED_CHANGES.iter().find(|(k, _, _)| *k == key).map(
        |(_, when, count)| json!({"kind": "prepared_spell_change", "when": when, "count": count}),
    )
}

/// A 2024 class's spellcasting columns of its class table (`data.progression`), as the SRD classes have them:
/// `cantrips` (when it has any), `prepared` (a caster that prepares), `slot_1`.. up to its highest slot level, or
/// pact magic's `pact_slots` and `pact_slot_level`.
#[must_use]
pub fn progression_columns(
    table: Option<&SlotTable>,
    cantrips: &[u32; N],
    prepared: Option<&[u32; N]>,
) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    if cantrips.iter().any(|c| *c > 0) {
        out.push(("cantrips".to_string(), json!(cantrips)));
    }
    if let Some(p) = prepared {
        out.push(("prepared".to_string(), json!(p)));
    }
    match table {
        Some(SlotTable::Pact) => {
            out.push(("pact_slots".to_string(), json!(PACT_COUNT)));
            out.push(("pact_slot_level".to_string(), json!(PACT_LEVEL)));
        }
        Some(t) => {
            let grid = t.grid();
            let top = grid.iter().map(max_slot_level).max().unwrap_or(0);
            for (k, col) in columns(&grid).iter().enumerate().take(top) {
                out.push((format!("slot_{}", k + 1), json!(col)));
            }
        }
        None => {}
    }
    out
}

/// The class table columns [`progression_columns`] writes (the others are the class's own counters).
#[must_use]
pub fn is_progression_column(key: &str) -> bool {
    matches!(
        key,
        "cantrips" | "prepared" | "pact_slots" | "pact_slot_level"
    ) || key
        .strip_prefix("slot_")
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=9).contains(&n))
}

#[cfg(test)]
#[path = "section_tests.rs"]
mod tests;
