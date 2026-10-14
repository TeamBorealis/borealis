//! Weapon groups, weapon categories and weapon properties a source has that no loaded pack
//! covers, made from the source (its names, the import's ids) in the bundled shape.
//!
//! A group is whatever the source names: a weapon's `data.weapon_group`, a
//! `data.weapon_category` outside the standard simple/martial ones, or a "... weapons" term on a
//! class's weapon proficiency line. Unless a loaded pack has it (matched by name) it gets a
//! category tag element and a group proficiency; each of its weapons gets a proficiency of its
//! own (its `proficiency` setter), granted by the group and by the standard category the source
//! also gives it. Classes whose proficiency text names the group are granted it.
//!
//! A property name no pack defines becomes a `Weapon Property` element (the source's paragraph
//! about it as its description) and the weapons using it carry its tag.

use std::collections::HashSet;

use borealis_content::ContentStore;
use serde_json::{json, Value};

use super::{norm, proficiency_text, push_support, rules_mut, str_at, Derived};
use crate::import::anything::items::{known_property, property_key, weapon_properties};
use crate::import::anything::types::ImportPlan;
use crate::import::ids::{make_id, slug};

/// Group proficiency ids Aurora-format content already references, by group key: a group made
/// for that key takes this id so such content resolves. Never a source of groups.
const KNOWN_GROUP_IDS: [(&str, &str); 1] =
    [("firearm", "ID_PROFICIENCY_WEAPON_PROFICIENCY_FIREARMS")];
/// The engine's ranged marker for a weapon outside the standard (simple/martial) categories.
const RANGED_TAG: &str = "ID_INTERNAL_WEAPON_CATEGORY_RANGED";
/// Longest class-text term read as a group name (longer ones are prose).
const MAX_TERM_WORDS: usize = 4;
/// Longest label before the ':' of a proficiency line ("Weapons", "Weapon proficiencies").
const MAX_LABEL_WORDS: usize = 3;
/// Fallback id prefix when the plan's ids show none.
const DEFAULT_PREFIX: &str = "IMPORT";

/// A group of the source and what names it.
struct Group {
    /// As the source prints it (first occurrence).
    name: String,
    /// `group_key` of the name.
    key: String,
    /// Named as a weapon's category (a `Weapon Category` element), else as a group.
    category: bool,
    weapons: Vec<usize>,
    classes: Vec<usize>,
}

/// The id prefix of the import, read back from a planned element's id (`ID_<PREFIX>_<TYPE>_<NAME>`).
fn id_prefix(plan: &ImportPlan) -> String {
    plan.elements
        .iter()
        .find_map(|e| {
            let tail = format!("_{}_{}", slug(&e.ty), slug(&e.name));
            let mid = e.id.strip_prefix("ID_")?.strip_suffix(&tail)?;
            (!mid.is_empty()).then(|| mid.to_string())
        })
        .unwrap_or_else(|| DEFAULT_PREFIX.to_string())
}

/// An import id for `name` that neither the library nor the draft uses.
fn fresh_id(prefix: &str, ty: &str, name: &str, store: &ContentStore, d: &Derived) -> String {
    let mut used: HashSet<String> = d
        .docs
        .iter()
        .chain(&d.extra)
        .map(|e| str_at(e, "id").to_string())
        .collect();
    loop {
        let id = make_id(prefix, ty, name, &used);
        if store.get(&id).is_none() {
            return id;
        }
        used.insert(id);
    }
}

fn singular(word: &str) -> String {
    if word.len() > 3 && word.ends_with('s') && !word.ends_with("ss") {
        word[..word.len() - 1].to_string()
    } else {
        word.to_string()
    }
}

/// Lowercase, singular, without a "Weapon Group (...)" wrapper: "Chain Weapons" -> "chain weapon".
fn group_key(name: &str) -> String {
    let inner = name
        .split_once('(')
        .map_or(name, |(_, rest)| rest.trim_end_matches(')'));
    let words: Vec<String> = norm(inner).split(' ').map(str::to_string).collect();
    let last = words.len().saturating_sub(1);
    words
        .iter()
        .enumerate()
        .map(|(i, w)| if i == last { singular(w) } else { w.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Simple / martial / melee / ranged words only: the categories bundled proficiencies cover.
fn is_standard(key: &str) -> bool {
    key.split(' ')
        .all(|w| matches!(w, "" | "simple" | "martial" | "melee" | "ranged" | "weapon"))
}

fn title(name: &str) -> String {
    name.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// "Firearm" -> "Firearms", "chain weapons" -> "Chain Weapons".
fn plural_title(name: &str) -> String {
    let t = title(name);
    if t.ends_with('s') {
        t
    } else {
        format!("{t}s")
    }
}

fn data_str<'a>(doc: &'a Value, key: &str) -> Option<&'a str> {
    doc.get("data")?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// The weapon's printed category is a standard one (a bundled proficiency covers it).
fn standard_category(doc: &Value) -> bool {
    data_str(doc, "weapon_category").is_some_and(|c| is_standard(&group_key(c)))
}

/// Terms of the class's weapon proficiency lines: "Weapons: Simple weapons, chain weapons".
fn weapon_terms(class: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for line in proficiency_text(class).lines() {
        let Some((label, list)) = line.split_once(':') else {
            continue;
        };
        let label = label.to_lowercase();
        if !label.contains("weapon") || label.split_whitespace().count() > MAX_LABEL_WORDS {
            continue;
        }
        for part in list.split([',', ';']) {
            for term in part.split(" and ").flat_map(|t| t.split(" or ")) {
                let t = term.trim();
                let t = t
                    .strip_prefix("and ")
                    .or_else(|| t.strip_prefix("or "))
                    .unwrap_or(t)
                    .trim()
                    .trim_end_matches('.');
                let words = t.split_whitespace().count();
                if words > 0 && words <= MAX_TERM_WORDS {
                    out.push(t.to_string());
                }
            }
        }
    }
    out
}

/// A loaded `Weapon Category` / `Weapon Group` of that key.
fn library_tag(store: &ContentStore, key: &str) -> Option<String> {
    ["Weapon Category", "Weapon Group"]
        .iter()
        .flat_map(|t| store.by_type(t))
        .find(|s| group_key(&s.element.name) == key)
        .map(|s| s.element.id.clone())
}

/// A loaded "Weapon Proficiency (<group>)" of that key.
fn library_proficiency(store: &ContentStore, key: &str) -> Option<String> {
    store
        .by_type("Proficiency")
        .find(|s| {
            norm(&s.element.name).starts_with("weapon proficiency")
                && group_key(&s.element.name) == key
        })
        .map(|s| s.element.id.clone())
}

fn group_mut<'a>(groups: &'a mut Vec<Group>, name: &str, category: bool) -> &'a mut Group {
    let key = group_key(name);
    let at = match groups.iter().position(|g| g.key == key) {
        Some(at) => at,
        None => {
            groups.push(Group {
                name: name.to_string(),
                key,
                category,
                weapons: Vec::new(),
                classes: Vec::new(),
            });
            groups.len() - 1
        }
    };
    &mut groups[at]
}

/// The groups of the draft's weapons and of its classes' proficiency text.
fn collect(store: &ContentStore, d: &Derived) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (i, doc) in d.docs.iter().enumerate() {
        if str_at(doc, "type") != "Weapon" {
            continue;
        }
        let named = data_str(doc, "weapon_group")
            .map(|g| (g, false))
            .or_else(|| {
                data_str(doc, "weapon_category")
                    .filter(|c| !is_standard(&group_key(c)))
                    .map(|c| (c, true))
            });
        if let Some((name, category)) = named {
            group_mut(&mut groups, name, category).weapons.push(i);
        }
    }
    for (i, doc) in d.docs.iter().enumerate() {
        if str_at(doc, "type") != "Class" {
            continue;
        }
        for term in weapon_terms(doc) {
            let key = group_key(&term);
            if is_standard(&key) {
                continue;
            }
            let known = groups.iter().any(|g| g.key == key)
                || norm(&term).ends_with("weapons")
                || (library_tag(store, &key).is_some()
                    && library_proficiency(store, &key).is_some());
            if known {
                let g = group_mut(&mut groups, &term, false);
                if !g.classes.contains(&i) {
                    g.classes.push(i);
                }
            }
        }
    }
    groups
}

pub(super) fn weapon_groups(plan: &ImportPlan, store: &ContentStore, d: &mut Derived) {
    let prefix = id_prefix(plan);
    for g in collect(store, d) {
        apply_group(&g, &prefix, store, d);
    }
}

fn source_of(d: &Derived, g: &Group) -> String {
    g.weapons
        .iter()
        .chain(&g.classes)
        .map(|i| str_at(&d.docs[*i], "source"))
        .find(|s| !s.is_empty())
        .unwrap_or("Imported")
        .to_string()
}

/// The group's category tag: a loaded one, else a new element named from the source.
fn group_tag(
    g: &Group,
    prefix: &str,
    source: &str,
    store: &ContentStore,
    d: &mut Derived,
) -> String {
    if let Some(id) = library_tag(store, &g.key) {
        return id;
    }
    let (ty, name) = if g.category {
        ("Weapon Category", title(&g.name))
    } else {
        (
            "Weapon Group",
            format!("Weapon Group ({})", plural_title(&g.name)),
        )
    };
    let id = fresh_id(prefix, ty, &name, store, d);
    d.extra
        .push(json!({"id": id, "name": name, "type": ty, "source": source,
        "description": format!("<p>{name} ({ty}).</p>")}));
    d.note(
        &id,
        format!("new {} \"{name}\" from the source", ty.to_lowercase()),
    );
    id
}

fn apply_group(g: &Group, prefix: &str, store: &ContentStore, d: &mut Derived) {
    let plural = plural_title(&g.name);
    let lib = library_proficiency(store, &g.key);
    if lib.is_some() && g.weapons.is_empty() {
        let id = lib.unwrap_or_default();
        for c in &g.classes {
            grant_group(*c, &id, &plural, d);
        }
        return;
    }
    let source = source_of(d, g);
    let tag = group_tag(g, prefix, &source, store, d);
    for i in &g.weapons {
        let w = &mut d.docs[*i];
        push_support(w, &tag);
        let melee = w.pointer("/data/melee").and_then(Value::as_bool);
        if !standard_category(w) && melee == Some(false) {
            push_support(w, RANGED_TAG);
        }
    }
    // A weapon of a non-standard category keeps the fallback proficiency `items.rs` gave it
    // only while nothing grants its group.
    let ungranted = lib.is_none() && g.classes.is_empty();
    let base_of = |doc: &Value| -> Option<String> {
        let keep = standard_category(doc) || ungranted;
        keep.then(|| setter_value(doc, "proficiency")).flatten()
    };
    let needs_group = !ungranted || g.weapons.iter().any(|i| base_of(&d.docs[*i]).is_none());
    if !needs_group {
        d.note(&tag, format!("{plural}: no class of the source grants them, so their weapons keep the proficiency of their category"));
        return;
    }
    let group_id = lib.clone().unwrap_or_else(|| {
        KNOWN_GROUP_IDS
            .iter()
            .find(|(k, id)| *k == g.key && store.get(id).is_none())
            .map(|(_, id)| (*id).to_string())
            .unwrap_or_else(|| {
                fresh_id(
                    prefix,
                    "Proficiency",
                    &format!("Weapon Proficiency ({plural})"),
                    store,
                    d,
                )
            })
    });
    let mut grants = Vec::new();
    for i in &g.weapons {
        let own = weapon_proficiency(*i, &tag, prefix, &source, store, d);
        if let Some(base) = base_of(&d.docs[*i]) {
            if base != own {
                append_grant(d, &base, &own);
            }
        }
        set_proficiency(&mut d.docs[*i], &own);
        grants.push(json!({"kind": "grant", "id": own, "type": "Proficiency"}));
    }
    let name = format!("Weapon Proficiency ({plural})");
    if lib.is_some() {
        for gr in &grants {
            append_grant(d, &group_id, str_at(gr, "id"));
        }
    } else {
        d.extra
            .push(json!({"id": group_id, "name": name, "type": "Proficiency",
            "source": source, "supports": ["Weapon"], "rules": grants,
            "description": format!("<p>Proficiency: {name}.</p>")}));
        d.note(&group_id, format!("new weapon group proficiency \"{name}\" created for {} weapon(s) of the import; attacks with them are proficient when a class or feature grants it", g.weapons.len()));
    }
    for c in &g.classes {
        grant_group(*c, &group_id, &plural, d);
    }
}

/// The weapon's own proficiency: a loaded one of its name, else a new element.
fn weapon_proficiency(
    i: usize,
    tag: &str,
    prefix: &str,
    source: &str,
    store: &ContentStore,
    d: &mut Derived,
) -> String {
    let weapon = str_at(&d.docs[i], "name").to_string();
    let name = format!("Weapon Proficiency ({weapon})");
    let key = norm(&name);
    if let Some(s) = store
        .by_type("Proficiency")
        .find(|s| norm(&s.element.name) == key)
    {
        return s.element.id.clone();
    }
    let id = fresh_id(prefix, "Proficiency", &name, store, d);
    d.extra.push(
        json!({"id": id, "name": name, "type": "Proficiency", "source": source,
        "supports": ["Weapon", tag], "description": format!("<p>Proficiency: {name}.</p>")}),
    );
    id
}

fn setter_value(doc: &Value, name: &str) -> Option<String> {
    doc.get("setters")?
        .as_array()?
        .iter()
        .find(|s| str_at(s, "name") == name)
        .map(|s| str_at(s, "value").to_string())
        .filter(|v| !v.is_empty())
}

fn set_proficiency(weapon: &mut Value, id: &str) {
    let Some(o) = weapon.as_object_mut() else {
        return;
    };
    let setters = o.entry("setters").or_insert_with(|| json!([]));
    if let Some(arr) = setters.as_array_mut() {
        arr.retain(|s| str_at(s, "name") != "proficiency");
        arr.push(json!({"name": "proficiency", "value": id}));
    }
}

/// `target` (a loaded proficiency) also grants `id`, by an append.
fn append_grant(d: &mut Derived, target: &str, id: &str) {
    let grant = json!({"kind": "grant", "id": id, "type": "Proficiency"});
    if let Some(a) = d
        .appends
        .iter_mut()
        .find(|a| str_at(a, "id") == target && str_at(a, "type") == "Proficiency")
    {
        if let Some(rules) = rules_mut(a) {
            rules.push(grant);
        }
        return;
    }
    d.appends
        .push(json!({"id": target, "type": "Proficiency", "rules": [grant]}));
}

/// Grant the group where the class grants its other weapon proficiencies (same requirements).
fn grant_group(class_idx: usize, group_id: &str, group: &str, d: &mut Derived) {
    let class_id = str_at(&d.docs[class_idx], "id").to_string();
    let Some(rules) = rules_mut(&mut d.docs[class_idx]) else {
        return;
    };
    if rules.iter().any(|r| str_at(r, "id") == group_id) {
        return;
    }
    let requirements = rules
        .iter()
        .find(|r| {
            str_at(r, "type") == "Proficiency" && str_at(r, "id").contains("_PROFICIENCY_WEAPON_")
        })
        .and_then(|r| r.get("requirements").cloned());
    let mut g = json!({"kind": "grant", "id": group_id, "type": "Proficiency"});
    if let Some(req) = requirements {
        g["requirements"] = req;
    }
    rules.push(g);
    d.note(
        &class_id,
        format!("the class's proficiency with {group} is granted (\"{group_id}\")"),
    );
}

// ---- weapon properties ------------------------------------------------------------------------

/// Plain text of an HTML description, one entry per paragraph.
fn paragraphs(html: &str) -> Vec<String> {
    let mut plain = String::new();
    let mut in_tag = false;
    for c in html
        .replace("</p>", "\n")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .chars()
    {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => plain.push(c),
            _ => {}
        }
    }
    plain
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// The source's paragraph about property `name` on the draft's weapons ("Name. What it does.").
fn property_text(d: &Derived, name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    let min_words = name.split_whitespace().count() + 3;
    d.docs
        .iter()
        .filter(|e| str_at(e, "type") == "Weapon")
        .flat_map(|e| paragraphs(str_at(e, "description")))
        .find(|p| p.to_lowercase().starts_with(&lower) && p.split_whitespace().count() >= min_words)
}

/// Property names on the draft's weapons that neither core, the source's own `Weapon Property`
/// elements nor a loaded pack define become elements; the weapons carry their tags.
pub(super) fn new_properties(plan: &ImportPlan, store: &ContentStore, d: &mut Derived) {
    let prefix = id_prefix(plan);
    let own: Vec<String> = d
        .docs
        .iter()
        .filter(|e| str_at(e, "type") == "Weapon Property")
        .map(|e| property_key(str_at(e, "name")))
        .collect();
    let mut made: Vec<(String, String)> = Vec::new();
    for i in 0..d.docs.len() {
        if str_at(&d.docs[i], "type") != "Weapon" {
            continue;
        }
        for p in weapon_properties(&d.docs[i]) {
            let key = property_key(&p);
            if key.is_empty() || known_property(&p).is_some() || own.contains(&key) {
                continue;
            }
            let id = match made.iter().find(|(k, _)| *k == key) {
                Some((_, id)) => id.clone(),
                None => {
                    let id = property_element(&p, &key, i, &prefix, store, d);
                    made.push((key, id.clone()));
                    id
                }
            };
            push_support(&mut d.docs[i], &id);
        }
    }
}

/// A loaded property of that name, else a new element with the source's text.
fn property_element(
    printed: &str,
    key: &str,
    weapon: usize,
    prefix: &str,
    store: &ContentStore,
    d: &mut Derived,
) -> String {
    if let Some(s) = store
        .by_type("Weapon Property")
        .find(|s| property_key(&s.element.name) == key)
    {
        return s.element.id.clone();
    }
    let name = title(printed.split('(').next().unwrap_or(printed).trim());
    let id = fresh_id(prefix, "Weapon Property", &name, store, d);
    let text = property_text(d, &name);
    let description = text.as_deref().map_or_else(
        || format!("<p>{name} (Weapon Property).</p>"),
        |t| format!("<p>{t}</p>"),
    );
    let source = str_at(&d.docs[weapon], "source").to_string();
    d.extra.push(
        json!({"id": id, "name": name, "type": "Weapon Property", "source": source,
        "supports": ["Weapon Property"], "data": {"property": key}, "description": description}),
    );
    let how = if text.is_some() {
        "with the source's text"
    } else {
        "by name (the source does not define it)"
    };
    d.note(
        &id,
        format!("new weapon property \"{name}\" made {how}; the engine applies no rules for it"),
    );
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_names_match_singular_and_wrapped_forms() {
        assert_eq!(group_key("Chain Weapons"), "chain weapon");
        assert_eq!(group_key("chain weapon"), "chain weapon");
        assert_eq!(group_key("Weapon Group (Swords)"), "sword");
        assert_eq!(group_key("Firearm"), "firearm");
        assert!(is_standard(&group_key("Martial Melee")));
        assert!(is_standard(&group_key("Simple weapons")));
        assert!(!is_standard(&group_key("Firearms")));
        assert_eq!(plural_title("chain weapon"), "Chain Weapons");
    }

    #[test]
    fn class_lines_split_into_terms() {
        let class = json!({"description": "<p><strong>Weapons:</strong> Simple weapons, hand crossbows, and chain weapons</p><p>Weapon Mastery: Your training with weapons lets you use the mastery properties of two kinds of Simple or Martial weapons of your choice.</p>"});
        let terms = weapon_terms(&class);
        assert!(terms.contains(&"chain weapons".to_string()), "{terms:?}");
        assert!(terms.contains(&"hand crossbows".to_string()));
        assert!(terms
            .iter()
            .all(|t| t.split_whitespace().count() <= MAX_TERM_WORDS));
    }
}
