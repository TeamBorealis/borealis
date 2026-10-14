//! A "choose one of X, Y or Z" whose options are existing elements (skills, tools, spells,
//! languages) is ONE select over those elements, the way the bundled packs write it
//! (`{"kind": "select", "type": "Proficiency", "supports": "ID_A|ID_B|ID_C"}`), never a family of
//! generated one-grant options that hide the ids.

use borealis_content::ContentStore;

use super::fewshot::in_edition;
use super::types::{Edition, OutlinePart};

/// Element types an option list can name directly.
const TARGET_TYPES: [&str; 3] = ["Proficiency", "Spell", "Language"];
/// Words that do not tell two proficiencies apart ("Painter's Tools" ~ "Painter's Supplies").
const FILLER: [&str; 12] = [
    "tools",
    "tool",
    "supplies",
    "kit",
    "set",
    "the",
    "proficiency",
    "skill",
    "weapon",
    "armor",
    "language",
    "cantrip",
];
/// Least options of a list that must be existing elements.
const MIN_RESOLVED: usize = 2;

/// Match key of a name: its words without filler, letters and digits only, joined.
fn key(name: &str) -> String {
    name.to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '-' || c == '/')
        .map(|w| {
            w.chars()
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
        })
        .filter(|w| !w.is_empty() && !FILLER.contains(&w.as_str()))
        .collect()
}

/// The name inside a bundled proficiency's parentheses ("Tool Proficiency (Smith's Tools)").
fn inner(name: &str) -> &str {
    name.split_once('(')
        .and_then(|(_, r)| r.rsplit_once(')'))
        .map_or(name, |(i, _)| i)
}

/// The options of a list that resolve to existing elements of one type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    /// Element type of the select.
    pub ty: &'static str,
    /// Ids of the resolved options, in the list's order.
    pub ids: Vec<String>,
    /// Option names with no existing element (kept as text).
    pub unresolved: Vec<String>,
}

fn find(store: &ContentStore, edition: Edition, ty: &str, name: &str) -> Option<String> {
    let want = key(name);
    if want.is_empty() {
        return None;
    }
    let hits: Vec<&borealis_content::Element> = store
        .by_type(ty)
        .filter(|s| in_edition(&s.element, edition))
        .map(|s| &s.element)
        .filter(|e| key(&e.name) == want || key(inner(&e.name)) == want)
        .filter(|e| !e.name.to_lowercase().contains("expertise"))
        .collect();
    // A plain element before an internal helper of the same name.
    hits.iter()
        .find(|e| !e.id.starts_with("ID_INTERNAL_"))
        .or_else(|| hits.first())
        .map(|e| e.id.clone())
}

/// The type most options resolve to, when at least two thirds of them (and two or more) do.
pub(crate) fn resolve(
    store: &ContentStore,
    edition: Edition,
    options: &[&OutlinePart],
) -> Option<Target> {
    let best = TARGET_TYPES
        .iter()
        .map(|ty| {
            let found: Vec<Option<String>> = options
                .iter()
                .map(|o| find(store, edition, ty, &o.name))
                .collect();
            (*ty, found)
        })
        .max_by_key(|(_, found)| found.iter().flatten().count())?;
    let (ty, found) = best;
    let resolved = found.iter().flatten().count();
    if resolved < MIN_RESOLVED || resolved * 3 < options.len() * 2 {
        return None;
    }
    let mut ids: Vec<String> = Vec::new();
    for id in found.iter().flatten() {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    let unresolved = options
        .iter()
        .zip(&found)
        .filter(|(_, f)| f.is_none())
        .map(|(o, _)| o.name.clone())
        .collect();
    Some(Target {
        ty,
        ids,
        unresolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin};
    use serde_json::json;

    fn part(name: &str) -> OutlinePart {
        OutlinePart {
            name: name.into(),
            summary: String::new(),
            quote: String::new(),
            level: None,
        }
    }

    fn store() -> ContentStore {
        let file = ContentFile::from_json_str(
            &json!({"format": "borealis-content/1", "elements": [
                {"id": "ID_T_SKILL_LEAPING", "type": "Proficiency", "name": "Skill Proficiency (Leaping)", "source": "T"},
                {"id": "ID_T_SKILL_HIDING", "type": "Proficiency", "name": "Skill Proficiency (Hiding)", "source": "T"},
                {"id": "ID_T_TOOL_GLASS", "type": "Proficiency", "name": "Tool Proficiency (Glassblower's Supplies)", "source": "T"},
                {"id": "ID_T_SPELL_EMBER", "type": "Spell", "name": "Ember Bolt", "source": "T"},
                {"id": "ID_T_SPELL_FROST", "type": "Spell", "name": "Frost Lash", "source": "T"}
            ]})
            .to_string(),
        )
        .expect("test content parses");
        ContentStoreBuilder::new()
            .add_file("lib.json", PackOrigin::Custom, file)
            .build()
    }

    #[test]
    fn option_names_resolve_to_one_target_type() {
        let s = store();
        let opts = [part("Leaping"), part("Hiding"), part("Glassblower's Tools")];
        let refs: Vec<&OutlinePart> = opts.iter().collect();
        let t = resolve(&s, Edition::Y2014, &refs).expect("resolves");
        assert_eq!(t.ty, "Proficiency");
        assert_eq!(
            t.ids,
            ["ID_T_SKILL_LEAPING", "ID_T_SKILL_HIDING", "ID_T_TOOL_GLASS"]
        );
        // A misspelt and an unknown spell: two of three still resolve.
        let spells = [part("Emberbolt"), part("Frost Lash"), part("Cinder Hymn")];
        let refs: Vec<&OutlinePart> = spells.iter().collect();
        let t = resolve(&s, Edition::Y2014, &refs).expect("resolves");
        assert_eq!((t.ty, t.ids.len()), ("Spell", 2));
        assert_eq!(t.unresolved, ["Cinder Hymn"]);
        // Feature-like options are no target.
        let own = [part("Way of Ash"), part("Way of Frost"), part("Leaping")];
        let refs: Vec<&OutlinePart> = own.iter().collect();
        assert!(resolve(&s, Edition::Y2014, &refs).is_none());
    }
}
