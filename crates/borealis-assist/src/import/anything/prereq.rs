//! A printed prerequisite (`prerequisite` text of a feat, option or feature) as element
//! `requirements`, the way the bundled packs gate them: another feat or feature (of this import
//! or the library) by `[feature:<Name>]`, an ability score by `[str:13]`, a character level by
//! `[character:N]` (feats). Terms the model already wrote are kept.

use serde_json::{json, Value};

use super::types::{ImportPlan, PlannedElement, Role};

/// Abbreviations of the ability keys the requirements grammar reads.
const ABILITIES: [(&str, &str); 6] = [
    ("strength", "str"),
    ("dexterity", "dex"),
    ("constitution", "con"),
    ("intelligence", "int"),
    ("wisdom", "wis"),
    ("charisma", "cha"),
];
/// Words naming an element kind after a prerequisite name ("Tough feat").
const KIND_WORDS: [&str; 3] = [" feat", " feature", " trait"];

fn norm(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// "Strength 13" / "Strength 13 or higher" -> `[str:13]`.
fn ability_term(t: &str) -> Option<String> {
    let n = norm(t);
    let mut words = n.split(' ');
    let ability = words.next()?;
    let (_, abbr) = ABILITIES
        .iter()
        .find(|(a, b)| *a == ability || *b == ability)?;
    let score: u32 = words.next()?.parse().ok()?;
    let rest: Vec<&str> = words.collect();
    (rest.is_empty() || rest == ["or", "higher"]).then(|| format!("[{abbr}:{score}]"))
}

/// "Level 4", "4th level", "Level 4+" -> `[character:4]`.
fn level_term(t: &str) -> Option<String> {
    let n = norm(t);
    let digits: String = n.chars().filter(char::is_ascii_digit).collect();
    let words: Vec<&str> = n
        .split(' ')
        .filter(|w| !w.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .collect();
    let level: u32 = digits.parse().ok()?;
    (words == ["level"] || words == ["character", "level"]).then(|| format!("[character:{level}]"))
}

/// A name without its articles ("Speaker of the Tides" ~ "Speaker of Tides").
fn loose(s: &str) -> String {
    norm(s)
        .split(' ')
        .filter(|w| !matches!(*w, "the" | "a" | "an"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// An element this import plans, by name, that can be a prerequisite.
fn planned_named<'a>(plan: &'a ImportPlan, name: &str, own: &str) -> Option<&'a PlannedElement> {
    let usable = |e: &&PlannedElement| e.id != own && e.role != Role::Text && e.ty != "Spell";
    let want = norm(name);
    let want_loose = loose(name);
    plan.elements
        .iter()
        .filter(usable)
        .find(|e| norm(&e.name) == want)
        .or_else(|| {
            plan.elements
                .iter()
                .filter(usable)
                .find(|e| !want_loose.is_empty() && loose(&e.name) == want_loose)
        })
}

/// The planned names of "A, B, or C" alternatives; a bare alternative may share the first
/// one's leading words ("Speaker of Tides, Stones, or Winds"). `None` unless every one is a
/// planned element.
fn alt_names(alts: &[&str], plan: &ImportPlan, own: &str) -> Option<Vec<String>> {
    let first = norm(alts.first()?);
    let lead: Vec<&str> = first.split(' ').collect();
    let lead = lead[..lead.len().saturating_sub(1)].join(" ");
    alts.iter()
        .map(|a| {
            let a = a.trim().trim_end_matches('.').trim();
            let a = KIND_WORDS
                .iter()
                .find_map(|w| {
                    a.to_lowercase()
                        .ends_with(w)
                        .then(|| a[..a.len() - w.len()].trim())
                })
                .unwrap_or(a);
            planned_named(plan, a, own)
                .or_else(|| {
                    (!lead.is_empty())
                        .then(|| planned_named(plan, &format!("{lead} {a}"), own))
                        .flatten()
                })
                .map(|e| e.name.clone())
        })
        .collect()
}

/// `([feature:A]||[feature:B])` for alternatives that all name planned elements.
fn alt_features(alts: &[&str], plan: &ImportPlan, own: &str) -> Option<String> {
    let names = alt_names(alts, plan, own)?;
    let ok = names
        .iter()
        .all(|n| !n.contains([']', '[', '|', '&', '!', ':']));
    (ok && names.len() > 1).then(|| {
        let terms: Vec<String> = names.iter().map(|n| format!("[feature:{n}]")).collect();
        format!("({})", terms.join("||"))
    })
}

/// A part that reads as a level or an ability score, never one alternative of a name list.
fn is_level_or_score(part: &str) -> bool {
    let n = norm(part);
    n.chars().any(|c| c.is_ascii_digit())
}

/// The prerequisite parts with an "A, B, or C" list joined back into one " or " part.
fn parts_of(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for seg in text.split([',', ';']) {
        let t = seg.trim();
        let Some(last) = t.strip_prefix("or ") else {
            out.push(t.to_string());
            continue;
        };
        // Walk back over the alternatives of the same kind before "or C" (names, or scores).
        let scores = is_level_or_score(last);
        let mut alts = vec![last.to_string()];
        while let Some(prev) = out.last().filter(|p| {
            !p.is_empty()
                && is_level_or_score(p) == scores
                && (!scores || ability_term(p).is_some())
        }) {
            alts.insert(0, prev.clone());
            out.pop();
        }
        out.push(alts.join(" or "));
    }
    out
}

/// The requirement of one prerequisite term, if it is one the grammar can test.
fn term(t: &str, plan: &ImportPlan, planned: &PlannedElement) -> Option<String> {
    let t = t.trim().trim_end_matches('.').trim();
    let t = t.strip_prefix("the ").unwrap_or(t);
    if t.is_empty() {
        return None;
    }
    if let Some(r) = ability_term(t) {
        return Some(r);
    }
    if planned.ty == "Feat" {
        if let Some(r) = level_term(t) {
            return Some(r);
        }
    }
    let lower = t.to_lowercase();
    let named = KIND_WORDS
        .iter()
        .find_map(|w| lower.ends_with(w).then(|| t[..t.len() - w.len()].trim()));
    // By name, not id: an element of the same generation batch is not loaded yet when the
    // batch is validated, and the name also finds library feats and features.
    let name = planned_named(plan, named.unwrap_or(t), &planned.id)
        .map(|e| e.name.as_str())
        .or(named)?;
    (!name.is_empty() && !name.contains([']', '[', '|', '&', '!', ':']))
        .then(|| format!("[feature:{name}]"))
}

/// Split a prerequisite into its required parts (an "or" part stays one term and is skipped
/// unless every alternative is an ability score).
fn requirements_of(text: &str, plan: &ImportPlan, planned: &PlannedElement) -> Vec<String> {
    let mut out = Vec::new();
    let parts = parts_of(text);
    for part in parts.iter().flat_map(|p| p.split(" and ")) {
        let alts: Vec<&str> = part.split(" or ").collect();
        if alts.len() > 1 && !part.trim().ends_with("or higher") {
            if let Some(a) = alt_abilities(&alts).or_else(|| alt_features(&alts, plan, &planned.id))
            {
                out.push(a);
            }
            continue;
        }
        if let Some(r) = term(part, plan, planned) {
            out.push(r);
        }
    }
    out
}

/// "Strength or Dexterity 13" -> `([str:13]||[dex:13])`.
fn alt_abilities(alts: &[&str]) -> Option<String> {
    let last = norm(alts.last()?);
    let score = last.split(' ').nth(1)?.to_string();
    let terms: Option<Vec<String>> = alts
        .iter()
        .map(|a| {
            let a = norm(a);
            let name = a.split(' ').next().unwrap_or("");
            ability_term(&format!("{name} {score}"))
        })
        .collect();
    terms.map(|t| format!("({})", t.join("||")))
}

/// Labels of a printed prerequisite line.
const LABELS: [&str; 2] = ["prerequisites", "prerequisite"];

/// Plain text of the first paragraph of an HTML description.
fn first_paragraph(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut tag = String::new();
    for c in html.trim_start().chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let t = tag.to_lowercase();
                let ends = t.starts_with("/p") || t.starts_with("br") || t.starts_with("/li");
                if ends && !out.trim().is_empty() {
                    break;
                }
            }
            c if in_tag => tag.push(c),
            '\n' if !out.trim().is_empty() => break,
            c => out.push(c),
        }
    }
    out.replace("&amp;", "&").trim().to_string()
}

/// The printed prerequisite: the element's `prerequisite`, else a leading "Prerequisite:"
/// paragraph of its description (the model often keeps it there only).
fn printed(el: &Value) -> Option<String> {
    if let Some(t) = el
        .get("prerequisite")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
    {
        return Some(t.to_string());
    }
    let para = first_paragraph(el.get("description").and_then(Value::as_str)?);
    let lower = para.to_lowercase();
    LABELS.iter().find_map(|l| {
        let rest = lower.strip_prefix(l)?;
        let rest = rest.trim_start().strip_prefix(':')?;
        let at = para.len().checked_sub(rest.len())?;
        Some(para.get(at..)?.trim().to_string()).filter(|t| !t.is_empty())
    })
}

/// Add the requirements of `el`'s printed prerequisite that it does not test yet.
pub(crate) fn apply(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    let Some(text) = printed(el) else {
        return;
    };
    let existing = el
        .get("requirements")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let add: Vec<String> = requirements_of(&text, plan, planned)
        .into_iter()
        .filter(|r| !existing.contains(r.as_str()))
        .collect();
    if add.is_empty() {
        return;
    }
    let mut all = Vec::new();
    if !existing.is_empty() {
        all.push(if existing.contains("||") {
            format!("({existing})")
        } else {
            existing
        });
    }
    all.extend(add);
    if let Some(o) = el.as_object_mut() {
        o.insert("requirements".into(), json!(all.join("&&")));
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::Edition;
    use super::*;

    fn planned(id: &str, name: &str, ty: &str) -> PlannedElement {
        PlannedElement {
            id: id.into(),
            ty: ty.into(),
            name: name.into(),
            role: Role::Standalone,
            level: None,
            owner: None,
            brief: String::new(),
            excerpt: None,
            parent: None,
            picks: None,
            prerequisite_level: None,
            spells: Vec::new(),
        }
    }

    #[test]
    fn prerequisites_become_requirements_on_feats_features_scores_and_levels() {
        let plan = ImportPlan {
            edition: Edition::Y2014,
            groups: Vec::new(),
            elements: vec![
                planned("ID_F_GLOOMSTEP", "Gloomstep", "Feat"),
                planned("ID_F_ASHEN_GRIP", "Ashen Grip", "Feat"),
            ],
            notes: Vec::new(),
        };
        let me = &plan.elements[1];
        let mut el = json!({"prerequisite": "Gloomstep feat, Strength or Dexterity 13, 4th level, Fey Sight feature"});
        apply(&mut el, me, &plan);
        assert_eq!(
            el["requirements"],
            "[feature:Gloomstep]&&([str:13]||[dex:13])&&[character:4]&&[feature:Fey Sight]"
        );
        // Idempotent, and the model's own terms are kept.
        apply(&mut el, me, &plan);
        let mut kept = json!({"prerequisite": "Wisdom 13 or higher", "requirements": "[level:5]"});
        apply(&mut kept, me, &plan);
        assert_eq!(kept["requirements"], "[level:5]&&[wis:13]");
        let req = el["requirements"].as_str().unwrap();
        assert_eq!(req.matches("Gloomstep").count(), 1);
        let mut prose = json!({"prerequisite": "the ability to cast at least one spell"});
        apply(&mut prose, me, &plan);
        assert!(prose.get("requirements").is_none());
    }

    #[test]
    fn a_description_prerequisite_and_name_lists_become_requirements() {
        let plan = ImportPlan {
            edition: Edition::Y2014,
            groups: Vec::new(),
            elements: vec![
                planned("ID_A_TIDES", "Keeper of the Tides", "Archetype"),
                planned("ID_A_STONES", "Keeper of Stones", "Archetype"),
                planned("ID_A_GALES", "Keeper of Gales", "Archetype"),
                planned("ID_F_EMBER", "Gift of Embers", "Class Feature"),
                planned("ID_F_OPT", "Ember Step", "Class Feature"),
            ],
            notes: Vec::new(),
        };
        let me = &plan.elements[4];
        // Only in the description, as the model keeps it.
        let mut el = json!({"description": "<p><i>Prerequisite: 9th level, Gift of Embers feature</i></p><p>You step.</p>",
            "requirements": "[level:9]"});
        apply(&mut el, me, &plan);
        assert_eq!(el["requirements"], "[level:9]&&[feature:Gift of Embers]");
        // "A, B, or C" with the first one's leading words; an article does not matter.
        let mut list = json!({"prerequisite": "9th level, Keeper of Tides, Stones, or Gales"});
        apply(&mut list, me, &plan);
        assert_eq!(
            list["requirements"],
            "([feature:Keeper of the Tides]||[feature:Keeper of Stones]||[feature:Keeper of Gales])"
        );
        // A list with an unknown name adds nothing; scores in a list stay one "or" term.
        let mut unknown = json!({"prerequisite": "Keeper of Stones, Mist, or Gales"});
        apply(&mut unknown, me, &plan);
        assert!(unknown.get("requirements").is_none());
        let mut scores = json!({"prerequisite": "Strength 13, Dexterity 13, or Constitution 13"});
        apply(&mut scores, me, &plan);
        assert_eq!(scores["requirements"], "([str:13]||[dex:13]||[con:13])");
        // Prose that is not a prerequisite line adds nothing.
        let mut prose = json!({"description": "<p>You need no prerequisite: anyone can.</p>"});
        apply(&mut prose, me, &plan);
        assert!(prose.get("requirements").is_none());
    }
}
