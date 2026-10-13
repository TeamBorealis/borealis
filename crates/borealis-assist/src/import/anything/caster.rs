//! Deterministic spellcasting builder. The model only declares a caster profile
//! ([`SpellcastingOutline`]); this module emits the spellcasting node, the slot-progression stats
//! and the level-gated cantrip / spell rules in exactly the shape the bundled SRD classes use
//! (2014: Wizard, Paladin, Warlock; 2024: Cleric, Ranger, Warlock).

use serde_json::{json, Map, Value};

use super::types::{Edition, SpellcastingOutline};

const N: usize = 20;
type Row = [u32; 9];

/// Full-caster slots by caster level 1..20.
const FULL: [Row; N] = [
    [2, 0, 0, 0, 0, 0, 0, 0, 0],
    [3, 0, 0, 0, 0, 0, 0, 0, 0],
    [4, 2, 0, 0, 0, 0, 0, 0, 0],
    [4, 3, 0, 0, 0, 0, 0, 0, 0],
    [4, 3, 2, 0, 0, 0, 0, 0, 0],
    [4, 3, 3, 0, 0, 0, 0, 0, 0],
    [4, 3, 3, 1, 0, 0, 0, 0, 0],
    [4, 3, 3, 2, 0, 0, 0, 0, 0],
    [4, 3, 3, 3, 1, 0, 0, 0, 0],
    [4, 3, 3, 3, 2, 0, 0, 0, 0],
    [4, 3, 3, 3, 2, 1, 0, 0, 0],
    [4, 3, 3, 3, 2, 1, 0, 0, 0],
    [4, 3, 3, 3, 2, 1, 1, 0, 0],
    [4, 3, 3, 3, 2, 1, 1, 0, 0],
    [4, 3, 3, 3, 2, 1, 1, 1, 0],
    [4, 3, 3, 3, 2, 1, 1, 1, 0],
    [4, 3, 3, 3, 2, 1, 1, 1, 1],
    [4, 3, 3, 3, 3, 1, 1, 1, 1],
    [4, 3, 3, 3, 3, 2, 1, 1, 1],
    [4, 3, 3, 3, 3, 2, 2, 1, 1],
];
const PACT_COUNT: [u32; N] = [1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4];
const PACT_LEVEL: [u32; N] = [1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5];
// Cumulative defaults (SRD class tables) used when the profile gives none.
const CANTRIPS_FULL: [u32; N] = [3, 3, 3, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5];
const CANTRIPS_PACT: [u32; N] = [2, 2, 2, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4];
const CANTRIPS_THIRD: [u32; N] = [0, 0, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3];
const PREPARED_FULL: [u32; N] = [
    4, 5, 6, 7, 9, 10, 11, 12, 14, 15, 16, 16, 17, 17, 18, 18, 19, 20, 21, 22,
];
const PREPARED_HALF: [u32; N] = [
    2, 3, 4, 5, 6, 6, 7, 7, 9, 9, 10, 10, 11, 11, 12, 12, 14, 14, 15, 15,
];
const PREPARED_PACT: [u32; N] = [
    2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15,
];
const KNOWN_FULL: [u32; N] = [
    2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 12, 13, 13, 14, 14, 15, 15, 15, 15,
];
const KNOWN_HALF: [u32; N] = [
    0, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11,
];
const KNOWN_THIRD: [u32; N] = [
    0, 0, 3, 4, 4, 4, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12,
];

/// How the class casts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CasterKind {
    /// Full caster (Wizard).
    Full,
    /// Half caster (Paladin, Ranger).
    Half,
    /// Third caster (Eldritch Knight).
    Third,
    /// Pact magic (Warlock).
    Pact,
    /// Does not cast.
    None,
}

/// Parse the free-text kind the model wrote.
pub fn caster_kind(p: &SpellcastingOutline) -> CasterKind {
    let k = p.progression.to_lowercase();
    if k.contains("none") || k.trim() == "no" {
        CasterKind::None
    } else if k.contains("pact") {
        CasterKind::Pact
    } else if k.contains("third") || k.contains("1/3") {
        CasterKind::Third
    } else if k.contains("half") || k.contains("1/2") {
        CasterKind::Half
    } else {
        CasterKind::Full
    }
}

/// Is this feature name the class's spellcasting feature?
pub fn is_caster_feature(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("spellcasting") || n.contains("pact magic")
}

/// What the builder produces for one class.
#[derive(Debug, Clone)]
pub struct BuiltCaster {
    /// The `spellcasting` node of the Spellcasting class feature.
    pub node: Value,
    /// Rules of the Spellcasting class feature (stats, selects, grants).
    pub rules: Vec<Value>,
    /// 2024: `data.spellcasting` of the class.
    pub class_spellcasting: Option<Value>,
    /// 2024: spellcasting tables for `data.progression`.
    pub progression: Map<String, Value>,
}

/// Slots by class level (index 0 = level 1), `pact` giving the pact pool instead.
pub fn slot_rows(kind: CasterKind, edition: Edition) -> [Row; N] {
    let mut rows = [[0; 9]; N];
    for (i, row) in rows.iter_mut().enumerate() {
        let l = i + 1;
        let cl = match kind {
            CasterKind::Full => l,
            CasterKind::Half if edition == Edition::Y2014 && l < 2 => 0,
            CasterKind::Half => l.div_ceil(2),
            CasterKind::Third if l < 3 => 0,
            CasterKind::Third => l.div_ceil(3),
            CasterKind::Pact => {
                row[PACT_LEVEL[i] as usize - 1] = PACT_COUNT[i];
                continue;
            }
            CasterKind::None => 0,
        };
        if cl > 0 {
            *row = FULL[cl - 1];
        }
    }
    rows
}

fn table(given: &Option<Vec<u32>>, default: &[u32; N]) -> [u32; N] {
    match given {
        Some(v) if v.len() == N => {
            let mut out = [0; N];
            out.copy_from_slice(v);
            out
        }
        _ => *default,
    }
}

fn stat(name: &str, value: impl ToString, level: u32, leveled_first: bool) -> Value {
    if level == 1 && !leveled_first {
        json!({"kind": "stat", "name": name, "value": value.to_string()})
    } else {
        json!({"kind": "stat", "name": name, "value": value.to_string(), "level": level})
    }
}

/// Delta stat rules that rebuild a cumulative per-level table.
fn deltas(name: &str, values: &[u32; N], leveled_first: bool, out: &mut Vec<Value>) {
    let mut prev = 0i64;
    for (i, v) in values.iter().enumerate() {
        let d = i64::from(*v) - prev;
        if d != 0 {
            out.push(stat(name, d, i as u32 + 1, leveled_first));
        }
        prev = i64::from(*v);
    }
}

fn increments(values: &[u32; N]) -> Vec<(u32, u32)> {
    let mut prev = 0;
    let mut out = Vec::new();
    for (i, v) in values.iter().enumerate() {
        if *v > prev {
            out.push((i as u32 + 1, *v - prev));
        }
        prev = prev.max(*v);
    }
    out
}

/// Slots per level for each slot level 1..9.
fn columns(rows: &[Row; N]) -> Vec<[u32; N]> {
    (0..9)
        .map(|k| std::array::from_fn(|i| rows.get(i).map_or(0, |r| r[k])))
        .collect()
}

fn max_slot_level(row: &Row) -> usize {
    row.iter().rposition(|s| *s > 0).map_or(0, |i| i + 1)
}

fn spell_levels(max: usize) -> String {
    if max <= 1 {
        "1".to_string()
    } else {
        format!(
            "({})",
            (1..=max)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("||")
        )
    }
}

fn cap(s: &str) -> String {
    let mut c = s.trim().chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

/// 2014 prepared-count rules from a formula such as `mod + level` / `mod + half`.
fn prepare_rules(
    group: &str,
    ability: &str,
    kind: CasterKind,
    lc: &str,
    formula: Option<&str>,
    out: &mut Vec<Value>,
) {
    let default = match kind {
        CasterKind::Half => "mod + half",
        CasterKind::Third => "mod + third",
        _ => "mod + level",
    };
    let f = formula
        .filter(|f| !f.trim().is_empty())
        .unwrap_or(default)
        .to_lowercase();
    let mut rules = Vec::new();
    for token in f.split('+').map(str::trim) {
        let value = match token {
            "mod" | "modifier" | "ability" => format!("{ability}:modifier"),
            "level" => format!("level:{lc}"),
            "half" | "half level" | "level/2" => format!("level:{lc}:half"),
            "third" | "third level" | "level/3" => {
                let thirds = std::array::from_fn(|i| (i as u32 + 1) / 3);
                deltas(group, &thirds, true, &mut rules);
                continue;
            }
            n => match n.parse::<i32>() {
                Ok(v) => v.to_string(),
                Err(_) => continue,
            },
        };
        rules.push(json!({"kind": "stat", "name": group, "value": value}));
    }
    if rules.is_empty() {
        return prepare_rules(group, ability, kind, lc, Some(default), out);
    }
    out.extend(rules);
}

fn multiclass_grant(kind: CasterKind, edition: Edition) -> &'static str {
    match (kind, edition) {
        (CasterKind::Pact, _) => "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_SOLO",
        (CasterKind::Third, _) => "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_THIRD",
        (CasterKind::Half, Edition::Y2014) => {
            "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF"
        }
        (CasterKind::Half, Edition::Y2024) => {
            "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF_UP"
        }
        _ => "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_FULL",
    }
}

/// Build everything the engine needs for the profile, or `None` when the class does not cast.
pub fn build_caster(
    p: &SpellcastingOutline,
    class_name: &str,
    edition: Edition,
) -> Option<BuiltCaster> {
    let kind = caster_kind(p);
    if kind == CasterKind::None {
        return None;
    }
    let lc = class_name.to_lowercase();
    let ability = p.ability.trim().to_lowercase();
    let list = p
        .list
        .clone()
        .filter(|l| !l.trim().is_empty())
        .unwrap_or_else(|| class_name.to_string());
    let known_mode =
        kind != CasterKind::Pact && (p.known == Some(true) || p.prepared == Some(false));
    let is_2014 = edition == Edition::Y2014;
    let rows = slot_rows(kind, edition);
    let (cantrip_default, prepared_default, known_default) = match kind {
        CasterKind::Full => (CANTRIPS_FULL, PREPARED_FULL, KNOWN_FULL),
        CasterKind::Half => ([0; N], PREPARED_HALF, KNOWN_HALF),
        CasterKind::Third => (CANTRIPS_THIRD, KNOWN_THIRD, KNOWN_THIRD),
        _ => (CANTRIPS_PACT, PREPARED_PACT, PREPARED_PACT),
    };
    let cantrips = table(&p.cantrips_by_level, &cantrip_default);
    let counts = table(
        &p.spells_known_by_level,
        if is_2014 {
            &known_default
        } else {
            &prepared_default
        },
    );
    let slot_name = |k: usize| format!("{lc}:spellcasting:slots:{k}");
    let mut rules: Vec<Value> = Vec::new();

    // --- slots ---
    if kind == CasterKind::Pact && is_2014 {
        let count_name = format!("{lc}:spellcasting:slots:count");
        deltas(&count_name, &PACT_COUNT, true, &mut rules);
        for lvl in [1, 3, 5, 7, 9] {
            rules.push(stat(&format!("{lc}:spellcasting:slot"), 1, lvl, true));
        }
        for k in 1..=5u32 {
            let on = 2 * k - 1;
            rules.push(stat(&slot_name(k as usize), &count_name, on, true));
            if k < 5 {
                rules.push(stat(
                    &slot_name(k as usize),
                    format!("-{count_name}"),
                    on + 2,
                    true,
                ));
            }
        }
    } else {
        for (k, col) in columns(&rows).iter().enumerate() {
            deltas(&slot_name(k + 1), col, is_2014, &mut rules);
        }
    }

    // --- cantrips and spells ---
    let cantrip_name = |l: u32| {
        if is_2014 {
            format!("Cantrip ({class_name})")
        } else {
            format!("Cantrip ({class_name} {l})")
        }
    };
    for (level, n) in increments(&cantrips) {
        let mut sel = json!({"kind": "select", "type": "Spell", "name": cantrip_name(level),
            "supports": "$(spellcasting:list),0", "spellcasting": class_name});
        if n > 1 || is_2014 {
            sel["number"] = json!(n);
        }
        if level > 1 || is_2014 {
            sel["level"] = json!(level);
        }
        if !is_2014 {
            sel["allow_replace"] = json!(true);
        }
        rules.push(sel);
    }
    if is_2014 {
        if cantrips.iter().any(|c| *c > 0) {
            deltas(
                &format!("{lc}:spellcasting:cantrips known"),
                &cantrips,
                true,
                &mut rules,
            );
        }
        if known_mode || kind == CasterKind::Pact {
            deltas(
                &format!("{lc}:spellcasting:spells known"),
                &counts,
                true,
                &mut rules,
            );
            for (level, n) in increments(&counts) {
                let max = max_slot_level(&rows[level as usize - 1]).max(1);
                let mut sel = json!({"kind": "select", "type": "Spell", "name": format!("Spell ({class_name})"),
                    "number": n, "level": level,
                    "supports": format!("$(spellcasting:list),{}", spell_levels(max)), "spellcasting": class_name});
                if kind == CasterKind::Pact {
                    sel["allow_replace"] = json!(true);
                }
                rules.push(sel);
            }
        } else {
            prepare_rules(
                &format!("{lc}:spellcasting:prepare"),
                &ability,
                kind,
                &lc,
                p.prepared_formula.as_deref(),
                &mut rules,
            );
        }
        if p.ritual == Some(true) {
            rules.push(
                json!({"kind": "grant", "id": "ID_INTERNAL_RITUAL_CASTING", "type": "Grants"}),
            );
        }
    } else if kind == CasterKind::Pact {
        rules.push(
            json!({"kind": "pact_magic", "slots_recover": ["short_rest", "long_rest"],
            "slot_level_table": "pact_slot_level", "slot_count_table": "pact_slots"}),
        );
        rules.push(json!({"kind": "prepared_spell_change", "when": "gain_level", "count": "one"}));
    } else {
        let count = if kind == CasterKind::Full {
            "any"
        } else {
            "one"
        };
        rules.push(json!({"kind": "prepared_spell_change", "when": "long_rest", "count": count}));
    }
    rules.push(
        json!({"kind": "grant", "id": multiclass_grant(kind, edition), "type": "Grants",
        "requirements": "ID_INTERNAL_GRANT_MULTICLASS"}),
    );

    // --- node ---
    let mut node =
        json!({"name": class_name, "ability": cap(&ability), "list": {"supports": list}});
    let prepares = !known_mode && (kind != CasterKind::Pact || !is_2014);
    if prepares {
        node["prepare"] = json!(true);
        if kind != CasterKind::Pact || !is_2014 {
            node["list"]["known"] = json!(true);
        }
    }
    if kind == CasterKind::Pact || !is_2014 {
        node["allow_replace"] = json!(true);
    }

    // --- 2024 class data ---
    let (class_spellcasting, progression) = if is_2014 {
        (None, Map::new())
    } else {
        let mut prog = Map::new();
        if cantrips.iter().any(|c| *c > 0) {
            prog.insert("cantrips".into(), json!(cantrips));
        }
        prog.insert("prepared".into(), json!(counts));
        if kind == CasterKind::Pact {
            prog.insert("pact_slots".into(), json!(PACT_COUNT));
            prog.insert("pact_slot_level".into(), json!(PACT_LEVEL));
        } else {
            let top = rows.iter().map(max_slot_level).max().unwrap_or(0);
            for (k, col) in columns(&rows).iter().enumerate().take(top) {
                prog.insert(format!("slot_{}", k + 1), json!(col));
            }
        }
        let sc = json!({"ability": ability, "list": list, "model": if kind == CasterKind::Pact { "pact" } else { "prepared" },
            "replace": if kind == CasterKind::Pact { "level" } else { "long_rest" },
            "multiclass_slots_grant": multiclass_grant(kind, edition),
            "has_cantrips": cantrips.iter().any(|c| *c > 0)});
        (Some(sc), prog)
    };
    Some(BuiltCaster {
        node,
        rules,
        class_spellcasting,
        progression,
    })
}

/// Drop the spellcasting rules a model wrote, so the built ones replace them.
pub fn is_model_caster_rule(r: &Value) -> bool {
    let kind = r.get("kind").and_then(Value::as_str).unwrap_or("");
    let name = r.get("name").and_then(Value::as_str).unwrap_or("");
    let id = r.get("id").and_then(Value::as_str).unwrap_or("");
    match kind {
        "stat" => name.contains(":spellcasting:"),
        "select" => r.get("type").and_then(Value::as_str) == Some("Spell"),
        "pact_magic" | "prepared_spell_change" | "spellbook" => true,
        "grant" => {
            id.starts_with("ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING")
                || id == "ID_INTERNAL_RITUAL_CASTING"
        }
        _ => false,
    }
}
