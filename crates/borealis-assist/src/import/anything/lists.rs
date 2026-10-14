//! Class spell lists printed in the source (a section of spell names under a class name, kept
//! as a text-only element) and a spell's own "Classes:" line both make a spell a member of the
//! class's list, for classes of this import and of the library alike. Membership is the union
//! of both; where they disagree the report says so.
//!
//! A draft spell joins a list through its `supports`, a library spell through an `append`
//! (bundled spells carry their class lists in `supports`). Only names of known spells are read
//! from a list section, so prose around the list adds nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use borealis_content::ContentStore;
use serde_json::{json, Value};

use super::derived::Derived;
use super::fewshot::in_edition;
use super::types::{ImportPlan, Role};

/// Fewest and most letters of a class mark glued to a spell name ("Sh", "Wiz").
const MIN_MARKER_CHARS: usize = 2;
const MAX_MARKER_CHARS: usize = 3;
/// Fewest distinct spells a mark must follow to be a class mark (not a glued word).
const MIN_MARKED_SPELLS: usize = 2;
/// Most words of a spell name read before a mark.
const MAX_SPELL_WORDS: usize = 5;

/// Least share of a line's words that must be spell names for the line to count as a list.
const MIN_LIST_SHARE: f64 = 0.5;
/// Heading words after a class name that still name its list ("Wizard Spells").
const LIST_SUFFIXES: [&str; 4] = [" spell list", " spells", " spell", " list"];

fn norm(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// A normalised list name without its list suffix ("wizard spells" -> "wizard").
fn list_base(n: &str) -> &str {
    LIST_SUFFIXES
        .iter()
        .find_map(|s| n.strip_suffix(s))
        .filter(|b| !b.is_empty())
        .unwrap_or(n)
}

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn supports_of(v: &Value) -> Vec<String> {
    v.get("supports")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Text segments of an HTML description (paragraphs, line breaks and bold headings apart).
fn segments(html: &str) -> Vec<String> {
    marked_segments(html).into_iter().map(|(s, _)| s).collect()
}

/// [`segments`], each with whether it is a bold heading (a `<strong>` run).
fn marked_segments(html: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let t = tag.trim_start_matches('/').to_lowercase();
                if [
                    "p", "br", "br/", "strong", "li", "h1", "h2", "h3", "h4", "tr", "td",
                ]
                .iter()
                .any(|b| t.split_whitespace().next() == Some(b))
                {
                    let closes_bold =
                        tag.starts_with('/') && (t.starts_with("strong") || t.starts_with('h'));
                    out.push((std::mem::take(&mut cur), closes_bold));
                }
            }
            c if in_tag => tag.push(c),
            c => cur.push(c),
        }
    }
    out.push((cur, false));
    out.into_iter()
        .map(|(s, h)| (s.replace("&amp;", "&").trim().to_string(), h))
        .filter(|(s, _)| !s.is_empty())
        .collect()
}

/// Fewest spells a section must list under a heading that names no known list for the
/// heading to open a list of its own ("Vigil Spells").
const MIN_PRINTED_LIST_SPELLS: usize = 3;
/// Most words of a heading that opens a list of its own.
const MAX_LIST_HEADING_WORDS: usize = 5;

/// Level and structure words of a list's sub-headings ("1st Level", "Cantrips (0 Level)").
fn level_phrase(n: &str) -> bool {
    n.split(' ').any(|w| {
        w.starts_with(|c: char| c.is_ascii_digit()) || matches!(w, "level" | "cantrip" | "cantrips")
    })
}

/// A heading that opens a spell list named as printed though no class or known list has that
/// name ("Vigil Spells"): a short title ending in a list suffix, not a level heading.
fn printed_list(text: &str) -> Option<String> {
    let n = norm(text);
    let base = LIST_SUFFIXES
        .iter()
        .filter(|s| **s != " spell")
        .find_map(|s| n.strip_suffix(s))?;
    let words = text.split_whitespace().count();
    (!base.is_empty() && words <= MAX_LIST_HEADING_WORDS && !level_phrase(&n))
        .then(|| text.split_whitespace().collect::<Vec<_>>().join(" "))
}

struct Lists {
    /// norm(list) -> list name as the content writes it.
    names: HashMap<String, String>,
}

impl Lists {
    fn new(plan: &ImportPlan, store: &ContentStore, docs: &[Value]) -> Self {
        let mut names = HashMap::new();
        let mut add = |n: &str| {
            if !n.trim().is_empty() {
                names.entry(norm(n)).or_insert_with(|| n.trim().to_string());
            }
        };
        store
            .by_type("Class")
            .filter(|c| in_edition(&c.element, plan.edition))
            .for_each(|c| add(&c.element.name));
        for doc in docs {
            match str_at(doc, "type") {
                "Class" => add(str_at(doc, "name")),
                "Spell" => supports_of(doc).iter().for_each(|s| add(s)),
                _ => {}
            }
        }
        for g in &plan.groups {
            if let Some(list) = g
                .outline
                .as_ref()
                .and_then(|o| o.caster())
                .and_then(|p| p.list.as_deref())
            {
                add(list);
            }
        }
        Self { names }
    }

    /// The one list whose name starts with `marker` (a class abbreviation, "Wiz" for Wizard).
    /// Names that differ only by a list suffix ("Wizard", "Wizard Spells") are one list; the
    /// bare name is preferred.
    fn abbreviated(&self, marker: &str) -> Option<String> {
        let m = norm(marker);
        if m.chars().count() < MIN_MARKER_CHARS {
            return None;
        }
        let bases: BTreeSet<&str> = self
            .names
            .keys()
            .filter(|n| n.starts_with(&m))
            .map(|n| list_base(n))
            .collect();
        let mut it = bases.into_iter();
        let base = it.next()?;
        if it.next().is_some() {
            return None;
        }
        self.names.get(base).cloned().or_else(|| {
            let mut same: Vec<(&String, &String)> = self
                .names
                .iter()
                .filter(|(n, _)| list_base(n) == base)
                .collect();
            same.sort();
            same.first().map(|(_, l)| (*l).clone())
        })
    }

    /// The list a heading names ("Wizard", "Wizard Spells", "The Wizard Spell List").
    fn heading(&self, text: &str) -> Option<String> {
        let mut n = norm(text);
        if let Some(rest) = n.strip_prefix("the ") {
            n = rest.to_string();
        }
        if let Some(l) = self.names.get(&n) {
            return Some(l.clone());
        }
        LIST_SUFFIXES
            .iter()
            .find_map(|s| n.strip_suffix(s))
            .and_then(|base| self.names.get(base))
            .cloned()
    }
}

/// Known spell names, longest first (so "Mass Cure Wounds" is read before "Cure Wounds").
fn spell_names(store: &ContentStore, plan: &ImportPlan, docs: &[Value]) -> Vec<String> {
    let mut set: BTreeSet<String> = docs
        .iter()
        .filter(|d| str_at(d, "type") == "Spell")
        .map(|d| norm(str_at(d, "name")))
        .collect();
    set.extend(
        store
            .by_type("Spell")
            .filter(|s| in_edition(&s.element, plan.edition))
            .map(|s| norm(&s.element.name)),
    );
    let mut v: Vec<String> = set.into_iter().filter(|n| !n.is_empty()).collect();
    v.sort_by_key(|n| std::cmp::Reverse(n.len()));
    v
}

/// The spells a line names, if the line is mostly spell names.
fn spells_in(line: &str, names: &[String]) -> Vec<String> {
    let text = format!(" {} ", norm(line));
    let words = text.split_whitespace().count();
    if words == 0 {
        return Vec::new();
    }
    let mut taken = vec![false; text.len()];
    let mut found = Vec::new();
    let mut covered = 0;
    for name in names {
        let needle = format!(" {name} ");
        let mut from = 0;
        while let Some(pos) = text[from..].find(&needle).map(|p| p + from) {
            let span = pos + 1..pos + needle.len() - 1;
            if !taken[span.clone()].iter().any(|t| *t) {
                taken[span].iter_mut().for_each(|t| *t = true);
                covered += name.split(' ').count();
                found.push(name.clone());
            }
            from = pos + 1;
        }
    }
    // Level headings ("1st level", "cantrips") are list structure, not prose.
    let structure = text
        .split_whitespace()
        .filter(|w| {
            w.chars().next().is_some_and(|c| c.is_ascii_digit())
                || matches!(*w, "level" | "cantrips" | "cantrip")
        })
        .count();
    let share = covered as f64 / (words.saturating_sub(structure).max(1)) as f64;
    if share >= MIN_LIST_SHARE {
        found
    } else {
        Vec::new()
    }
}

/// list -> spell names (normalised) the text sections of the plan print under it.
fn section_members(
    plan: &ImportPlan,
    docs: &[Value],
    lists: &Lists,
    names: &[String],
) -> BTreeMap<String, (String, BTreeSet<String>)> {
    let mut out: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    let text_ids: Vec<&str> = plan
        .elements
        .iter()
        .filter(|e| e.role == Role::Text)
        .map(|e| e.id.as_str())
        .collect();
    // Lists opened by a heading that names no known list keep at least
    // MIN_PRINTED_LIST_SPELLS spells, or nothing.
    let mut printed: BTreeSet<String> = BTreeSet::new();
    let opens = |text: &str, printed: &mut BTreeSet<String>| {
        lists.heading(text).or_else(|| {
            let l = printed_list(text)?;
            printed.insert(l.clone());
            Some(l)
        })
    };
    for doc in docs.iter().filter(|d| text_ids.contains(&str_at(d, "id"))) {
        let mut current = opens(str_at(doc, "name"), &mut printed);
        for (seg, bold) in marked_segments(str_at(doc, "description")) {
            if let Some(l) = opens(&seg, &mut printed) {
                current = Some(l);
                continue;
            }
            let found = spells_in(&seg, names);
            // Another heading (not a level heading, not a spell) ends the list before it.
            if bold && found.is_empty() && !level_phrase(&norm(&seg)) {
                current = None;
                continue;
            }
            let Some(list) = current.as_ref() else {
                continue;
            };
            if !found.is_empty() {
                out.entry(list.clone())
                    .or_insert_with(|| (str_at(doc, "id").to_string(), BTreeSet::new()))
                    .1
                    .extend(found);
            }
        }
    }
    out.retain(|list, (_, spells)| {
        !printed.contains(list)
            || lists.heading(list).is_some()
            || spells.len() >= MIN_PRINTED_LIST_SPELLS
    });
    out
}

/// The lists a spell's own "Classes:" line names (in its description).
fn classes_line(doc: &Value, lists: &Lists) -> Vec<String> {
    segments(str_at(doc, "description"))
        .iter()
        .filter_map(|s| {
            let lower = s.to_lowercase();
            let at = lower
                .find("classes:")
                .map(|i| i + 8)
                .or_else(|| lower.find("class:").map(|i| i + 6))?;
            Some(s[at..].to_string())
        })
        .flat_map(|rest| {
            rest.split([',', '/', ';'])
                .flat_map(|p| p.split(" and "))
                .flat_map(|p| p.split(" or "))
                .filter_map(|p| lists.heading(p))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn push_append(d: &mut Derived, id: &str, list: &str) {
    if let Some(a) = d.appends.iter_mut().find(|a| str_at(a, "id") == id) {
        let mut s = supports_of(a);
        if !s.iter().any(|t| t == list) {
            s.push(list.to_string());
            a["supports"] = json!(s);
        }
        return;
    }
    d.appends
        .push(json!({"id": id, "type": "Spell", "supports": [list]}));
}

/// The spell lists the plan's casters cast from, by their [`list_base`].
fn casting_lists(plan: &ImportPlan) -> HashMap<String, String> {
    plan.groups
        .iter()
        .filter_map(|g| g.outline.as_ref()?.caster()?.list.clone())
        .filter(|l| !l.trim().is_empty())
        .map(|l| (list_base(&norm(&l)).to_string(), l.trim().to_string()))
        .collect()
}

/// `name` written as the casting list it names with or without a list suffix ("Warden" for a
/// class whose list is "Warden Spells"), when it differs from that list's own spelling.
fn as_casting_list<'a>(name: &str, casting: &'a HashMap<String, String>) -> Option<&'a String> {
    let n = norm(name);
    casting
        .get(list_base(&n))
        .filter(|l| norm(l) != n && !n.is_empty())
}

/// A spell select or spell `supports` that names a caster's list with another suffix than the
/// list itself ("Warden,6" for the "Warden Spells" list) names that list: otherwise the select
/// offers nothing and the spell joins no list. Run before the lists are read.
fn one_list_name(plan: &ImportPlan, d: &mut Derived) {
    let casting = casting_lists(plan);
    if casting.is_empty() {
        return;
    }
    let mut notes = Vec::new();
    for doc in &mut d.docs {
        let id = str_at(doc, "id").to_string();
        if str_at(doc, "type") == "Spell" {
            let s = supports_of(doc);
            let fixed: Vec<String> = s
                .iter()
                .map(|t| {
                    as_casting_list(t, &casting)
                        .cloned()
                        .unwrap_or_else(|| t.clone())
                })
                .collect();
            if fixed != s {
                notes.push((
                    id.clone(),
                    format!("supports the {} list by its printed name", fixed.join(", ")),
                ));
                doc["supports"] = json!(fixed);
            }
        }
        let Some(rules) = doc.get_mut("rules").and_then(Value::as_array_mut) else {
            continue;
        };
        for r in rules.iter_mut().filter(|r| {
            r.get("kind").and_then(Value::as_str) == Some("select")
                && r.get("type").and_then(Value::as_str) == Some("Spell")
        }) {
            let before = str_at(r, "supports").to_string();
            let after = before
                .split('|')
                .map(|part| {
                    let (list, rest) = part.split_once(',').unwrap_or((part, ""));
                    match as_casting_list(list, &casting) {
                        Some(l) if !list.trim_start().starts_with("$(") => {
                            if rest.is_empty() && !part.contains(',') {
                                l.clone()
                            } else {
                                format!("{l},{rest}")
                            }
                        }
                        _ => part.to_string(),
                    }
                })
                .collect::<Vec<_>>()
                .join("|");
            if after != before {
                notes.push((
                    id.clone(),
                    format!("a spell choice names the list as printed: {after}"),
                ));
                r["supports"] = json!(after);
            }
        }
    }
    d.notes.extend(notes);
}

/// Apply printed class spell lists and "Classes:" lines as list membership (see the module
/// docs); every addition and disagreement is reported.
pub(crate) fn class_lists(plan: &ImportPlan, store: &ContentStore, d: &mut Derived) {
    one_list_name(plan, d);
    let lists = Lists::new(plan, store, &d.docs);
    // "Classes:" lines in the spells' own text.
    for doc in d.docs.iter_mut().filter(|e| str_at(e, "type") == "Spell") {
        let mut s = supports_of(doc);
        let before = s.len();
        for l in classes_line(doc, &lists) {
            if !s.contains(&l) {
                s.push(l);
            }
        }
        if s.len() > before {
            doc["supports"] = json!(s);
        }
    }
    let names = spell_names(store, plan, &d.docs);
    let members = section_members(plan, &d.docs, &lists, &names);
    for (list, (section_id, spells)) in members {
        let printed_only = lists.heading(&list).is_none();
        apply_list(plan, store, d, &list, &section_id, &spells, printed_only);
    }
}

/// `word` split into a lowercase-ending stem and a glued mark of capitals ("madnessWz" ->
/// ("madness", "Wz"), "gustEE" -> ("gust", "EE")), as a superscript reads in a text layer.
fn glued_mark(word: &str) -> Option<(&str, &str)> {
    let at = word
        .char_indices()
        .zip(word.chars().skip(1))
        .find(|((_, a), b)| a.is_lowercase() && b.is_uppercase())
        .map(|((i, a), _)| i + a.len_utf8())?;
    let (stem, mark) = word.split_at(at);
    let n = mark.chars().count();
    ((MIN_MARKER_CHARS..=MAX_MARKER_CHARS).contains(&n) && mark.chars().all(char::is_alphabetic))
        .then_some((stem, mark))
}

/// mark -> spells (normalised) printed with that mark glued to their last word.
fn marked_spells(text: &str, names: &BTreeSet<String>) -> BTreeMap<String, BTreeSet<String>> {
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '.' | ':'))
        .filter(|w| !w.is_empty())
        .collect();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (i, w) in words.iter().enumerate() {
        let Some((stem, mark)) = glued_mark(w) else {
            continue;
        };
        // The longest known spell name ending in this stem.
        let spell = (0..MAX_SPELL_WORDS.min(i + 1))
            .rev()
            .map(|k| {
                let mut phrase: Vec<&str> = words[i - k..i].to_vec();
                phrase.push(stem);
                norm(&phrase.join(" "))
            })
            .find(|p| names.contains(p));
        if let Some(spell) = spell {
            out.entry(mark.to_string()).or_default().insert(spell);
        }
    }
    out
}

/// Spells the source marks with a class's abbreviation glued after their name (a superscript
/// such as "Wz" in the text layer: "moth swarmWz") join that class's list, when the mark
/// follows at least [`MIN_MARKED_SPELLS`] spells and names exactly one list. Run after
/// [`class_lists`] on the source text and the elements' descriptions; every addition is
/// reported.
pub(crate) fn marked_lists(source: &str, plan: &ImportPlan, store: &ContentStore, d: &mut Derived) {
    let lists = Lists::new(plan, store, &d.docs);
    let names: BTreeSet<String> = spell_names(store, plan, &d.docs).into_iter().collect();
    let mut texts = vec![source.to_string()];
    texts.retain(|t| !t.is_empty());
    texts.extend(
        d.docs
            .iter()
            .map(|e| segments(str_at(e, "description")).join(" ")),
    );
    let mut marked: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for t in &texts {
        for (mark, spells) in marked_spells(t, &names) {
            marked.entry(mark).or_default().extend(spells);
        }
    }
    for (mark, spells) in marked {
        let Some(list) = lists
            .abbreviated(&mark)
            .filter(|_| spells.len() >= MIN_MARKED_SPELLS)
        else {
            continue;
        };
        let why = format!("joins the {list} list: the source marks it \"{mark}\"");
        for key in &spells {
            if let Some(doc) = d
                .docs
                .iter_mut()
                .find(|e| str_at(e, "type") == "Spell" && norm(str_at(e, "name")) == *key)
            {
                let mut s = supports_of(doc);
                if !s.contains(&list) {
                    s.push(list.clone());
                    doc["supports"] = json!(s);
                    let id = str_at(doc, "id").to_string();
                    d.notes.push((id, why.clone()));
                }
                continue;
            }
            let lib = store
                .by_type("Spell")
                .find(|s| in_edition(&s.element, plan.edition) && norm(&s.element.name) == *key);
            if let Some(s) = lib.filter(|s| !s.element.supports.contains(&list)) {
                let id = s.element.id.clone();
                push_append(d, &id, &list);
                d.notes.push((id, why.clone()));
            }
        }
    }
}

fn apply_list(
    plan: &ImportPlan,
    store: &ContentStore,
    d: &mut Derived,
    list: &str,
    section_id: &str,
    spells: &BTreeSet<String>,
    printed_only: bool,
) {
    let mut added_own = Vec::new();
    let mut appended = Vec::new();
    for key in spells {
        if let Some(doc) = d
            .docs
            .iter_mut()
            .find(|e| str_at(e, "type") == "Spell" && norm(str_at(e, "name")) == *key)
        {
            let mut s = supports_of(doc);
            if !s.iter().any(|t| t == list) {
                s.push(list.to_string());
                doc["supports"] = json!(s);
                added_own.push(str_at(doc, "name").to_string());
            }
            continue;
        }
        let lib = store
            .by_type("Spell")
            .find(|s| in_edition(&s.element, plan.edition) && norm(&s.element.name) == *key);
        if let Some(s) = lib.filter(|s| !s.element.supports.iter().any(|t| t == list)) {
            let (id, name) = (s.element.id.clone(), s.element.name.clone());
            push_append(d, &id, list);
            appended.push(name);
        }
    }
    let not_listed: Vec<String> = d
        .docs
        .iter()
        .filter(|e| str_at(e, "type") == "Spell")
        .filter(|e| supports_of(e).iter().any(|t| t == list))
        .filter(|e| !spells.contains(&norm(str_at(e, "name"))))
        .map(|e| str_at(e, "name").to_string())
        .collect();
    let mut note = |m: String| d.notes.push((section_id.to_string(), m));
    if printed_only && !(added_own.is_empty() && appended.is_empty()) {
        let all: Vec<String> = added_own.iter().chain(&appended).cloned().collect();
        note(format!("{} spell(s) joined the {list} list, as the source prints it under that heading (no class or known list of that name): {}", all.len(), all.join(", ")));
        return;
    }
    if !added_own.is_empty() {
        note(format!("{} spell(s) are in the {list} spell list section but their own Classes line omits {list}; they were added to the list (union): {}", added_own.len(), added_own.join(", ")));
    }
    if !appended.is_empty() {
        note(format!("{} existing spell(s) of the {list} spell list section were added to the {list} list: {}", appended.len(), appended.join(", ")));
    }
    if !not_listed.is_empty() {
        note(format!("{} spell(s) name {list} in their Classes line but are missing from its spell list section; they stay on the list (union): {}", not_listed.len(), not_listed.join(", ")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_class_mark_glued_to_spell_names_joins_its_list() {
        let names: BTreeSet<String> = ["moth swarm", "dew lance", "light", "gust"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let text = "You learn the gustXX, moth swarmLa, and dew lanceLa cantrips.\n\
The dim lightLa fades. A lightThe glow.";
        let got = marked_spells(text, &names);
        let la: Vec<&str> = got["La"].iter().map(String::as_str).collect();
        assert_eq!(la, ["dew lance", "light", "moth swarm"]);
        assert_eq!(got["XX"].len(), 1);
        // A glued word of mark length is read too; it names no list, so it adds nothing.
        assert_eq!(got["The"].len(), 1);
        assert_eq!(glued_mark("madnessWz"), Some(("madness", "Wz")));
        assert_eq!(glued_mark("lightWizards"), None);
    }

    #[test]
    fn marked_spells_get_the_list_the_mark_abbreviates() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let store = borealis_content::ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .and_then(|b| b.add_pack_dir(&root.join("srd-5.1")))
            .expect("bundled packs")
            .build();
        let plan: ImportPlan = serde_json::from_value(
            json!({"edition": "2014", "elements": [], "notes": [], "groups": []}),
        )
        .expect("plan");
        let spell = |id: &str, name: &str| json!({"id": id, "name": name, "type": "Spell", "supports": [], "description": "<p>x</p>"});
        let mut d = Derived {
            docs: vec![
                json!({"id": "ID_T_CLASS_LAMPLIGHTER", "name": "Lamplighter", "type": "Class",
                    "description": "<p>Keepers.</p>"}),
                spell("ID_T_SPELL_MOTH_SWARM", "Moth Swarm"),
                spell("ID_T_SPELL_DEW_LANCE", "Dew Lance"),
                spell("ID_T_SPELL_REED_SONG", "Reed Song"),
            ],
            ..Derived::default()
        };
        let source =
            "Spells marked La are new.\nYou gain the moth swarmLa and dew lanceLa cantrips \
and learn reed song. You also know mage handLa.\nA lone reed songLam. The lightThe and gustThe.";
        marked_lists(source, &plan, &store, &mut d);
        let supports = |i: usize| supports_of(&d.docs[i]);
        assert_eq!(supports(1), ["Lamplighter"]);
        assert_eq!(supports(2), ["Lamplighter"]);
        assert!(
            supports(3).is_empty(),
            "unmarked, or a mark of one spell only"
        );
        // A library spell with the mark is appended to the list.
        assert_eq!(d.appends.len(), 1, "{:?}", d.appends);
        assert_eq!(supports_of(&d.appends[0]), ["Lamplighter"]);
        assert_eq!(d.notes.len(), 3);
    }

    #[test]
    fn a_printed_list_heading_opens_its_own_list_and_other_headings_end_one() {
        let lists = Lists {
            names: [("lamplighter".to_string(), "Lamplighter".to_string())]
                .into_iter()
                .collect(),
        };
        let names: Vec<String> = ["moth swarm", "dew lance", "reed song", "light", "gust"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let plan: ImportPlan = serde_json::from_value(json!({"edition": "2014", "groups": [], "notes": [],
            "elements": [
                {"id": "T1", "ty": "Rules Glossary Entry", "name": "Lamplighter Spells", "role": "text", "brief": ""},
                {"id": "T2", "ty": "Rules Glossary Entry", "name": "Vigil Spells", "role": "text", "brief": ""},
                {"id": "T3", "ty": "Rules Glossary Entry", "name": "1st Level Spells", "role": "text", "brief": ""}]}))
        .expect("plan");
        let text = |id: &str, name: &str, desc: &str| json!({"id": id, "name": name, "type": "Rules Glossary Entry", "description": desc});
        let docs = vec![
            text("T1", "Lamplighter Spells", "<p><strong>Lamplighter Spells</strong></p><p><strong>1st Level</strong></p><p>Moth Swarm Dew Lance</p><p><strong>Lamp Care</strong></p><p>Light Gust</p>"),
            text("T2", "Vigil Spells", "<p><strong>Vigil Spells</strong></p><p>Moth Swarm Reed Song</p><p>59</p><p>Gust</p>"),
            text("T3", "1st Level Spells", "<p><strong>1st Level Spells</strong></p><p>Light Gust Reed Song</p>"),
        ];
        let got = section_members(&plan, &docs, &lists, &names);
        let members = |l: &str| {
            got.get(l)
                .map(|(_, s)| s.iter().cloned().collect::<Vec<_>>())
        };
        // A level sub-heading keeps the class list; another heading ends it.
        assert_eq!(members("Lamplighter").unwrap(), ["dew lance", "moth swarm"]);
        // "Vigil Spells" names no class: it is a list of its own, named as printed.
        assert_eq!(
            members("Vigil Spells").unwrap(),
            ["gust", "moth swarm", "reed song"]
        );
        assert_eq!(got.len(), 2, "a level heading is no list: {got:?}");
        assert_eq!(
            printed_list("Vigil Spells").as_deref(),
            Some("Vigil Spells")
        );
        assert_eq!(printed_list("Cantrips (0 Level) Spells"), None);
        assert_eq!(printed_list("Class Spell Lists"), None);
    }

    #[test]
    fn a_list_named_with_and_without_its_suffix_is_one_list_for_a_mark() {
        let names = |v: &[&str]| Lists {
            names: v.iter().map(|n| (norm(n), n.to_string())).collect(),
        };
        let l = names(&["Lamplighter", "Lamplighter Spells", "Wizard"]);
        assert_eq!(l.abbreviated("La").as_deref(), Some("Lamplighter"));
        let only_suffixed = names(&["Lamplighter Spell List", "Wizard"]);
        assert_eq!(
            only_suffixed.abbreviated("La").as_deref(),
            Some("Lamplighter Spell List")
        );
        // Two different lists sharing the prefix stay ambiguous.
        let two = names(&["Lamplighter", "Lantern Spells"]);
        assert_eq!(two.abbreviated("La"), None);
    }

    #[test]
    fn a_spell_choice_naming_the_class_finds_the_list_spelled_with_a_suffix() {
        let mut g = crate::import::anything::plan::standalone(
            crate::import::anything::types::ImportKind::Class,
            "Lamplighter",
            None,
        );
        g.outline = Some(
            serde_json::from_value(json!({"name": "Lamplighter", "summary": "", "quote": "",
                "spellcasting": {"ability": "Wisdom", "progression": "full",
                    "list": "Lamplighter Spells"},
                "features": []}))
            .expect("outline"),
        );
        let plan: ImportPlan = ImportPlan {
            edition: crate::import::anything::types::Edition::Y2014,
            groups: vec![g],
            elements: Vec::new(),
            notes: Vec::new(),
        };
        let mut d = Derived {
            docs: vec![
                json!({"id": "ID_T_LORE", "type": "Class Feature", "name": "Lore", "rules": [
                    {"kind": "select", "type": "Spell", "supports": "Lamplighter,6"},
                    {"kind": "select", "type": "Spell", "supports": "$(spellcasting:list),0"},
                    {"kind": "select", "type": "Spell", "supports": "Wizard,1|Lamplighter,2"}]}),
                json!({"id": "ID_T_SPELL_GLOW", "type": "Spell", "name": "Glow",
                    "supports": ["Lamplighter", "Wizard"], "description": "<p>x</p>"}),
            ],
            ..Derived::default()
        };
        one_list_name(&plan, &mut d);
        let r = &d.docs[0]["rules"];
        assert_eq!(r[0]["supports"], "Lamplighter Spells,6");
        assert_eq!(r[1]["supports"], "$(spellcasting:list),0");
        assert_eq!(r[2]["supports"], "Wizard,1|Lamplighter Spells,2");
        assert_eq!(supports_of(&d.docs[1]), ["Lamplighter Spells", "Wizard"]);
        assert_eq!(d.notes.len(), 3);
    }

    /// Offline: `BOREALIS_MARKS_PROBE=<draft.json>` prints what [`marked_lists`] adds to the
    /// stored draft's derived elements.
    #[test]
    #[ignore = "needs a local draft.json"]
    fn probe_marked_lists() {
        let Ok(path) = std::env::var("BOREALIS_MARKS_PROBE") else {
            return;
        };
        let draft: crate::import::anything::types::ImportDraft =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let mut b = borealis_content::ContentStoreBuilder::new();
        for pack in ["core", "srd-5.1", "srd-5.2.1"] {
            b = b.add_pack_dir(&root.join(pack)).unwrap();
        }
        let store = b.build();
        let raw: Vec<Value> = draft.elements.iter().map(|e| e.json.clone()).collect();
        let d = super::super::derived::derive_with_source(
            &draft.plan,
            &raw,
            &store,
            &draft.source_text,
        );
        for (id, m) in d
            .notes
            .iter()
            .filter(|(_, m)| m.contains("the source marks it"))
        {
            println!("MARK {id}: {m}");
        }
    }

    #[test]
    fn a_list_line_reads_the_longest_spell_names_and_skips_prose() {
        let names: Vec<String> = ["mass cure wounds", "cure wounds", "light", "ember veil"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let got = spells_in("1st Level: Mass Cure Wounds, Ember Veil", &names);
        assert_eq!(got, ["mass cure wounds", "ember veil"]);
        assert!(spells_in("The torches give off a dim light in the hall.", &names).is_empty());
        let segs = segments("<p><strong>Wizard Spells</strong></p><p>Light<br>Ember Veil</p>");
        assert_eq!(segs, ["Wizard Spells", "Light", "Ember Veil"]);
    }
}
