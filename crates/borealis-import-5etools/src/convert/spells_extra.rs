//! `additionalSpells`: spells granted or chosen by races, feats, backgrounds, classes, subclasses
//! and optional features.
//!
//! The array lists alternatives: one group applies directly, several groups are options the
//! owner offers (named or not). Inside a group, `known` / `prepared` / `innate` are keyed by
//! character or class level (`"3"`, `"_"` = always) or by spell slot level (`"s2"` = once the
//! character can cast 2nd-level spells), optionally split by casting frequency (`daily`, `rest`,
//! `will`, `ritual`, `resource`); `expanded` adds spells to the spell list of a class.
//!
//! Mapping: fixed spells become `Spell` grants (always prepared unless `known`); limited or
//! special casting becomes a `grant_spell` rule with a `mode` and `free_cast` / `resource_cost`
//! / `cast_level` (the bundled representation); choices become
//! `Spell` selects (with a `spell_free_cast` rule when the pick has limited uses); expanded
//! lists and slot-keyed always-prepared spells become spellcasting *extensions*
//! ([`Casting::expanded`], placed by [`super::spell_blocks`]).

use super::build::{custom, select};
use super::Shared;
use crate::core_ids;
use crate::text::RefKind;
use crate::util::{id_fragment, title_case};
use borealis_content::{GrantRule, Rule, Ruleset, SpellcastingList};
use serde_json::{json, Map, Value};

/// The spellcasting ability of a group.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SpellAbility {
    /// Not given (class spells, or rituals and other spells without attacks).
    #[default]
    Unspecified,
    /// A fixed ability (full name, `Charisma`).
    Fixed(String),
    /// The player chooses among these abilities (full names).
    Choose(Vec<String>),
}

/// How the spells of one group are cast and which lists they extend.
#[derive(Debug, Clone, Default)]
pub struct Casting {
    /// Spellcasting name the rules use (`grant/select spellcasting`).
    pub label: String,
    /// Ability of the group.
    pub ability: SpellAbility,
    /// Spell list extensions: (class or character level from which they apply, entry).
    pub expanded: Vec<(u32, SpellcastingList)>,
}

/// One alternative group of additional spells (a lineage, legacy, land or patron option).
#[derive(Debug, Default)]
pub struct SpellOption {
    /// Option name.
    pub name: String,
    /// Rules of the option.
    pub rules: Vec<Rule>,
    /// Casting of the option.
    pub casting: Casting,
}

/// Result of converting `additionalSpells`.
#[derive(Debug, Default)]
pub struct SpellRules {
    /// Rules of the single group (apply directly).
    pub direct: Vec<Rule>,
    /// Casting of the single group.
    pub casting: Casting,
    /// Alternative groups (the owner offers a choice between them).
    pub options: Vec<SpellOption>,
    /// Losses.
    pub partial: Vec<String>,
}

/// A parsed spell reference like `fireball|xphb#2`.
#[derive(Debug, PartialEq, Eq)]
pub struct SpellRef {
    /// Spell name (lower case as written).
    pub name: String,
    /// Source abbreviation (defaults to PHB).
    pub source: String,
    /// Cast at this level.
    pub cast_level: Option<u32>,
    /// Flagged `#c`.
    pub cantrip: bool,
}

/// Parse a spell reference string.
#[must_use]
pub fn parse_spell_ref(text: &str) -> SpellRef {
    let (main, suffix) = text.split_once('#').map_or((text, ""), |(m, s)| (m, s));
    let (name, source) = main.split_once('|').map_or((main, "PHB"), |(n, s)| (n, s));
    let source = if source.trim().is_empty() {
        "PHB"
    } else {
        source.trim()
    };
    SpellRef {
        name: name.trim().to_string(),
        source: source.to_uppercase(),
        cast_level: suffix.parse().ok(),
        cantrip: suffix == "c",
    }
}

/// Convert a filter like `level=0|class=Wizard` to a supports expression.
#[must_use]
pub fn filter_to_supports(filter: &str) -> Option<String> {
    let mut parts = Vec::new();
    for pair in filter.split('|') {
        let (k, v) = pair.split_once('=')?;
        let values: Vec<String> = v
            .split(';')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect();
        if values.is_empty() || values.iter().any(|x| x.starts_with('!')) {
            return None;
        }
        let tags: Vec<String> = match k.trim() {
            "level" => values,
            // class names are matched against spell `supports`, which use the class's own
            // spelling ("Cleric"); filters may be written in lower case
            "class" => values.iter().map(|c| crate::util::title_case(c)).collect(),
            "school" => values.iter().map(|c| school_name(c).to_string()).collect(),
            // implicit spell tags of the engine (`isRitual`, component setters)
            k if k.eq_ignore_ascii_case("components & miscellaneous") => values
                .iter()
                .map(|c| component_tag(c).map(str::to_string))
                .collect::<Option<Vec<_>>>()?,
            _ => return None,
        };
        parts.push(if tags.len() == 1 {
            tags[0].clone()
        } else {
            format!("({})", tags.join("||"))
        });
    }
    (!parts.is_empty()).then(|| parts.join("&&"))
}

/// Implicit support tag of a `components & miscellaneous` filter value.
fn component_tag(value: &str) -> Option<&'static str> {
    Some(match value.to_lowercase().as_str() {
        "ritual" => "Ritual",
        "concentration" => "ID_INTERNAL_SUPPORT_CONCENTRATION",
        "v" => "ID_INTERNAL_SUPPORT_VERBAL",
        "s" => "ID_INTERNAL_SUPPORT_SOMATIC",
        "m" => "ID_INTERNAL_SUPPORT_MATERIAL",
        _ => return None,
    })
}

/// Whether a spell of `spell` edition is offered to an owner of `owner` edition by default:
/// 2024 characters take 2014 spells, 2014 characters do not take 2024 spells (the engine
/// profiles' default cross-edition setting), so a 2014 owner's explicit list keeps only them.
fn offered_to(owner: Ruleset, spell: Option<Ruleset>) -> bool {
    owner == Ruleset::Dnd2024 || spell != Some(Ruleset::Dnd2024)
}

/// Ids of the imported spells a filter selects for an owner of edition `owner`, for filters
/// the supports grammar cannot express (`spell attack=m;r`): an ID-range supports, as Aurora
/// lists explicit spells. `None` when a key is not understood, an empty string when nothing
/// matches.
#[must_use]
pub fn filter_to_ids(
    filter: &str,
    rows: &[super::prereq_index::SpellRow],
    owner: Ruleset,
) -> Option<String> {
    let mut rows: Vec<_> = rows
        .iter()
        .filter(|s| offered_to(owner, s.ruleset))
        .collect();
    for clause in filter.split('|') {
        let (k, v) = clause.split_once('=')?;
        let values: Vec<String> = v.split(';').map(|x| x.trim().to_lowercase()).collect();
        match k.trim().to_lowercase().as_str() {
            "level" => rows.retain(|s| values.contains(&s.level.to_string())),
            "class" => rows.retain(|s| s.classes.iter().any(|c| values.contains(c))),
            "spell attack" => {
                rows.retain(|s| s.attacks.iter().any(|a| values.contains(&a.to_lowercase())));
            }
            _ => return None,
        }
    }
    match rows.as_slice() {
        // the filter is understood but no imported spell matches it
        [] => Some(String::new()),
        // a lone id is written twice: ID-range mode needs a single pipe
        [only] => Some(format!("{0}|{0}", only.id)),
        many => Some(
            many.iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>()
                .join("|"),
        ),
    }
}

/// Full school name for a code.
#[must_use]
pub fn school_name(code: &str) -> &'static str {
    match code {
        "A" => "Abjuration",
        "C" => "Conjuration",
        "D" => "Divination",
        "E" => "Enchantment",
        "V" => "Evocation",
        "I" => "Illusion",
        "N" => "Necromancy",
        "T" => "Transmutation",
        "P" => "Psionic",
        _ => "Unknown",
    }
}

/// A level key of a spell map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    /// Character or class level (`_` = 1).
    Level(u32),
    /// Spell slot level (`s3`).
    Slot(u32),
}

fn key_of(key: &str) -> Option<Key> {
    if key == "_" {
        return Some(Key::Level(1));
    }
    if let Some(n) = key.strip_prefix('s') {
        return n.parse::<u32>().ok().filter(|n| *n <= 9).map(Key::Slot);
    }
    key.parse::<u32>().ok().map(|n| Key::Level(n.max(1)))
}

/// What a group grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Known,
    Prepared,
    Innate,
}

/// A casting frequency (`daily`, `rest`, `will`, ...).
#[derive(Debug, Clone, Copy, Default)]
struct Cast<'a> {
    /// `at_will`, `ritual`, `innate`, `resource` or `limited`.
    how: &'a str,
    /// Free casts: (uses key as written, recharge, shared between the spells of the list).
    free: Option<(&'a str, &'a str, bool)>,
    /// Resource points per cast.
    cost: Option<u32>,
}

/// The `grant_spell.mode` (bundled representation, `GRANT_SPELL_MODES`): at-will and innate
/// spells without a frequency are cast at will without a slot (an innate cantrip is a known
/// cantrip), ritual spells only as rituals; limited and resource casts keep the owner's mode and
/// say the rest in `free_cast` / `resource_cost`.
fn grant_mode(kind: Kind, how: Option<&str>, cantrip: bool) -> &'static str {
    match how {
        Some("innate") if cantrip => "known_cantrip",
        Some("at_will" | "innate") => "at_will_no_slot",
        Some("ritual") => "ritual_only",
        _ if kind == Kind::Known => "known",
        _ => "always_prepared",
    }
}

/// Uses of a frequency key: `"2"` / `"2e"` (each) is a number, `"cha"` the ability modifier.
pub(crate) fn uses_value(key: &str) -> Option<Value> {
    if let Ok(n) = key.trim_end_matches('e').parse::<u32>() {
        return Some(json!(n));
    }
    core_ids::ability_stat(key).map(Value::String)
}

fn recharge_of(mode: &str) -> Option<&'static str> {
    Some(match mode {
        "daily" => "long_rest",
        "rest" => "short_rest",
        "weekly" => "weekly",
        "monthly" => "monthly",
        "yearly" => "yearly",
        _ => return None,
    })
}

const ALL_SPELL_LEVELS: &str = "(0||1||2||3||4||5||6||7||8||9)";

struct Ctx<'a> {
    sh: &'a Shared,
    /// Edition of the owner (feat, race, ...), for explicit spell lists.
    ruleset: Ruleset,
    label: String,
    ability: Option<String>,
    resource: Option<String>,
    partial: Vec<String>,
    expanded: Vec<(u32, SpellcastingList)>,
    /// Choices dropped because no imported spell of the owner's edition matches them.
    empty_choices: usize,
}

/// The id of a referenced spell: the linked element, else the id its import gets.
pub(crate) fn spell_ref_id(sh: &Shared, r: &SpellRef) -> String {
    sh.link(RefKind::Spell, &r.name, &r.source)
        .unwrap_or_else(|| {
            format!(
                "ID_5ET_{}_SPELL_{}",
                id_fragment(&r.source),
                id_fragment(&r.name)
            )
        })
}

impl Ctx<'_> {
    fn spell_id(&self, r: &SpellRef) -> String {
        spell_ref_id(self.sh, r)
    }

    /// Ids of the imported spells of the sources named by a `source=A;B` filter.
    fn source_spells(&self, filter: &str) -> Vec<String> {
        let Some(sources) = filter.trim().strip_prefix("source=") else {
            return Vec::new();
        };
        let wanted: Vec<String> = sources
            .split(';')
            .map(|x| x.trim().to_uppercase())
            .collect();
        self.sh
            .links
            .iter()
            .filter(|((kind, _, source), _)| *kind == RefKind::Spell && wanted.contains(source))
            .map(|(_, id)| id.clone())
            .collect()
    }

    fn free_cast(cast: Option<Cast<'_>>) -> Option<Value> {
        let (uses, recharge, shared) = cast?.free?;
        let mut v = json!({"uses": uses_value(uses)?, "recharge": recharge});
        if shared {
            v["shared"] = Value::Bool(true);
        }
        Some(v)
    }

    fn grant(&self, text: &str, level: u32, kind: Kind, cast: Option<Cast<'_>>) -> Rule {
        let r = parse_spell_ref(text);
        let id = self.spell_id(&r);
        if cast.is_none() && r.cast_level.is_none() {
            return Rule::Grant(GrantRule {
                id,
                ty: Some("Spell".into()),
                level: (level > 1).then_some(level),
                spellcasting: Some(self.label.clone()),
                prepared: (kind != Kind::Known).then_some(true),
                ..GrantRule::default()
            });
        }
        let mode = grant_mode(kind, cast.map(|c| c.how), r.cantrip);
        let mut f = Map::new();
        f.insert("spell".into(), json!(id));
        if level > 1 {
            f.insert("level".into(), json!(level));
        }
        f.insert("mode".into(), json!(mode));
        f.insert("spell_list".into(), json!(self.label));
        if let Some(fc) = Self::free_cast(cast) {
            f.insert("free_cast".into(), fc);
        }
        if let (Some(cost), Some(res)) = (cast.and_then(|c| c.cost), &self.resource) {
            f.insert(
                "resource_cost".into(),
                json!({"resource": res, "cost": cost}),
            );
        }
        if let Some(cl) = r.cast_level {
            f.insert("cast_level".into(), json!(cl));
        }
        if let Some(ab) = &self.ability {
            f.insert("ability".into(), json!(ab));
        }
        custom("grant_spell", Value::Object(f))
    }

    fn push_select(out: &mut Vec<Rule>, new: borealis_content::rules::SelectRule) {
        // identical choices merge: select names must be unique
        let same = out.iter_mut().find_map(|r| match r {
            Rule::Select(s) if s.name == new.name && s.level == new.level => Some(s),
            _ => None,
        });
        match same {
            Some(existing) => {
                existing.number = Some(existing.number.unwrap_or(1) + new.number.unwrap_or(1));
            }
            None => out.push(Rule::Select(new)),
        }
    }

    fn choice(
        &mut self,
        choose: &Value,
        count: u32,
        level: u32,
        kind: Kind,
        cast: Option<Cast<'_>>,
        out: &mut Vec<Rule>,
    ) {
        let (supports, what) = match choose {
            Value::String(f) if f.trim().is_empty() => {
                (Some(ALL_SPELL_LEVELS.to_string()), "any spell".to_string())
            }
            Value::String(f) => (
                filter_to_supports(f)
                    .or_else(|| filter_to_ids(f, &self.sh.spell_rows, self.ruleset)),
                f.replace('|', ", "),
            ),
            Value::Object(o) => {
                let from: Vec<&str> = o
                    .get("from")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                if from.len() == 1 {
                    out.push(self.grant(from[0], level, kind, cast));
                    return;
                }
                let ids: Vec<String> = from
                    .iter()
                    .map(|t| self.spell_id(&parse_spell_ref(t)))
                    .collect();
                let names: Vec<String> = from
                    .iter()
                    .map(|t| title_case(&parse_spell_ref(t).name))
                    .collect();
                // ID-range mode: single pipes between element ids
                ((ids.len() > 1).then(|| ids.join("|")), names.join(", "))
            }
            _ => (None, String::new()),
        };
        let Some(supports) = supports else {
            self.partial
                .push(format!("spell choice filter {choose} was not converted"));
            return;
        };
        if supports.is_empty() {
            self.empty_choices += 1;
            self.partial.push(format!(
                "spell choice filter {choose} matches no imported spell (nothing to offer)"
            ));
            return;
        }
        let name = format!("Spell ({}) {what}", self.label);
        let mut s = select("Spell", &name, Some(&supports), count);
        s.level = (level > 1).then_some(level);
        s.spellcasting = Some(self.label.clone());
        s.prepared = (kind != Kind::Known).then_some(true);
        let fresh = !out
            .iter()
            .any(|r| matches!(r, Rule::Select(x) if x.name == name));
        Self::push_select(out, s);
        let free = cast.and_then(|c| c.free);
        if let (true, Some((uses, recharge, _))) = (fresh, free) {
            if let Some(uses) = uses_value(uses) {
                out.push(custom(
                    "spell_free_cast",
                    json!({"select": name, "uses": uses, "recharge": recharge}),
                ));
            }
        }
    }

    /// Rules for a list of spell strings and choice objects.
    fn spell_list(
        &mut self,
        list: &Value,
        level: u32,
        kind: Kind,
        cast: Option<Cast<'_>>,
        out: &mut Vec<Rule>,
    ) {
        let Some(items) = list.as_array() else { return };
        for item in items {
            match item {
                Value::String(s) => out.push(self.grant(s, level, kind, cast)),
                Value::Object(o) => {
                    let count = o
                        .get("count")
                        .and_then(Value::as_u64)
                        .and_then(|c| u32::try_from(c).ok())
                        .unwrap_or(1);
                    match o.get("choose") {
                        Some(c) => {
                            let count = c
                                .get("count")
                                .and_then(Value::as_u64)
                                .and_then(|c| u32::try_from(c).ok())
                                .unwrap_or(count);
                            self.choice(c, count, level, kind, cast, out);
                        }
                        None => self
                            .partial
                            .push("an additional spell entry has an unknown shape".into()),
                    }
                }
                _ => {}
            }
        }
    }

    /// A frequency group: `will` / `ritual` lists, `daily` / `rest` / `resource` maps by uses.
    fn mode(&mut self, mode: &str, list: &Value, level: u32, kind: Kind, out: &mut Vec<Rule>) {
        match mode {
            "will" | "ritual" => {
                let how = if mode == "will" { "at_will" } else { "ritual" };
                let cast = Cast {
                    how,
                    ..Cast::default()
                };
                self.spell_list(list, level, kind, Some(cast), out);
            }
            "resource" | "daily" | "rest" | "weekly" | "monthly" | "yearly" => {
                let Some(by_uses) = list.as_object() else {
                    return;
                };
                for (uses, spells) in by_uses {
                    let n = uses.trim_end_matches('e').parse::<u32>().ok();
                    let shared = !uses.ends_with('e')
                        && n.is_some()
                        && spells.as_array().is_some_and(|a| a.len() > 1);
                    let cast = match (mode, recharge_of(mode)) {
                        ("resource", _) => Cast {
                            how: "resource",
                            cost: n,
                            ..Cast::default()
                        },
                        (_, Some(recharge)) => Cast {
                            how: "limited",
                            free: uses_value(uses).map(|_| (uses.as_str(), recharge, shared)),
                            ..Cast::default()
                        },
                        _ => continue,
                    };
                    self.spell_list(spells, level, kind, Some(cast), out);
                }
            }
            other => self.partial.push(format!(
                "additional spell group {other:?} was not converted"
            )),
        }
    }

    /// Spells keyed by spell slot level: always prepared / known once castable.
    fn slot_keyed(&mut self, v: &Value, kind: Kind) {
        let items = match v {
            Value::Array(a) => a.clone(),
            Value::Object(o) => o
                .get("_")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        if kind == Kind::Innate || items.iter().any(|x| !x.is_string()) || items.is_empty() {
            self.partial
                .push("additional spells keyed by spell slot level were not converted".into());
            return;
        }
        for t in items.iter().filter_map(Value::as_str) {
            let mut entry = SpellcastingList::new(self.spell_id(&parse_spell_ref(t)));
            entry.prepared = Some(true);
            self.expanded.push((1, entry));
        }
    }

    /// `known` / `prepared` / `innate`: `{ "<level>|_|s<n>": [..] | { "_"|mode: .. } }`
    fn level_map(&mut self, map: &Value, kind: Kind, out: &mut Vec<Rule>) {
        let Some(map) = map.as_object() else { return };
        for (k, v) in map {
            let level = match key_of(k) {
                Some(Key::Level(l)) => l,
                Some(Key::Slot(_)) => {
                    self.slot_keyed(v, kind);
                    continue;
                }
                None => {
                    self.partial
                        .push(format!("additional spells keyed {k:?} were not converted"));
                    continue;
                }
            };
            let plain = (kind == Kind::Innate).then_some(Cast {
                how: "innate",
                ..Cast::default()
            });
            match v {
                Value::Array(_) => self.spell_list(v, level, kind, plain, out),
                Value::Object(inner) => {
                    for (ik, iv) in inner {
                        if ik == "_" || ik.parse::<u32>().is_ok() {
                            self.spell_list(iv, level, kind, plain, out);
                        } else {
                            self.mode(ik, iv, level, kind, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// `expanded`: `{ "<class level>|s<n>": [spell | {all: filter}] }`
    fn expanded(&mut self, map: &Value) {
        let Some(map) = map.as_object() else { return };
        for (k, list) in map {
            let gate = match key_of(k) {
                Some(Key::Level(l)) => l,
                Some(Key::Slot(_)) => 1,
                None => {
                    self.partial
                        .push(format!("expanded spells keyed {k:?} were not converted"));
                    continue;
                }
            };
            for item in list.as_array().into_iter().flatten() {
                let entries: Vec<String> = match item {
                    Value::String(t) => vec![self.spell_id(&parse_spell_ref(t))],
                    Value::Object(o) => match o.get("all").and_then(Value::as_str) {
                        // `source=EGW` has no tag: every imported spell of the source, as ids
                        Some(f) => {
                            filter_to_supports(f).map_or_else(|| self.source_spells(f), |e| vec![e])
                        }
                        None => Vec::new(),
                    },
                    _ => Vec::new(),
                };
                if entries.is_empty() {
                    self.partial
                        .push(format!("expanded spell entry {item} was not converted"));
                }
                self.expanded.extend(
                    entries
                        .into_iter()
                        .map(|e| (gate, SpellcastingList::new(e))),
                );
            }
        }
    }
}

/// Abilities an owner's `ability` array lets the player raise (`[{choose: {from: [..]}}]`),
/// as full names: the spellcasting ability of `"ability": "inherit"` groups.
#[must_use]
pub fn inherited_abilities(ability: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in ability {
        let from = a.pointer("/choose/from").and_then(Value::as_array);
        let fixed = a
            .as_object()
            .map(|o| o.keys().cloned().map(Value::String).collect::<Vec<_>>());
        let codes: Vec<Value> = from.cloned().or(fixed).unwrap_or_default();
        for name in codes
            .iter()
            .filter_map(Value::as_str)
            .filter_map(core_ids::ability_name)
        {
            if !out.iter().any(|x| x == name) {
                out.push(name.to_string());
            }
        }
    }
    out
}

fn group_ability(group: &Value, inherit: &[String]) -> SpellAbility {
    match group.get("ability") {
        Some(Value::String(a)) if a == "inherit" => match inherit {
            [] => SpellAbility::Unspecified,
            [one] => SpellAbility::Fixed(one.clone()),
            many => SpellAbility::Choose(many.to_vec()),
        },
        Some(Value::String(a)) => core_ids::ability_name(a)
            .map_or(SpellAbility::Unspecified, |n| SpellAbility::Fixed(n.into())),
        Some(v) => {
            let names: Vec<String> = v
                .get("choose")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().and_then(core_ids::ability_name))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if names.is_empty() {
                SpellAbility::Unspecified
            } else {
                SpellAbility::Choose(names)
            }
        }
        None => SpellAbility::Unspecified,
    }
}

/// A spell granted both plainly and with a casting mode (always prepared from level 3 and
/// castable once per day): the engine keeps only the first grant of an element, so the plain
/// grant folds into the `grant_spell`, which takes the lower level.
fn merge_duplicate_grants(rules: Vec<Rule>) -> Vec<Rule> {
    let mut out: Vec<Rule> = Vec::with_capacity(rules.len());
    for rule in rules {
        if let Rule::Grant(g) = &rule {
            let twin = out.iter_mut().find_map(|r| match r {
                Rule::Custom(c)
                    if c.kind == "grant_spell" && c.fields["spell"] == g.id.as_str() =>
                {
                    Some(c)
                }
                _ => None,
            });
            if let Some(c) = twin {
                lower_level(&mut c.fields, g.level.unwrap_or(1));
                widen_ritual_only(&mut c.fields, g);
                continue;
            }
        }
        if let Rule::Custom(c) = &rule {
            if c.kind == "grant_spell" {
                let level = c.fields.get("level").and_then(Value::as_u64).unwrap_or(1);
                let mut c = c.clone();
                let mut lowest = u32::try_from(level).unwrap_or(1);
                out.retain(|r| match r {
                    Rule::Grant(g) if c.fields["spell"] == g.id.as_str() => {
                        lowest = lowest.min(g.level.unwrap_or(1));
                        widen_ritual_only(&mut c.fields, g);
                        false
                    }
                    _ => true,
                });
                lower_level(&mut c.fields, lowest);
                out.push(Rule::Custom(c));
                continue;
            }
        }
        out.push(rule);
    }
    out
}

/// A ritual-only spell that is also granted plainly is cast normally as well: the plain grant's
/// mode wins.
fn widen_ritual_only(fields: &mut Map<String, Value>, plain: &GrantRule) {
    if fields.get("mode").and_then(Value::as_str) == Some("ritual_only") {
        let mode = if plain.prepared == Some(true) {
            "always_prepared"
        } else {
            "known"
        };
        fields.insert("mode".into(), json!(mode));
    }
}

fn lower_level(fields: &mut Map<String, Value>, level: u32) {
    let current = fields
        .get("level")
        .and_then(Value::as_u64)
        .and_then(|l| u32::try_from(l).ok())
        .unwrap_or(1);
    let lowest = current.min(level);
    if lowest > 1 {
        fields.insert("level".into(), json!(lowest));
    } else {
        fields.remove("level");
    }
}

/// A readable name for an unnamed alternative: its fixed spells, or its position.
fn option_name(group: &Value, index: usize) -> String {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => {
                let n = title_case(&parse_spell_ref(s).name);
                if !out.contains(&n) {
                    out.push(n);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut names = Vec::new();
    for key in ["known", "prepared", "innate"] {
        if let Some(v) = group.get(key) {
            walk(v, &mut names);
        }
    }
    if names.is_empty() {
        format!("Option {}", index + 1)
    } else {
        names.truncate(3);
        names.join(" / ")
    }
}

/// One converted group: its rules, casting and partial notes; `offers_nothing` when its only
/// content was a choice no imported spell of the owner's edition matches.
struct Group {
    rules: Vec<Rule>,
    casting: Casting,
    partial: Vec<String>,
    offers_nothing: bool,
}

fn convert_group(
    sh: &Shared,
    ruleset: Ruleset,
    label: String,
    group: &Value,
    inherit: &[String],
) -> Group {
    let ability = group_ability(group, inherit);
    let mut ctx = Ctx {
        sh,
        ruleset,
        label: label.clone(),
        ability: match &ability {
            SpellAbility::Fixed(a) => Some(a.clone()),
            _ => None,
        },
        resource: group
            .get("resourceName")
            .and_then(Value::as_str)
            .map(str::to_string),
        partial: Vec::new(),
        expanded: Vec::new(),
        empty_choices: 0,
    };
    let mut rules = Vec::new();
    for (key, kind) in [
        ("known", Kind::Known),
        ("prepared", Kind::Prepared),
        ("innate", Kind::Innate),
    ] {
        if let Some(v) = group.get(key) {
            ctx.level_map(v, kind, &mut rules);
        }
    }
    if let Some(v) = group.get("expanded") {
        ctx.expanded(v);
    }
    let rules = merge_duplicate_grants(rules);
    let offers_nothing = ctx.empty_choices > 0 && rules.is_empty() && ctx.expanded.is_empty();
    Group {
        rules,
        casting: Casting {
            label,
            ability,
            expanded: ctx.expanded,
        },
        partial: ctx.partial,
        offers_nothing,
    }
}

/// Convert an `additionalSpells` array of an owner of edition `ruleset`. `label` names the
/// spellcasting source: the class for class spells (`own_label == false`), otherwise the owner
/// (race, feat, ...); options of an owner with its own label are labelled `<label>: <option>`.
#[must_use]
pub fn convert(
    sh: &Shared,
    ruleset: Ruleset,
    label: &str,
    groups: &[Value],
    own_label: bool,
) -> SpellRules {
    convert_inheriting(sh, ruleset, label, groups, own_label, &[])
}

/// [`convert`] for an owner whose ability increase also sets the spellcasting ability
/// (`"ability": "inherit"`): `inherit` lists the abilities the owner lets the player raise.
///
/// An alternative whose only content is a choice that no imported spell of the owner's
/// edition matches (PHB Spell Sniper's cleric and bard cantrips) is not offered, as long as
/// another alternative is: picking it would open an empty choice.
#[must_use]
pub fn convert_inheriting(
    sh: &Shared,
    ruleset: Ruleset,
    label: &str,
    groups: &[Value],
    own_label: bool,
    inherit: &[String],
) -> SpellRules {
    let mut out = SpellRules::default();
    if let [group] = groups {
        let g = convert_group(sh, ruleset, label.to_string(), group, inherit);
        out.direct = g.rules;
        out.casting = g.casting;
        out.partial = g.partial;
        return out;
    }
    let converted: Vec<(String, Group)> = groups
        .iter()
        .enumerate()
        .map(|(i, group)| {
            let name = group
                .get("name")
                .and_then(Value::as_str)
                .map_or_else(|| option_name(group, i), title_case);
            let group_label = if own_label {
                format!("{label}: {name}")
            } else {
                label.to_string()
            };
            let g = convert_group(sh, ruleset, group_label, group, inherit);
            (name, g)
        })
        .collect();
    let any_offered = converted.iter().any(|(_, g)| !g.offers_nothing);
    for (name, g) in converted {
        if any_offered && g.offers_nothing {
            continue;
        }
        out.partial.extend(g.partial);
        out.options.push(SpellOption {
            name,
            rules: g.rules,
            casting: g.casting,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> Shared {
        let mut sh = Shared::default();
        sh.links.insert(
            (RefKind::Spell, "thaumaturgy".into(), "XPHB".into()),
            "ID_5ET_XPHB_SPELL_THAUMATURGY".into(),
        );
        sh
    }

    fn grant_ids(rules: &[Rule]) -> Vec<(String, Option<u32>)> {
        rules
            .iter()
            .filter_map(|r| match r {
                Rule::Grant(g) => Some((g.id.clone(), g.level)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn spell_refs_and_filters() {
        let r = parse_spell_ref("dancing lights|xphb#c");
        assert_eq!(
            r,
            SpellRef {
                name: "dancing lights".into(),
                source: "XPHB".into(),
                cast_level: None,
                cantrip: true
            }
        );
        let r = parse_spell_ref("hellish rebuke#2");
        assert_eq!((r.source.as_str(), r.cast_level), ("PHB", Some(2)));
        assert_eq!(
            filter_to_supports("level=0|class=Wizard").as_deref(),
            Some("0&&Wizard")
        );
        assert_eq!(
            filter_to_supports("level=0|class=cleric;wizard").as_deref(),
            Some("0&&(Cleric||Wizard)")
        );
        assert_eq!(
            filter_to_supports("level=1;2|school=A").as_deref(),
            Some("(1||2)&&Abjuration")
        );
        assert!(filter_to_supports("level=!0").is_none());
        assert_eq!(
            filter_to_supports("level=1|class=Wizard|components & miscellaneous=ritual").as_deref(),
            Some("1&&Wizard&&Ritual")
        );
        assert_eq!(
            filter_to_supports("Components & Miscellaneous=ritual|level=1").as_deref(),
            Some("Ritual&&1")
        );
        assert!(filter_to_supports("components & miscellaneous=royalty").is_none());
        assert!(filter_to_supports("level=0|spell attack=m;r").is_none());
        let row = |id: &str, level: u32, class: &str, attacks: &[&str]| {
            super::super::prereq_index::SpellRow {
                name: id.to_lowercase(),
                level,
                classes: vec![class.into()],
                damage: true,
                attacks: attacks.iter().map(|a| (*a).to_string()).collect(),
                id: id.into(),
                ruleset: None,
            }
        };
        let rows = vec![
            row("ID_A", 0, "wizard", &["R"]),
            row("ID_B", 0, "wizard", &["M"]),
            row("ID_C", 0, "wizard", &[]),
            row("ID_D", 1, "wizard", &["R"]),
            row("ID_E", 0, "cleric", &["R"]),
        ];
        assert_eq!(
            filter_to_ids(
                "level=0|class=Wizard|spell attack=m;r;o",
                &rows,
                Ruleset::Dnd2014
            )
            .as_deref(),
            Some("ID_A|ID_B")
        );
        assert_eq!(
            filter_to_ids(
                "level=0|class=Wizard|spell attack=o",
                &rows,
                Ruleset::Dnd2014
            )
            .as_deref(),
            Some("")
        );
        assert_eq!(
            filter_to_ids("level=1|spell attack=r", &rows, Ruleset::Dnd2014).as_deref(),
            Some("ID_D|ID_D")
        );
        assert!(filter_to_ids("level=0|range=touch", &rows, Ruleset::Dnd2014).is_none());
        assert_eq!(school_name("V"), "Evocation");
        assert_eq!(school_name("?"), "Unknown");
    }

    fn customs<'a>(rules: &'a [Rule], kind: &str) -> Vec<&'a Map<String, Value>> {
        rules
            .iter()
            .filter_map(|r| match r {
                Rule::Custom(c) if c.kind == kind => Some(&c.fields),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn innate_spells_get_free_casts_and_known_spells_plain_grants() {
        let sh = shared();
        let r = convert(
            &sh,
            Ruleset::Dnd2014,
            "Hornfolk",
            &[
                json!({"ability": "cha", "known": {"1": ["thaumaturgy|xphb#c"]},
                     "innate": {"3": {"daily": {"1": ["hellish rebuke#2"]}}, "5": {"rest": {"1": ["darkness"]}}}}),
            ],
            true,
        );
        assert!(r.options.is_empty() && r.partial.is_empty());
        assert_eq!(r.casting.label, "Hornfolk");
        assert_eq!(r.casting.ability, SpellAbility::Fixed("Charisma".into()));
        assert_eq!(
            grant_ids(&r.direct),
            vec![("ID_5ET_XPHB_SPELL_THAUMATURGY".to_string(), None)]
        );
        let Rule::Grant(g) = &r.direct[0] else {
            panic!()
        };
        assert_eq!(g.prepared, None, "known spells are not always prepared");
        let gs = customs(&r.direct, "grant_spell");
        assert_eq!(gs.len(), 2);
        assert_eq!(gs[0]["spell"], "ID_5ET_PHB_SPELL_HELLISH_REBUKE");
        assert_eq!(gs[0]["level"], 3);
        assert_eq!(gs[0]["mode"], "always_prepared");
        assert_eq!(gs[0]["spell_list"], "Hornfolk");
        assert_eq!(
            gs[0]["free_cast"],
            json!({"uses": 1, "recharge": "long_rest"})
        );
        assert_eq!(gs[0]["cast_level"], 2);
        assert_eq!(gs[0]["ability"], "Charisma");
        assert_eq!(gs[1]["free_cast"]["recharge"], "short_rest");
    }

    #[test]
    fn several_groups_are_alternatives_named_or_not() {
        let sh = shared();
        let r = convert(
            &sh,
            Ruleset::Dnd2014,
            "Elf",
            &[
                json!({"name": "Night Elf", "ability": {"choose": ["int", "wis", "cha"]},
                       "known": {"1": ["dancing lights|xphb#c"]},
                       "innate": {"3": {"daily": {"1": ["faerie fire|xphb"]}}}}),
                json!({"name": "Sun Elf", "known": {"1": {"_": [{"choose": "level=0|class=Wizard"}]}}}),
                json!({"known": {"1": ["light|xphb#c"]}}),
            ],
            true,
        );
        assert_eq!(r.options.len(), 3);
        assert_eq!(r.options[0].name, "Night Elf");
        assert_eq!(
            r.options[2].name, "Light",
            "unnamed options are named after their spells"
        );
        assert_eq!(
            r.options[0].casting.ability,
            SpellAbility::Choose(vec![
                "Intelligence".into(),
                "Wisdom".into(),
                "Charisma".into()
            ])
        );
        assert_eq!(r.options[1].casting.label, "Elf: Sun Elf");
        let Rule::Select(s) = &r.options[1].rules[0] else {
            panic!()
        };
        assert_eq!(s.supports.as_deref(), Some("0&&Wizard"));
        assert_eq!(s.spellcasting.as_deref(), Some("Elf: Sun Elf"));
        // class spells keep the class label in every option
        let r = convert(
            &sh,
            Ruleset::Dnd2014,
            "Druid",
            &[
                json!({"name": "A", "prepared": {"3": ["x"]}}),
                json!({"name": "B", "prepared": {"3": ["y"]}}),
            ],
            false,
        );
        assert_eq!(r.options[1].casting.label, "Druid");
    }

    #[test]
    fn modes_choices_and_frequencies() {
        let sh = shared();
        let r = convert(
            &sh,
            Ruleset::Dnd2014,
            "Feat",
            &[json!({
                "resourceName": "Focus",
                "prepared": {"_": {"will": [{"choose": "level=1|school=A"}], "daily": {"1": [{"choose": "level=1;2|school=A"}]}}},
                "innate": {"_": {"ritual": ["detect magic"], "daily": {"2e": ["bless", "bane"], "1": ["fog cloud", "sleep"], "wis": ["guidance"]}},
                           "6": {"resource": {"2": ["darkness"]}}, "9": ["augury", "light#c"],
                           "11": {"rest": {"1": ["shatter#3"]}}},
                "known": {"_": [{"choose": "level=0|class=Cleric", "count": 2}, {"choose": "level=!0"},
                                {"choose": {"from": ["shield", "mage armor"], "count": 1}}, {"choose": ""}]},
            })],
            true,
        );
        let gs = customs(&r.direct, "grant_spell");
        let by = |suffix: &str| {
            *gs.iter()
                .find(|g| g["spell"].as_str().unwrap().ends_with(suffix))
                .unwrap()
        };
        assert!(
            gs.iter().all(|g| g.get("cast").is_none()),
            "no field the engine ignores"
        );
        let problems: Vec<_> = r
            .direct
            .iter()
            .filter_map(|x| match x {
                Rule::Custom(c) if c.kind == "grant_spell" => Some(c),
                _ => None,
            })
            .flat_map(|c| {
                let spec = borealis_content::rule_kinds::spec("grant_spell").unwrap();
                let mut p: Vec<String> = borealis_content::rule_kinds::check_custom(c)
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                p.extend(spec.shape_problems(&c.fields));
                p
            })
            .collect();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(by("DETECT_MAGIC")["mode"], "ritual_only");
        assert_eq!(by("BLESS")["mode"], "always_prepared");
        assert_eq!(by("LIGHT")["mode"], "known_cantrip");
        assert_eq!(by("SHATTER")["cast_level"], 3);
        assert_eq!(by("SHATTER")["free_cast"]["recharge"], "short_rest");
        assert_eq!(
            by("BLESS")["free_cast"],
            json!({"uses": 2, "recharge": "long_rest"})
        );
        assert_eq!(
            by("SLEEP")["free_cast"]["shared"],
            true,
            "one use shared by the list"
        );
        assert_eq!(by("GUIDANCE")["free_cast"]["uses"], "wisdom");
        assert_eq!(
            by("DARKNESS")["resource_cost"],
            json!({"resource": "Focus", "cost": 2})
        );
        assert_eq!(by("DARKNESS")["level"], 6);
        assert_eq!(
            by("AUGURY")["mode"],
            "at_will_no_slot",
            "innate without a frequency"
        );
        let selects: Vec<_> = r
            .direct
            .iter()
            .filter_map(|x| match x {
                Rule::Select(s) => Some(s),
                _ => None,
            })
            .collect();
        assert!(selects
            .iter()
            .any(|s| s.number == Some(2) && s.supports.as_deref() == Some("0&&Cleric")));
        assert!(selects.iter().any(|s| s.supports.as_deref()
            == Some("ID_5ET_PHB_SPELL_SHIELD|ID_5ET_PHB_SPELL_MAGE_ARMOR")));
        assert!(selects
            .iter()
            .any(|s| s.supports.as_deref() == Some(ALL_SPELL_LEVELS)));
        let daily = selects
            .iter()
            .find(|s| s.supports.as_deref() == Some("(1||2)&&Abjuration"))
            .unwrap();
        assert_eq!(daily.prepared, Some(true));
        let fc = customs(&r.direct, "spell_free_cast");
        assert_eq!(fc.len(), 1);
        assert_eq!(fc[0]["select"], json!(daily.name));
        assert_eq!(fc[0]["recharge"], "long_rest");
        assert_eq!(r.partial.len(), 1, "{:?}", r.partial);
        assert!(r.partial[0].contains("level=!0"));
    }

    #[test]
    fn a_ritual_only_spell_also_granted_plainly_is_cast_normally() {
        let sh = shared();
        let r = convert(
            &sh,
            Ruleset::Dnd2014,
            "Feat",
            &[json!({"prepared": {"_": ["alarm"]}, "innate": {"_": {"ritual": ["alarm"]}}})],
            false,
        );
        let gs = customs(&r.direct, "grant_spell");
        assert_eq!(r.direct.len(), 1, "{:?}", r.direct);
        assert_eq!(gs[0]["mode"], "always_prepared");
    }

    #[test]
    fn a_spell_granted_twice_keeps_one_grant_with_its_casting_mode() {
        let sh = shared();
        let r = convert(
            &sh,
            Ruleset::Dnd2014,
            "Warlock",
            &[
                json!({"prepared": {"3": ["misty step"]}, "innate": {"_": {"daily": {"cha": ["misty step"]}}}}),
            ],
            false,
        );
        assert_eq!(r.direct.len(), 1, "{:?}", r.direct);
        let gs = customs(&r.direct, "grant_spell");
        assert_eq!(gs[0]["free_cast"]["uses"], "charisma");
        assert!(gs[0].get("level").is_none(), "the lower level wins");
        let r = convert(
            &sh,
            Ruleset::Dnd2014,
            "Warlock",
            &[json!({"innate": {"5": {"daily": {"1": ["fly"]}}}, "prepared": {"3": ["fly"]}})],
            false,
        );
        assert_eq!(r.direct.len(), 1);
        assert_eq!(customs(&r.direct, "grant_spell")[0]["level"], 3);
    }

    #[test]
    fn inherited_ability_follows_the_owner_increase() {
        let sh = shared();
        let raised =
            inherited_abilities(&[json!({"choose": {"from": ["int", "wis", "cha"], "amount": 1}})]);
        assert_eq!(raised, vec!["Intelligence", "Wisdom", "Charisma"]);
        assert_eq!(inherited_abilities(&[json!({"wis": 1})]), vec!["Wisdom"]);
        let group = [json!({"ability": "inherit", "known": {"_": ["light"]}})];
        let r = convert_inheriting(&sh, Ruleset::Dnd2014, "Feat", &group, true, &raised);
        assert_eq!(r.casting.ability, SpellAbility::Choose(raised.clone()));
        let r = convert_inheriting(&sh, Ruleset::Dnd2014, "Feat", &group, true, &raised[1..2]);
        assert_eq!(r.casting.ability, SpellAbility::Fixed("Wisdom".into()));
    }

    #[test]
    fn slot_keyed_and_expanded_lists_become_extensions() {
        let sh = shared();
        let r = convert(
            &sh,
            Ruleset::Dnd2014,
            "Adept",
            &[json!({
                "prepared": {"s1": ["shield"], "s2": ["blur"]},
                "expanded": {"s1": ["burning hands"], "10": [{"all": "level=1;2|class=Cleric"}], "x": ["nope"]},
            })],
            true,
        );
        assert!(r.direct.is_empty());
        let e = &r.casting.expanded;
        assert_eq!(e.len(), 4);
        assert_eq!(
            (e[0].0, e[0].1.supports.as_str(), e[0].1.prepared),
            (1, "ID_5ET_PHB_SPELL_SHIELD", Some(true))
        );
        assert!(e.contains(&(1, SpellcastingList::new("ID_5ET_PHB_SPELL_BURNING_HANDS"))));
        assert!(e.contains(&(10, SpellcastingList::new("(1||2)&&Cleric"))));
        assert_eq!(r.partial.len(), 1);
        let r = convert(
            &sh,
            Ruleset::Dnd2014,
            "X",
            &[json!({"innate": {"s1": ["shield"]}})],
            true,
        );
        assert_eq!(r.partial.len(), 1);
        // cantrips (s0) and whole-source lists
        let r = convert(
            &sh,
            Ruleset::Dnd2014,
            "X",
            &[json!({"expanded": {"s0": ["light#c"], "1": [{"all": "source=xphb"}]}})],
            false,
        );
        assert!(r.partial.is_empty(), "{:?}", r.partial);
        assert!(r
            .casting
            .expanded
            .contains(&(1, SpellcastingList::new("ID_5ET_PHB_SPELL_LIGHT"))));
        assert!(r
            .casting
            .expanded
            .contains(&(1, SpellcastingList::new("ID_5ET_XPHB_SPELL_THAUMATURGY"))));
    }

    fn edition_row(
        id: &str,
        class: &str,
        ruleset: Ruleset,
    ) -> super::super::prereq_index::SpellRow {
        super::super::prereq_index::SpellRow {
            name: id.to_lowercase(),
            level: 0,
            classes: vec![class.into()],
            damage: true,
            attacks: vec!["R".into()],
            id: id.into(),
            ruleset: Some(ruleset),
        }
    }

    #[test]
    fn explicit_spell_lists_keep_the_spells_the_owner_edition_is_offered() {
        let rows = vec![
            edition_row("ID_OLD", "wizard", Ruleset::Dnd2014),
            edition_row("ID_NEW", "wizard", Ruleset::Dnd2024),
            edition_row("ID_NEW_BARD", "bard", Ruleset::Dnd2024),
        ];
        let f = "level=0|class=Wizard|spell attack=r";
        assert_eq!(
            filter_to_ids(f, &rows, Ruleset::Dnd2014).as_deref(),
            Some("ID_OLD|ID_OLD")
        );
        assert_eq!(
            filter_to_ids(f, &rows, Ruleset::Dnd2024).as_deref(),
            Some("ID_OLD|ID_NEW")
        );
        assert_eq!(
            filter_to_ids("level=0|class=Bard|spell attack=r", &rows, Ruleset::Dnd2014).as_deref(),
            Some("")
        );
    }

    /// PHB Spell Sniper: per-class cantrip alternatives; the bard list holds only a 2024
    /// cantrip and the cleric list none, so a 2014 owner is not offered either.
    #[test]
    fn alternatives_with_nothing_to_choose_are_not_offered() {
        let mut sh = shared();
        sh.spell_rows = vec![
            edition_row("ID_FIRE_BOLT", "wizard", Ruleset::Dnd2014),
            edition_row("ID_STARRY_WISP", "bard", Ruleset::Dnd2024),
        ];
        let pick = |class: &str| {
            json!({"name": format!("{class} Spells"),
                   "known": {"_": [{"choose": format!("level=0|class={class}|spell attack=m;r;o")}]}})
        };
        let groups = [pick("Bard"), pick("Cleric"), pick("Wizard")];
        let r = convert(&sh, Ruleset::Dnd2014, "Spell Sniper", &groups, true);
        let names: Vec<&str> = r.options.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["Wizard Spells"]);
        assert!(r.partial.is_empty(), "{:?}", r.partial);
        // a 2024 owner is offered the 2024 bard cantrip
        let r = convert(&sh, Ruleset::Dnd2024, "Spell Sniper", &groups, true);
        let names: Vec<&str> = r.options.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["Bard Spells", "Wizard Spells"]);
        // nothing offered at all: the alternatives stay, with their notes
        let r = convert(&sh, Ruleset::Dnd2014, "X", &groups[..2], true);
        assert_eq!(r.options.len(), 2);
        assert_eq!(r.partial.len(), 2);
    }
}
