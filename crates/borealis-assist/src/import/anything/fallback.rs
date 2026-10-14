//! "You gain proficiency in the X skill. If you already have it, choose another skill": the
//! grant holds only when the character is not proficient yet, and a marker element (granted
//! only when the character already is, from elsewhere) opens one Proficiency `select` on the
//! same element. Grant requirements are read without the element's own grants (the bundled
//! "proficiency or double" helpers rely on the same rule), so the two never both apply.

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::import::ids::make_id;

const SKILL_PREFIX: &str = "ID_PROFICIENCY_SKILL_";

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn plain_lower(html: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => tag = true,
            '>' => {
                tag = false;
                out.push(' ');
            }
            _ if !tag => out.extend(c.to_lowercase()),
            _ => {}
        }
    }
    out.replace('\u{2019}', "'")
}

/// The text offers another skill when the granted one is already known.
fn offers_fallback(text: &str) -> bool {
    text.split(['.', ';']).any(|s| {
        s.contains("already")
            && (s.contains("another skill")
                || s.contains("skill of your choice")
                || s.contains("different skill")
                || s.contains("other skill"))
    })
}

/// The class whose skill list the text names ("from the <class>'s skill list"), else `own`.
fn list_class(text: &str, own: &str) -> String {
    let Some(end) = text.find(" skill list") else {
        return own.to_string();
    };
    let head = &text[..end];
    let name = head
        .rfind("the ")
        .map(|i| head[i + 4..].trim_end_matches("'s").trim().to_string())
        .unwrap_or_default();
    if name.is_empty() {
        own.to_string()
    } else {
        name
    }
}

/// `supports` of the fallback choice: the class's own skill choice when the pack has it, else
/// any skill.
fn skill_supports(docs: &[Value], class: &str) -> String {
    docs.iter()
        .filter(|d| str_at(d, "type") == "Class" && str_at(d, "name").eq_ignore_ascii_case(class))
        .flat_map(|d| {
            d.get("rules")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .find(|r| {
            str_at(r, "kind") == "select"
                && str_at(r, "type") == "Proficiency"
                && str_at(r, "supports").contains(SKILL_PREFIX)
        })
        .map_or_else(
            || "Skill".to_string(),
            |r| str_at(&r, "supports").to_string(),
        )
}

/// Rewrite every element whose text offers a fallback skill; returns the marker elements to
/// add and one `(element id, note)` per rewrite.
pub(crate) fn skill_fallbacks(
    docs: &mut [Value],
    prefix: &str,
    used: &mut HashSet<String>,
) -> (Vec<Value>, Vec<(String, String)>) {
    let mut markers = Vec::new();
    let mut notes = Vec::new();
    for i in 0..docs.len() {
        let text = plain_lower(str_at(&docs[i], "description"));
        if !offers_fallback(&text) {
            continue;
        }
        let own = docs[i]
            .pointer("/data/class")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let supports = skill_supports(docs, &list_class(&text, &own));
        if let Some((marker, note)) = rewrite(&mut docs[i], &supports, prefix, used) {
            notes.push((str_at(&docs[i], "id").to_string(), note));
            markers.push(marker);
        }
    }
    (markers, notes)
}

fn rewrite(
    el: &mut Value,
    supports: &str,
    prefix: &str,
    used: &mut HashSet<String>,
) -> Option<(Value, String)> {
    let name = str_at(el, "name").to_string();
    let source = str_at(el, "source").to_string();
    let rules = el.get_mut("rules")?.as_array_mut()?;
    let modelled = rules.iter().any(|r| {
        str_at(r, "kind") == "select"
            && str_at(r, "type") == "Proficiency"
            && r.get("requirements").is_some()
    });
    let grants: Vec<usize> = rules
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            str_at(r, "kind") == "grant"
                && str_at(r, "type") == "Proficiency"
                && str_at(r, "id").starts_with(SKILL_PREFIX)
                && r.get("requirements").is_none()
        })
        .map(|(i, _)| i)
        .collect();
    let [g] = grants.as_slice() else { return None };
    if modelled {
        return None;
    }
    let skill = str_at(&rules[*g], "id").to_string();
    let level = rules[*g].get("level").cloned();
    let marker_name = format!("{name} (already proficient)");
    let marker_id = make_id(prefix, "Grants", &marker_name, used);
    used.insert(marker_id.clone());
    rules[*g]["requirements"] = json!(format!("!{skill}"));
    let mut open =
        json!({"kind": "grant", "type": "Grants", "id": marker_id, "requirements": skill});
    let mut pick = json!({"kind": "select", "type": "Proficiency",
        "name": format!("Skill Proficiency ({name})"), "supports": supports,
        "requirements": marker_id});
    if let Some(l) = level {
        open["level"] = l.clone();
        pick["level"] = l;
    }
    rules.push(open);
    rules.push(pick);
    let marker = json!({"id": marker_id, "name": marker_name, "type": "Grants", "source": source});
    Some((
        marker,
        format!("already proficient in {skill}: a choice of another skill instead (the grant applies only otherwise)"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_already_proficient_sentence_becomes_a_guarded_grant_and_a_choice() {
        let class = json!({"id": "ID_T_CLASS_DRIFTER", "name": "Drifter", "type": "Class",
            "rules": [{"kind": "select", "type": "Proficiency", "name": "Skill Proficiency (Drifter)",
                "number": 2, "supports": "ID_PROFICIENCY_SKILL_STEALTH|ID_PROFICIENCY_SKILL_INSIGHT"}]});
        let feature = json!({"id": "ID_T_FEATURE_KEEN_EAR", "name": "Keen Ear",
            "type": "Archetype Feature", "source": "T", "data": {"class": "Drifter"},
            "description": "<p>You gain proficiency in the Insight skill. If you already have it, you \
instead gain proficiency in one skill of your choice from the drifter\u{2019}s skill list.</p>",
            "rules": [{"kind": "grant", "type": "Proficiency", "id": "ID_PROFICIENCY_SKILL_INSIGHT", "level": 3}]});
        let plainly = json!({"id": "ID_T_FEATURE_PLAIN", "name": "Plain", "type": "Feat",
            "description": "<p>You gain proficiency in Stealth.</p>",
            "rules": [{"kind": "grant", "type": "Proficiency", "id": "ID_PROFICIENCY_SKILL_STEALTH"}]});
        let mut docs = vec![class, feature, plainly.clone()];
        let mut used = HashSet::new();
        let (markers, notes) = skill_fallbacks(&mut docs, "T", &mut used);
        assert_eq!(markers.len(), 1);
        assert_eq!(notes.len(), 1);
        let marker_id = markers[0]["id"].as_str().unwrap();
        assert_eq!(markers[0]["type"], "Grants");
        let rules = docs[1]["rules"].as_array().unwrap();
        assert_eq!(rules[0]["requirements"], "!ID_PROFICIENCY_SKILL_INSIGHT");
        assert_eq!(
            rules[1],
            json!({"kind": "grant", "type": "Grants", "id": marker_id,
                "requirements": "ID_PROFICIENCY_SKILL_INSIGHT", "level": 3})
        );
        assert_eq!(rules[2]["type"], "Proficiency");
        assert_eq!(rules[2]["requirements"], marker_id);
        assert_eq!(
            rules[2]["supports"],
            "ID_PROFICIENCY_SKILL_STEALTH|ID_PROFICIENCY_SKILL_INSIGHT"
        );
        assert_eq!(docs[2], plainly);
        // Idempotent: the guarded grant is no longer a bare one.
        let (again, _) = skill_fallbacks(&mut docs, "T", &mut used);
        assert!(again.is_empty());
    }

    /// The engine offers the choice only to a character who already has the skill.
    #[test]
    fn the_engine_opens_the_choice_only_when_already_proficient() {
        use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
        use borealis_engine::{commands, Engine};
        let mut docs = vec![
            json!({"id": "ID_T_OPT_KEEN", "name": "Keen Ear", "type": "Option",
            "source": "T", "description": "<p>You gain proficiency in Insight. If you already have \
it, choose another skill.</p>",
            "rules": [{"kind": "grant", "type": "Proficiency", "id": "ID_PROFICIENCY_SKILL_INSIGHT"}]}),
        ];
        let (markers, _) = skill_fallbacks(&mut docs, "T", &mut HashSet::new());
        docs.extend(markers);
        docs.push(json!({"id": "ID_T_OPT_SAGE", "name": "Sage", "type": "Option", "source": "T",
            "rules": [{"kind": "grant", "type": "Proficiency", "id": "ID_PROFICIENCY_SKILL_INSIGHT"}]}));
        let core = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
        let file: ContentFile = serde_json::from_value(
            json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": docs}),
        )
        .expect("content");
        let store = ContentStoreBuilder::new()
            .add_pack_dir(&core)
            .expect("core")
            .add_file("t.json", PackOrigin::Custom, file)
            .build();
        let engine = Engine::new(&store);
        let slots = |options: &[&str]| {
            let mut s = commands::new_character(&engine, Ruleset::Dnd2014);
            s.options.extend(options.iter().map(|o| (*o).to_string()));
            let snap = engine.derive(&s);
            let skill = snap.skill("insight").is_some_and(|k| k.proficient);
            let n = snap
                .slots
                .iter()
                .filter(|x| x.owner.name == "Keen Ear" && x.ty == "Proficiency")
                .count();
            (skill, n)
        };
        assert_eq!(slots(&["ID_T_OPT_KEEN"]), (true, 0));
        assert_eq!(slots(&["ID_T_OPT_SAGE", "ID_T_OPT_KEEN"]), (true, 1));
    }
}
