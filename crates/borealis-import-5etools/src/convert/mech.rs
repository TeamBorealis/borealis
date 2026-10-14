//! Shared mechanics: proficiencies, abilities, speed, senses, resistances, size, feats.
//!
//! A [`Mech`] accumulates the rules of one element. Anything that cannot be expressed as a
//! rule is recorded in `partial` (for the report) and `unmapped` (kept on the element).

use super::build::{gate, grant, select, setter_with, stat_bonus};
use super::Shared;
use crate::core_ids::{self as core, DamageRelation};
use crate::text::RefKind;
use crate::util::{id_fragment, title_case};
use borealis_content::Rule;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

mod ability;

/// Where an ability increase comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilitySource {
    /// Race / species.
    Race,
    /// Background.
    Background,
    /// Feat.
    Feat,
}

/// Accumulates rules for one element.
pub struct Mech<'a> {
    sh: &'a Shared,
    label: String,
    gate: Option<String>,
    /// The rules, in order.
    pub rules: Vec<Rule>,
    /// Loss descriptions for the report.
    pub partial: Vec<String>,
    /// Unconverted details to keep on the element: key -> values.
    pub unmapped: BTreeMap<String, Vec<String>>,
    names: BTreeMap<String, u32>,
}

fn obj_entries(v: &Value) -> Vec<(&String, &Value)> {
    v.as_object()
        .map(|m| m.iter().collect())
        .unwrap_or_default()
}

fn count_of(v: &Value) -> u32 {
    v.as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(1)
        .max(1)
}

impl<'a> Mech<'a> {
    /// A collector whose select names are prefixed with `label`.
    #[must_use]
    pub fn new(sh: &'a Shared, label: &str) -> Self {
        Self {
            sh,
            label: label.to_string(),
            gate: None,
            rules: Vec::new(),
            partial: Vec::new(),
            unmapped: BTreeMap::new(),
            names: BTreeMap::new(),
        }
    }

    /// Gate every rule pushed from now on with a requirements expression.
    pub fn set_gate(&mut self, requirements: Option<&str>) {
        self.gate = requirements.map(str::to_string);
    }

    /// Push a rule, applying the current gate.
    pub fn push(&mut self, mut rule: Rule) {
        if let Some(g) = &self.gate {
            if let Err(e) = gate(&mut rule, g) {
                self.partial.push(e);
            }
        }
        self.rules.push(rule);
    }

    fn unique_name(&mut self, base: &str) -> String {
        let full = format!("{base} ({})", self.label);
        let n = self.names.entry(full.clone()).or_insert(0);
        *n += 1;
        if *n == 1 {
            full
        } else {
            format!("{base} {} ({})", *n, self.label)
        }
    }

    fn note(&mut self, key: &str, what: String) {
        self.partial.push(what.clone());
        self.unmapped.entry(key.to_string()).or_default().push(what);
    }

    /// A select over fixed element ids (ID-range supports). One candidate becomes a grant.
    fn pick_from(&mut self, ty: &str, base: &str, ids: &[String], count: u32) {
        match ids {
            [] => {}
            [only] => self.push(grant(only, ty)),
            many => {
                let supports = many.join("|");
                let name = self.unique_name(base);
                self.push(Rule::Select(select(ty, &name, Some(&supports), count)));
            }
        }
    }

    fn pick_tag(&mut self, ty: &str, base: &str, supports: &str, count: u32) {
        let name = self.unique_name(base);
        self.push(Rule::Select(select(ty, &name, Some(supports), count)));
    }

    // ---------------------------------------------------------------- skills

    /// `skillProficiencies`
    pub fn skills(&mut self, list: &[Value]) {
        for obj in list {
            for (k, v) in obj_entries(obj) {
                match k.as_str() {
                    "any" => {
                        self.pick_tag("Proficiency", "Skill Proficiency", "Skill", count_of(v))
                    }
                    "choose" => {
                        let from = string_list(v.get("from"));
                        let ids: Vec<String> =
                            from.iter().filter_map(|s| core::skill_id(s)).collect();
                        self.pick_from(
                            "Proficiency",
                            "Skill Proficiency",
                            &ids,
                            count_of(v.get("count").unwrap_or(&json!(1))),
                        );
                    }
                    name if v == &Value::Bool(true) => match core::skill_id(name) {
                        Some(id) => self.push(grant(&id, "Proficiency")),
                        None => self.note("skill", format!("unknown skill proficiency {name:?}")),
                    },
                    other => self.note(
                        "skill",
                        format!("skill proficiency entry {other:?} not converted"),
                    ),
                }
            }
        }
    }

    // ------------------------------------------------------------- languages

    fn language_tags(key: &str) -> Option<&'static str> {
        Some(match key {
            "any" | "anyLanguage" | "other" | "otherLanguage" => "Standard||Exotic",
            "anyStandard" | "anyStandardLanguage" => "Standard",
            "anyExotic" | "anyExoticLanguage" => "Exotic",
            "anyRare" | "anyRareLanguage" => "Exotic||Secret",
            _ => return None,
        })
    }

    /// `languageProficiencies`
    pub fn languages(&mut self, list: &[Value]) {
        for obj in list {
            for (k, v) in obj_entries(obj) {
                if let Some(tags) = Self::language_tags(k) {
                    if v.is_number() {
                        self.pick_tag("Language", "Language", tags, count_of(v));
                    } else if v == &Value::Bool(true) {
                        self.pick_tag("Language", "Language", tags, 1);
                    }
                } else if k == "choose" {
                    let from = string_list(v.get("from"));
                    let count = count_of(v.get("count").unwrap_or(&json!(1)));
                    // "other" / "anyStandard" in the list means any language of that kind; a
                    // select is either an id list or a tag filter, so the choice is widened.
                    if let Some(tags) = from.iter().find_map(|n| Self::language_tags(n)) {
                        self.note(
                            "language",
                            "a language choice mixing named languages and \"any\" was widened to any language".into(),
                        );
                        self.pick_tag("Language", "Language", tags, count);
                    } else {
                        let ids: Vec<String> =
                            from.iter().map(|n| self.sh.language_id(n)).collect();
                        self.pick_from("Language", "Language", &ids, count);
                    }
                } else if v == &Value::Bool(true) {
                    let id = self.sh.language_id(k);
                    self.push(grant(&id, "Language"));
                } else {
                    self.note("language", format!("language entry {k:?} not converted"));
                }
            }
        }
    }

    // ----------------------------------------------------------------- tools

    fn tool_choice_tag(key: &str) -> Option<&'static str> {
        Some(match key {
            "anyArtisansTool" => "Artisan tools",
            "anyGamingSet" => "Gaming Set",
            "anyMusicalInstrument" => "Musical Instrument",
            "any" | "anyTool" => "Tool||Gaming Set",
            _ => return None,
        })
    }

    /// `toolProficiencies`
    pub fn tools(&mut self, list: &[Value]) {
        for obj in list {
            for (k, v) in obj_entries(obj) {
                if let Some(tag) = Self::tool_choice_tag(k) {
                    if v.is_number() {
                        self.pick_tag("Proficiency", "Tool Proficiency", tag, count_of(v));
                    }
                } else if k == "choose" {
                    self.tool_choose(v);
                } else if v == &Value::Bool(true) {
                    match core::tool_prof_id(k) {
                        Some(id) => self.push(grant(&id, "Proficiency")),
                        None => self.note(
                            "tool",
                            format!("tool proficiency {k:?} has no core element"),
                        ),
                    }
                } else {
                    self.note("tool", format!("tool entry {k:?} not converted"));
                }
            }
        }
    }

    fn tool_choose(&mut self, v: &Value) {
        let from = string_list(v.get("from"));
        let count = count_of(v.get("count").unwrap_or(&json!(1)));
        let specials: Vec<&String> = from
            .iter()
            .filter(|s| Self::tool_choice_tag(s).is_some())
            .collect();
        if specials.is_empty() {
            let ids: Vec<String> = from.iter().filter_map(|s| core::tool_prof_id(s)).collect();
            if ids.len() != from.len() {
                self.partial
                    .push("some tools in a tool choice have no core element".into());
            }
            self.pick_from("Proficiency", "Tool Proficiency", &ids, count);
        } else {
            let tags: Vec<&str> = specials
                .iter()
                .filter_map(|s| Self::tool_choice_tag(s))
                .collect();
            self.pick_tag("Proficiency", "Tool Proficiency", &tags.join("||"), count);
            if specials.len() != from.len() {
                self.partial.push("a tool choice mixing categories and single tools was widened to the categories".into());
            }
        }
    }

    // ------------------------------------------------------- weapons / armor

    /// `weaponProficiencies`
    pub fn weapons(&mut self, list: &[Value]) {
        for obj in list {
            for (k, v) in obj_entries(obj) {
                match k.as_str() {
                    "choose" | "all" => {
                        let filter = v.get("fromFilter").and_then(Value::as_str).unwrap_or("?");
                        self.note(
                            "weapon",
                            format!("weapon proficiency chosen by filter {filter:?} not converted"),
                        );
                    }
                    name if v == &Value::Bool(true) => {
                        let base = name.split('|').next().unwrap_or(name);
                        match core::weapon_prof_id(base) {
                            Some(id) => self.push(grant(&id, "Proficiency")),
                            None => self.note(
                                "weapon",
                                format!("weapon proficiency {base:?} has no core element"),
                            ),
                        }
                    }
                    other => self.note("weapon", format!("weapon entry {other:?} not converted")),
                }
            }
        }
    }

    /// `armorProficiencies`
    pub fn armor(&mut self, list: &[Value]) {
        for obj in list {
            for (k, v) in obj_entries(obj) {
                if v != &Value::Bool(true) {
                    continue;
                }
                let base = k.split('|').next().unwrap_or(k);
                match core::armor_prof_id(base) {
                    Some(id) => self.push(grant(id, "Proficiency")),
                    None => self.note(
                        "armor",
                        format!("armor proficiency {base:?} has no core element"),
                    ),
                }
            }
        }
    }

    /// Class `proficiency` (saving throw abbreviations).
    pub fn saving_throws(&mut self, abbrs: &[String]) {
        for a in abbrs {
            match core::saving_throw_id(a) {
                Some(id) => self.push(grant(&id, "Proficiency")),
                None => self.note("saving throw", format!("unknown saving throw {a:?}")),
            }
        }
    }

    /// `skillToolLanguageProficiencies`: objects mixing skills, tools and languages.
    pub fn skill_tool_language(&mut self, list: &[Value]) {
        for obj in list {
            let Some(map) = obj.as_object() else { continue };
            let mut skills = Map::new();
            let mut tools = Map::new();
            let mut langs = Map::new();
            for (k, v) in map {
                if k == "choose" {
                    self.stl_choose(v);
                } else if let Some(rest) = k.strip_prefix("anySkill") {
                    let _ = rest;
                    skills.insert("any".into(), v.clone());
                } else if k == "anyTool"
                    || k == "anyArtisansTool"
                    || k == "anyGamingSet"
                    || k == "anyMusicalInstrument"
                {
                    tools.insert(k.clone(), v.clone());
                } else if k.starts_with("anyLanguage")
                    || k.starts_with("anyStandard")
                    || k.starts_with("anyExotic")
                    || k.starts_with("anyRare")
                    || k == "otherLanguage"
                {
                    langs.insert(k.clone(), v.clone());
                } else if core::is_skill(k) {
                    skills.insert(k.clone(), v.clone());
                } else if core::tool_prof_id(k).is_some() {
                    tools.insert(k.clone(), v.clone());
                } else {
                    langs.insert(k.clone(), v.clone());
                }
            }
            self.skills(&[Value::Object(skills)]);
            self.tools(&[Value::Object(tools)]);
            self.languages(&[Value::Object(langs)]);
        }
    }

    fn stl_choose(&mut self, v: &Value) {
        let groups = match v {
            Value::Array(a) => a.clone(),
            other => vec![other.clone()],
        };
        for g in groups {
            let from = string_list(g.get("from"));
            let count = count_of(g.get("count").unwrap_or(&json!(1)));
            let mut tags: Vec<&str> = Vec::new();
            for f in &from {
                match f.as_str() {
                    "anySkill" => tags.push("Skill"),
                    "anyTool" => tags.push("Tool||Gaming Set"),
                    "anyLanguage" | "anyStandardLanguage" => tags.push("Standard||Exotic"),
                    _ => {}
                }
            }
            if tags.is_empty() {
                self.note(
                    "proficiency",
                    "skill/tool/language choice not converted".into(),
                );
            } else {
                tags.sort_unstable();
                tags.dedup();
                // A pick across several proficiency types: the element type must be Proficiency.
                let supports = tags.join("||");
                self.pick_tag("Proficiency", "Proficiency", &supports, count);
                if from
                    .iter()
                    .any(|f| f.contains("anyLanguage") || f.contains("anyStandardLanguage"))
                {
                    self.partial.push(
                        "language options of a mixed proficiency choice are not offered".into(),
                    );
                }
            }
        }
    }

    /// `expertise`
    pub fn expertise(&mut self, list: &[Value]) {
        for obj in list {
            for (k, v) in obj_entries(obj) {
                if k == "anyProficientSkill" {
                    self.pick_tag("Class Feature", "Expertise", "Expertise", count_of(v));
                } else if v == &Value::Bool(true) {
                    if let Some(id) = core::skill_id(k) {
                        self.push(grant(
                            &id.replace("ID_PROFICIENCY_SKILL_", "ID_EXPERTISE_SKILL_"),
                            "Class Feature",
                        ));
                    } else if core::tool_prof_id(k).as_deref()
                        == Some("ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS")
                    {
                        self.push(grant("ID_EXPERTISE_TOOL_THIEVES_TOOLS", "Class Feature"));
                    } else {
                        self.note("expertise", format!("expertise in {k:?} not converted"));
                    }
                } else {
                    self.note("expertise", format!("expertise entry {k:?} not converted"));
                }
            }
        }
    }

    // ------------------------------------------------------------ resistances

    fn damage_list(&mut self, rel: DamageRelation, list: &[Value], key: &str) {
        for item in list {
            match item {
                Value::String(s) => match core::damage_condition_id(rel, s) {
                    Some(id) => self.push(grant(&id, "Condition")),
                    None => self.note(
                        key,
                        format!("{} {s:?} has no core condition", rel.tag().to_lowercase()),
                    ),
                },
                Value::Object(_) if item.get("choose").is_some() => {
                    let choose = &item["choose"];
                    let from: Vec<String> = choose
                        .get("from")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| {
                                    v.as_str().and_then(|s| core::damage_condition_id(rel, s))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let count = count_of(choose.get("count").unwrap_or(&json!(1)));
                    self.pick_from("Condition", &format!("Damage {}", rel.tag()), &from, count);
                }
                other => self.note(
                    key,
                    format!("{} entry {other} not converted", rel.tag().to_lowercase()),
                ),
            }
        }
    }

    /// `resist` / `immune` / `vulnerable` / `conditionImmune`
    pub fn resistances(
        &mut self,
        resist: &[Value],
        immune: &[Value],
        vulnerable: &[Value],
        cond_immune: &[Value],
    ) {
        self.damage_list(DamageRelation::Resistance, resist, "resistance");
        self.damage_list(DamageRelation::Immunity, immune, "immunity");
        self.damage_list(DamageRelation::Vulnerability, vulnerable, "vulnerability");
        for c in cond_immune {
            let text = match c {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            self.note(
                "condition immunity",
                format!("condition immunity {text} has no core element"),
            );
        }
    }

    // ---------------------------------------------------------------- senses

    fn vision(&mut self, sense: &str, range: i64) {
        match sense {
            "darkvision" if range == 120 => {
                self.push(grant("ID_VISION_SUPERIORDARKVISION", "Vision"))
            }
            "darkvision" | "blindsight" | "truesight" => {
                let Some((id, group)) = core::vision(sense) else {
                    return;
                };
                self.push(grant(id, "Vision"));
                if !(sense == "darkvision" && range == 60) {
                    self.push(stat_bonus(group, range, "base"));
                }
            }
            other => {
                self.push(stat_bonus(&format!("{other}:range"), range, "base"));
                self.note(
                    "sense",
                    format!("{other} {range} ft. is a statistic only (no core vision element)"),
                );
            }
        }
    }

    /// Racial `darkvision` / `blindsight` and feat `senses` objects.
    pub fn senses(&mut self, darkvision: Option<i64>, blindsight: Option<i64>, senses: &[Value]) {
        if let Some(r) = darkvision {
            self.vision("darkvision", r);
        }
        if let Some(r) = blindsight {
            self.vision("blindsight", r);
        }
        for obj in senses {
            for (k, v) in obj_entries(obj) {
                if let Some(r) = v.as_i64() {
                    self.vision(k, r);
                }
            }
        }
    }

    // ------------------------------------------------------------ size/speed

    /// `size` codes.
    pub fn size(&mut self, sizes: &[String]) {
        let ids: Vec<String> = sizes
            .iter()
            .filter_map(|c| core::size_id(c))
            .map(str::to_string)
            .collect();
        self.pick_from("Size", "Size", &ids, 1);
    }

    /// `speed`
    pub fn speed(&mut self, speed: Option<&Value>) {
        match speed {
            None => {}
            Some(Value::Number(n)) => {
                if let Some(n) = n.as_i64() {
                    self.push(stat_bonus("innate speed", n, "base"));
                }
            }
            Some(Value::Object(m)) => {
                for (mode, v) in m {
                    let group = if mode == "walk" {
                        "innate speed".to_string()
                    } else {
                        format!("innate speed:{mode}")
                    };
                    match (mode.as_str(), v) {
                        ("walk" | "fly" | "swim" | "climb" | "burrow", Value::Number(n)) => {
                            if let Some(n) = n.as_i64() {
                                self.push(stat_bonus(&group, n, "base"));
                            }
                        }
                        ("fly" | "swim" | "climb" | "burrow", Value::Bool(true)) => {
                            self.push(stat_bonus(&group, "innate speed", "base"));
                        }
                        ("walk" | "fly" | "swim" | "climb" | "burrow", Value::Object(o)) => {
                            if let Some(n) = o.get("number").and_then(Value::as_i64) {
                                self.push(stat_bonus(&group, n, "base"));
                                if let Some(c) = o.get("condition").and_then(Value::as_str) {
                                    self.partial
                                        .push(format!("{mode} speed condition {c:?} dropped"));
                                }
                            }
                        }
                        ("canHover", _) => {}
                        ("choose", _) => self.note(
                            "speed",
                            "speed chosen from several modes not converted".into(),
                        ),
                        ("alternate" | "hidden", _) => {}
                        (other, _) => {
                            self.note("speed", format!("speed entry {other:?} not converted"))
                        }
                    }
                }
            }
            Some(other) => self.note("speed", format!("speed {other} not converted")),
        }
    }

    // ----------------------------------------------------------------- feats

    /// `feats` granted by a race, species or background.
    pub fn feats(&mut self, list: &[Value]) {
        for obj in list {
            for (k, v) in obj_entries(obj) {
                match k.as_str() {
                    // any feat: no `supports` filter (imported feats carry their category
                    // as a tag, never a generic "Feat" tag)
                    "any" => {
                        let name = self.unique_name("Feat");
                        self.push(Rule::Select(select("Feat", &name, None, count_of(v))));
                    }
                    "anyFromCategory" => {
                        let cats = string_list(v.get("category"));
                        let tags: Vec<String> = cats
                            .iter()
                            .map(|c| feat_category_label(c).to_string())
                            .collect();
                        let supports = tags.join("||");
                        self.pick_tag(
                            "Feat",
                            "Feat",
                            &supports,
                            count_of(v.get("count").unwrap_or(&json!(1))),
                        );
                    }
                    uid if v == &Value::Bool(true) => {
                        let (name_part, source) = uid.split_once('|').unwrap_or((uid, "PHB"));
                        let (name, variant) = name_part
                            .split_once(';')
                            .map_or((name_part, None), |(n, v)| (n, Some(v.trim())));
                        // a variant the data defines as a feat version ("Magic Initiate;
                        // Cleric") is granted as that version, so its choice is already made
                        let version = variant
                            .and_then(|_| self.sh.link(RefKind::Feat, name_part.trim(), source));
                        if let Some(id) = version {
                            self.push(grant(&id, "Feat"));
                            continue;
                        }
                        match self.sh.link(RefKind::Feat, name.trim(), source) {
                            Some(id) => {
                                let mut rule = grant(&id, "Feat");
                                if let (Rule::Grant(g), Some(var)) = (&mut rule, variant) {
                                    g.setters
                                        .push(setter_with("variant", &title_case(var), &[]));
                                }
                                self.push(rule);
                            }
                            None => {
                                let id = format!(
                                    "ID_5ET_{}_FEAT_{}",
                                    id_fragment(source),
                                    id_fragment(name.trim())
                                );
                                let mut rule = grant(&id, "Feat");
                                if let (Rule::Grant(g), Some(var)) = (&mut rule, variant) {
                                    g.setters
                                        .push(setter_with("variant", &title_case(var), &[]));
                                }
                                self.push(rule);
                                self.partial.push(format!("feat {name:?} ({source}) is not part of this import; the reference may dangle"));
                            }
                        }
                    }
                    other => self.note("feat", format!("feat entry {other:?} not converted")),
                }
            }
        }
    }
}

/// Plain strings of an optional JSON array.
#[must_use]
pub fn string_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Supports tag of a feat category code: the `<Category> Feat` tags the bundled SRD 5.2.1
/// feats and feat selects use, so imported selects offer bundled feats and the other way round.
#[must_use]
pub fn feat_category_label(code: &str) -> &'static str {
    match code {
        "O" => "Origin Feat",
        "G" => "General Feat",
        "D" | "DG" => "Dragonmark Feat",
        "FS" | "FS:P" | "FS:R" => "Fighting Style Feat",
        "EB" => "Epic Boon Feat",
        _ => "Other Feat",
    }
}

#[cfg(test)]
mod tests_support {
    use super::*;
    use crate::source::default_source_name;

    pub fn shared() -> Shared {
        let mut sh = Shared::default();
        sh.links.insert(
            (RefKind::Feat, "alert".into(), "XPHB".into()),
            "ID_5ET_XPHB_FEAT_ALERT".into(),
        );
        sh.languages
            .insert("gith".into(), "ID_5ET_LANGUAGE_GITH".into());
        let _ = default_source_name("PHB");
        sh
    }

    pub fn ids_of(m: &Mech<'_>) -> Vec<String> {
        m.rules
            .iter()
            .map(|r| match r {
                Rule::Grant(g) => format!("grant:{}", g.id),
                Rule::Select(s) => format!(
                    "select:{}:{}:{}",
                    s.ty,
                    s.name,
                    s.supports.clone().unwrap_or_default()
                ),
                Rule::Stat(s) => format!("stat:{}={}", s.name, s.value),
                Rule::Custom(c) => format!("custom:{}", c.kind),
                Rule::Unknown(_) => "unknown".into(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::{ids_of, shared};
    use super::*;

    #[test]
    fn skills_fixed_any_and_choose() {
        let sh = shared();
        let mut m = Mech::new(&sh, "Test");
        m.skills(&[
            json!({"insight": true, "sleight of hand": true}),
            json!({"any": 2}),
            json!({"choose": {"from": ["arcana", "history", "nature"], "count": 2}}),
            json!({"choose": {"from": ["stealth"]}}),
            json!({"basketweaving": true}),
        ]);
        let ids = ids_of(&m);
        assert!(ids.contains(&"grant:ID_PROFICIENCY_SKILL_INSIGHT".to_string()));
        assert!(ids.contains(&"grant:ID_PROFICIENCY_SKILL_SLEIGHTOFHAND".to_string()));
        assert!(ids.contains(&"select:Proficiency:Skill Proficiency (Test):Skill".to_string()));
        assert!(ids.contains(&"select:Proficiency:Skill Proficiency 2 (Test):ID_PROFICIENCY_SKILL_ARCANA|ID_PROFICIENCY_SKILL_HISTORY|ID_PROFICIENCY_SKILL_NATURE".to_string()));
        assert!(ids.contains(&"grant:ID_PROFICIENCY_SKILL_STEALTH".to_string()));
        assert_eq!(m.partial.len(), 1);
        assert!(m.unmapped.contains_key("skill"));
        let Rule::Select(s) = &m.rules[2] else {
            panic!()
        };
        assert_eq!(s.number, Some(2));
        let Rule::Select(s) = &m.rules[3] else {
            panic!()
        };
        assert_eq!(s.number, Some(2));
    }

    #[test]
    fn languages_core_imported_and_choices() {
        let sh = shared();
        let mut m = Mech::new(&sh, "L");
        m.languages(&[
            json!({"common": true, "dwarvish": true, "gith": true, "anyStandard": 1}),
            json!({"choose": {"from": ["elvish", "gith"], "count": 1}}),
            json!({"anyRare": 2}),
            json!({"choose": {"from": ["elvish", "other"]}}),
        ]);
        let ids = ids_of(&m);
        assert!(ids.contains(&"grant:ID_LANGUAGE_COMMON".to_string()));
        assert!(ids.contains(&"grant:ID_LANGUAGE_DWARVISH".to_string()));
        assert!(ids.contains(&"grant:ID_5ET_LANGUAGE_GITH".to_string()));
        assert!(ids.contains(&"select:Language:Language (L):Standard".to_string()));
        assert!(ids.contains(
            &"select:Language:Language 2 (L):ID_LANGUAGE_ELVISH|ID_5ET_LANGUAGE_GITH".to_string()
        ));
        assert!(ids.contains(&"select:Language:Language 3 (L):Exotic||Secret".to_string()));
        assert!(ids.contains(&"select:Language:Language 4 (L):Standard||Exotic".to_string()));
        assert!(m.partial.iter().any(|p| p.contains("widened")));
    }

    #[test]
    fn tools_weapons_armor_saves() {
        let sh = shared();
        let mut m = Mech::new(&sh, "T");
        m.tools(&[
            json!({"thieves' tools": true, "anyGamingSet": 1}),
            json!({"choose": {"from": ["smith's tools", "brewer's supplies", "mason's tools"]}}),
            json!({"choose": {"from": ["anyArtisansTool", "anyMusicalInstrument"], "count": 2}}),
            json!({"vehicles (air)": true}),
        ]);
        m.weapons(&[
            json!({"simple": true, "martial": true, "battleaxe|phb": true, "light crossbow|phb": true}),
            json!({"choose": {"fromFilter": "type=martial weapon", "count": 1}}),
        ]);
        m.armor(&[json!({"light": true, "shield": true, "plate|phb": true})]);
        m.saving_throws(&["str".into(), "con".into(), "zzz".into()]);
        let ids = ids_of(&m);
        assert!(ids.contains(&"grant:ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS".to_string()));
        assert!(ids.contains(&"select:Proficiency:Tool Proficiency (T):Gaming Set".to_string()));
        assert!(ids.contains(&"select:Proficiency:Tool Proficiency 2 (T):ID_PROFICIENCY_TOOL_PROFICIENCY_SMITHS_TOOLS|ID_PROFICIENCY_TOOL_PROFICIENCY_BREWERS_SUPPLIES|ID_PROFICIENCY_TOOL_PROFICIENCY_MASONS_TOOLS".to_string()));
        assert!(ids
            .iter()
            .any(|i| i.contains("Artisan tools||Musical Instrument")));
        assert!(ids.contains(&"grant:ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS".to_string()));
        assert!(ids.contains(&"grant:ID_PROFICIENCY_WEAPON_PROFICIENCY_CROSSBOW_LIGHT".to_string()));
        assert!(ids.contains(&"grant:ID_PROFICIENCY_ARMOR_PROFICIENCY_SHIELDS".to_string()));
        assert!(ids.contains(&"grant:ID_PROFICIENCY_SAVINGTHROW_CONSTITUTION".to_string()));
        assert!(m.unmapped.contains_key("weapon"));
        assert!(m.unmapped.contains_key("tool"));
        assert!(m.unmapped.contains_key("armor"));
        assert!(m.unmapped.contains_key("saving throw"));
    }

    #[test]
    fn resistances_senses_size_speed() {
        let sh = shared();
        let mut m = Mech::new(&sh, "R");
        m.resistances(
            &[
                json!("fire"),
                json!({"choose": {"from": ["poison", "necrotic", "fire"]}}),
                json!("bludgeoning, piercing, and slashing from nonmagical attacks"),
            ],
            &[json!("cold")],
            &[],
            &[json!("charmed")],
        );
        m.senses(
            Some(120),
            Some(10),
            &[json!({"truesight": 30, "tremorsense": 60})],
        );
        m.size(&["S".into(), "M".into()]);
        m.speed(Some(&json!({"walk": 30, "fly": true, "swim": {"number": 40, "condition": "(while wet)"}, "canHover": true})));
        m.speed(Some(&json!(25)));
        m.speed(Some(&json!("Varies")));
        let ids = ids_of(&m);
        assert!(ids.contains(&"grant:ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_FIRE".to_string()));
        assert!(ids.contains(&"grant:ID_INTERNAL_CONDITION_DAMAGE_IMMUNITY_COLD".to_string()));
        assert!(ids.iter().any(|i| i.starts_with("select:Condition:Damage Resistance (R):ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_POISON|")));
        assert!(ids.contains(&"grant:ID_VISION_SUPERIORDARKVISION".to_string()));
        assert!(ids.contains(&"grant:ID_VISION_BLINDSIGHT".to_string()));
        assert!(ids.contains(&"stat:blindsight:range=10".to_string()));
        assert!(ids.contains(&"stat:truesight:range=30".to_string()));
        assert!(ids.contains(&"stat:tremorsense:range=60".to_string()));
        assert!(ids.contains(&"select:Size:Size (R):ID_SIZE_SMALL|ID_SIZE_MEDIUM".to_string()));
        assert!(ids.contains(&"stat:innate speed=30".to_string()));
        assert!(ids.contains(&"stat:innate speed:fly=innate speed".to_string()));
        assert!(ids.contains(&"stat:innate speed:swim=40".to_string()));
        assert!(ids.contains(&"stat:innate speed=25".to_string()));
        assert!(m.partial.iter().any(|p| p.contains("swim speed condition")));
        assert!(m.partial.iter().any(|p| p.contains("charmed")));
        assert!(m.partial.iter().any(|p| p.contains("Varies")));
        let mut one = Mech::new(&sh, "D");
        one.senses(Some(60), None, &[]);
        assert_eq!(ids_of(&one), vec!["grant:ID_VISION_DARKVISION".to_string()]);
        let mut odd = Mech::new(&sh, "D");
        odd.senses(Some(90), None, &[]);
        assert_eq!(
            ids_of(&odd),
            vec![
                "grant:ID_VISION_DARKVISION".to_string(),
                "stat:darkvision:range=90".to_string()
            ]
        );
    }

    #[test]
    fn feats_grants_and_choices() {
        let sh = shared();
        let mut m = Mech::new(&sh, "Human");
        m.feats(&[
            json!({"alert|xphb": true}),
            json!({"magic initiate; wizard|xphb": true}),
            json!({"anyFromCategory": {"category": ["O"], "count": 1}}),
            json!({"any": 1}),
        ]);
        let ids = ids_of(&m);
        assert!(ids.contains(&"grant:ID_5ET_XPHB_FEAT_ALERT".to_string()));
        assert!(ids.contains(&"grant:ID_5ET_XPHB_FEAT_MAGIC_INITIATE".to_string()));
        assert!(ids.contains(&"select:Feat:Feat (Human):Origin Feat".to_string()));
        let Rule::Select(any) = &m.rules[3] else {
            panic!()
        };
        assert_eq!(any.supports, None, "any feat means no supports filter");
        let Rule::Grant(g) = &m.rules[1] else {
            panic!()
        };
        assert_eq!(g.setters[0].value, "Wizard");
        assert!(m.partial.iter().any(|p| p.contains("magic initiate")));
    }

    #[test]
    fn gating_and_expertise_and_mixed() {
        let sh = shared();
        let mut m = Mech::new(&sh, "C");
        m.set_gate(Some("!ID_MC"));
        m.skills(&[json!({"stealth": true})]);
        m.expertise(&[json!({"anyProficientSkill": 2, "thieves' tools": true, "stealth": true})]);
        m.skill_tool_language(&[
            json!({"choose": [{"from": ["anySkill", "anyTool"], "count": 3}]}),
            json!({"athletics": true, "anyLanguage": 1}),
        ]);
        for r in &m.rules {
            assert!(
                r.requirements().is_some_and(|x| x.contains("!ID_MC")),
                "{r:?}"
            );
        }
        let ids = ids_of(&m);
        assert!(ids.contains(&"grant:ID_EXPERTISE_SKILL_STEALTH".to_string()));
        assert!(ids.contains(&"grant:ID_EXPERTISE_TOOL_THIEVES_TOOLS".to_string()));
        assert!(ids
            .iter()
            .any(|i| i.contains("Expertise") && i.starts_with("select:Class Feature")));
        assert!(ids.iter().any(|i| i.ends_with("Skill||Tool||Gaming Set")));
        assert!(ids.contains(&"grant:ID_PROFICIENCY_SKILL_ATHLETICS".to_string()));
        assert!(ids.iter().any(|i| i.starts_with("select:Language")));
        assert_eq!(feat_category_label("EB"), "Epic Boon Feat");
        assert_eq!(feat_category_label("zz"), "Other Feat");
    }
}
