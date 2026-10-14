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

/// An element this import plans, by name, that can be a prerequisite.
fn planned_named<'a>(plan: &'a ImportPlan, name: &str, own: &str) -> Option<&'a PlannedElement> {
    let want = norm(name);
    plan.elements
        .iter()
        .find(|e| e.id != own && e.role != Role::Text && e.ty != "Spell" && norm(&e.name) == want)
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
    for part in text.split([',', ';']).flat_map(|p| p.split(" and ")) {
        let alts: Vec<&str> = part.split(" or ").collect();
        if alts.len() > 1 && !part.trim().ends_with("or higher") {
            let abilities = alt_abilities(&alts);
            if let Some(a) = abilities {
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

/// Add the requirements of `el`'s printed `prerequisite` that it does not test yet.
pub(crate) fn apply(el: &mut Value, planned: &PlannedElement, plan: &ImportPlan) {
    let Some(text) = el
        .get("prerequisite")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|t| !t.trim().is_empty())
    else {
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
}
