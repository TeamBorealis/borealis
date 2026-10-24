//! Languages an element's text grants ("You can speak, read, and write Common and X"): each one
//! the library lacks becomes a `Language` element of the import (the source's name, never merged
//! into a related bundled language), and the element grants every named language. A bundled
//! language the model granted in place of an unknown one (a fixed grant the sentence does not
//! name, beyond the sentence's count) is removed. Every change is reported.

use borealis_content::ContentStore;
use serde_json::{json, Value};

use super::groups::{fresh_id, id_prefix};
use super::{in_edition, norm, rules_mut, str_at, Derived};
use crate::import::anything::types::{Edition, ImportPlan};

/// Element types whose text never grants the reader a language.
const SKIP: [&str; 8] = [
    "Language",
    "Spell",
    "Rules Glossary Entry",
    "Magic Item",
    "Weapon",
    "Armor",
    "Item",
    "Weapon Property",
];
/// Words of a list entry that make it a choice or prose, not a language name.
const NOT_NAMES: [&str; 16] = [
    "one",
    "two",
    "three",
    "any",
    "another",
    "other",
    "others",
    "your",
    "their",
    "its",
    "a",
    "an",
    "the",
    "choice",
    "language",
    "languages",
];
/// Longest language name, in words.
const MAX_WORDS: usize = 3;

/// HTML to plain text, a tag ending a sentence.
fn plain(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn is_name(part: &str) -> bool {
    let words: Vec<&str> = part.split_whitespace().collect();
    !words.is_empty()
        && words.len() <= MAX_WORDS
        && words
            .iter()
            .all(|w| w.chars().next().is_some_and(char::is_uppercase))
        && !part.chars().any(|c| c.is_ascii_digit())
        && !words
            .iter()
            .any(|w| NOT_NAMES.contains(&w.to_lowercase().as_str()))
}

/// The verb phrases a language list follows ("read and write", "read, and write", "you can
/// speak"; a people's lore saying what they speak grants the reader nothing).
const LIST_VERBS: [&str; 6] = [
    "read and write ",
    "read, and write ",
    "read or write ",
    "you speak ",
    "you can speak ",
    "you can also speak ",
];
/// Lowercase words that may open a list item before the first name ("both Common and X",
/// "write in both Common and X", "write the X language").
const FILLERS: [&str; 6] = ["both", "either", "in", "also", "and", "the"];
/// Run-in heads whose text is a bare list of languages ("Languages. Common and X.").
const LIST_HEADS: [&str; 2] = ["languages", "language"];

/// A run-in head whose text lists languages: "Languages", or a short head ending in
/// "language(s)" ("Bonus Language", "Extra Languages").
fn list_head(head: &str) -> bool {
    let words = head.split_whitespace().count();
    LIST_HEADS.contains(&head)
        || (words <= MAX_WORDS && LIST_HEADS.iter().any(|h| head.ends_with(&format!(" {h}"))))
}

/// A list item without a trailing "language(s)" ("Abyssal language" -> "Abyssal").
fn strip_language_word(t: &str) -> &str {
    let lower = t.to_lowercase();
    LIST_HEADS
        .iter()
        .find_map(|h| {
            lower
                .strip_suffix(&format!(" {h}"))
                .map(|rest| t[..rest.len()].trim_end())
        })
        .unwrap_or(t)
}

/// Byte offset of the language list in `sentence` (after its verb phrase), if it has one.
fn list_start(sentence: &str) -> Option<usize> {
    let lower = sentence.to_ascii_lowercase();
    LIST_VERBS.iter().find_map(|v| {
        lower.match_indices(v).find_map(|(i, _)| {
            let word_start = lower[..i]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric());
            word_start.then_some(i + v.len())
        })
    })
}

/// `t` without its leading filler words.
fn strip_fillers(t: &str) -> &str {
    let mut t = t.trim().trim_matches(['*', '_', ' ']);
    while let Some((first, rest)) = t.split_once(char::is_whitespace) {
        if !FILLERS.contains(&first.to_lowercase().as_str()) {
            break;
        }
        t = rest.trim_start();
    }
    t
}

/// The language sentences of `text` (one clause each, as printed) and the list part of each.
fn language_lists(text: &str) -> Vec<(String, String)> {
    let text = plain(text);
    let mut out = Vec::new();
    let mut after_head = false;
    for sentence in text.split(['.', ';', '\n']) {
        let head = sentence.trim().trim_matches(['*', '_', ' ']).to_lowercase();
        let headed = std::mem::replace(&mut after_head, list_head(&head));
        // "Languages: Common and X": the head and its list in one clause.
        let colon = sentence.find(':').filter(|&c| {
            let h = sentence[..c]
                .trim()
                .trim_matches(['*', '_', ' '])
                .to_lowercase();
            list_head(&h)
        });
        let start = match (list_start(sentence), colon) {
            (Some(start), _) => start,
            (None, Some(c)) => c + 1,
            // "Languages. Common and X.": the run-in head's text is the list itself.
            (None, None) if headed => 0,
            // "X (speak, read, write)": a name with its verbs in parentheses.
            (None, None) if sentence.to_lowercase().contains(" (speak") => 0,
            (None, None) => continue,
        };
        let mut tail = &sentence[start..];
        for stop in [", but", " but ", " (", ":", " if ", " when "] {
            if let Some(i) = tail.to_ascii_lowercase().find(stop) {
                tail = &tail[..i];
            }
        }
        out.push((sentence.to_string(), tail.to_string()));
    }
    out
}

/// The languages one sentence of `text` lets the reader speak, read and write, as printed.
pub(super) fn named_languages(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (_, tail) in language_lists(text) {
        let spaced = tail.replace(" and ", ",").replace(" or ", ",");
        for part in spaced.split(',') {
            let p = strip_language_word(strip_fillers(part));
            if is_name(p) && !out.iter().any(|n: &String| norm(n) == norm(p)) {
                out.push(p.to_string());
            }
        }
    }
    out
}

/// The language `name` is printed in one of the language sentences of `text`.
fn printed(text: &str, name: &str) -> bool {
    let name = norm(name).to_lowercase();
    !name.is_empty()
        && language_lists(text).iter().any(|(sentence, _)| {
            format!(" {} ", norm(sentence).to_lowercase()).contains(&format!(" {name} "))
        })
}

/// `(id, name)` of every language the import can grant: the library's of the edition, the
/// draft's own and those already made.
fn known(store: &ContentStore, edition: Edition, d: &Derived) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = store
        .by_type("Language")
        .filter(|s| in_edition(&s.element, edition))
        .map(|s| (s.element.id.clone(), s.element.name.clone()))
        .collect();
    out.extend(
        d.docs
            .iter()
            .chain(&d.extra)
            .filter(|e| str_at(e, "type") == "Language")
            .map(|e| (str_at(e, "id").to_string(), str_at(e, "name").to_string())),
    );
    out
}

fn language_grants(doc: &Value) -> Vec<String> {
    doc.get("rules")
        .and_then(Value::as_array)
        .map(|rules| {
            rules
                .iter()
                .filter(|r| str_at(r, "kind") == "grant" && str_at(r, "type") == "Language")
                .map(|r| str_at(r, "id").to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn new_language(
    plan: &ImportPlan,
    store: &ContentStore,
    name: &str,
    source: &str,
    d: &mut Derived,
) -> String {
    let id = fresh_id(&id_prefix(plan), "Language", name, store, d);
    let supports = match plan.edition {
        Edition::Y2014 => json!(["ID_INTERNAL_SUPPORT_LANGUAGE_EXOTIC"]),
        Edition::Y2024 => json!(["Rare"]),
    };
    d.extra.push(
        json!({"id": id, "name": name, "type": "Language", "source": source,
        "supports": supports, "description": format!("<p>{name} (Language).</p>")}),
    );
    d.note(
        &id,
        format!("new language \"{name}\": the source names it and no loaded pack has it"),
    );
    id
}

/// Grant the languages each element's text names; make the unknown ones.
pub(super) fn languages(plan: &ImportPlan, store: &ContentStore, d: &mut Derived) {
    for i in 0..d.docs.len() {
        if SKIP.contains(&str_at(&d.docs[i], "type")) {
            continue;
        }
        let names = named_languages(str_at(&d.docs[i], "description"));
        if names.is_empty() {
            continue;
        }
        let id = str_at(&d.docs[i], "id").to_string();
        let source = str_at(&d.docs[i], "source").to_string();
        let mut wanted = Vec::new();
        // A named language no loaded pack has (made now, or by an earlier element or the
        // model): a bundled grant beyond the names may stand in for it.
        let mut made = false;
        for name in &names {
            let hit = known(store, plan.edition, d)
                .into_iter()
                .find(|(_, n)| norm(n) == norm(name))
                .map(|(lid, _)| lid);
            made |= hit.as_ref().is_none_or(|lid| store.get(lid).is_none());
            wanted.push(hit.unwrap_or_else(|| new_language(plan, store, name, &source, d)));
        }
        let had = language_grants(&d.docs[i]);
        let names_of = known(store, plan.edition, d);
        let text = str_at(&d.docs[i], "description").to_string();
        // A grant the sentence prints by name is never a substitute.
        let keep: Vec<String> = had
            .iter()
            .filter(|g| {
                names_of
                    .iter()
                    .any(|(lid, n)| lid == *g && printed(&text, n))
            })
            .cloned()
            .collect();
        let mut notes = Vec::new();
        let Some(rules) = rules_mut(&mut d.docs[i]) else {
            continue;
        };
        for lid in wanted.iter().filter(|w| !had.contains(w)) {
            rules.push(json!({"kind": "grant", "type": "Language", "id": lid}));
            notes.push(format!("grants the language {lid} its text names"));
        }
        let total = had.len() + wanted.iter().filter(|w| !had.contains(w)).count();
        if made && total > names.len() {
            rules.retain(|r| {
                let id = str_at(r, "id");
                let stray = str_at(r, "kind") == "grant"
                    && str_at(r, "type") == "Language"
                    && !wanted.iter().any(|w| w == id)
                    && !keep.iter().any(|k| k == id);
                if stray {
                    notes.push(format!(
                        "language grant {} removed: the text names {} instead",
                        str_at(r, "id"),
                        names.join(", ")
                    ));
                }
                !stray
            });
        }
        for n in notes {
            d.note(&id, n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_languages_become_elements_and_replace_substitutes() {
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
        let race = |id: &str, text: &str, grants: &[&str]| {
            json!({"id": id, "name": id, "type": "Race", "source": "Test",
            "description": text,
            "rules": grants.iter().map(|g| json!({"kind": "grant", "type": "Language", "id": g}))
                .collect::<Vec<_>>()})
        };
        let mut d = Derived {
            docs: vec![
                race(
                    "ID_T_RACE_A",
                    "<p><em>Languages.</em> You can speak, read, and write Common and Mossic.</p>",
                    &["ID_LANGUAGE_COMMON", "ID_LANGUAGE_PRIMORDIAL"],
                ),
                race(
                    "ID_T_RACE_B",
                    "<p>You can also speak, read, and write Mossic and Elvish.</p>",
                    &[],
                ),
                race(
                    "ID_T_RACE_C",
                    "<p>You can speak, read, and write Common and one other language.</p>",
                    &["ID_LANGUAGE_COMMON", "ID_LANGUAGE_DWARVISH"],
                ),
            ],
            ..Derived::default()
        };
        languages(&plan, &store, &mut d);
        assert_eq!(d.extra.len(), 1, "one new language, shared: {:?}", d.extra);
        let mossic = str_at(&d.extra[0], "id").to_string();
        assert_eq!(str_at(&d.extra[0], "name"), "Mossic");
        let grants = |i: usize| language_grants(&d.docs[i]);
        assert_eq!(grants(0), ["ID_LANGUAGE_COMMON", mossic.as_str()]);
        assert_eq!(grants(1), [mossic.as_str(), "ID_LANGUAGE_ELVISH"]);
        // Every name known: the model's extra grant (a chosen language) is left alone.
        assert_eq!(grants(2), ["ID_LANGUAGE_COMMON", "ID_LANGUAGE_DWARVISH"]);
    }

    #[test]
    fn language_sentences_yield_the_printed_names() {
        assert_eq!(
            named_languages("<p><strong><em>Languages.</em></strong> You can speak, read, and write Common and Mossic.</p>"),
            ["Common", "Mossic"]
        );
        assert_eq!(
            named_languages("You can also speak, read, and write Old Brindle"),
            ["Old Brindle"]
        );
        assert_eq!(
            named_languages("You can understand, read and write Common and Mossic, but you can speak only through mimicry."),
            ["Common", "Mossic"]
        );
        assert!(named_languages(
            "You can speak, read, and write Common and one extra language of your choice."
        )
        .iter()
        .eq(["Common"].iter()));
        assert!(named_languages("You speak softly. You can write poems.").is_empty());
        // Names that merely contain the verbs are no language sentence.
        assert!(named_languages(
            "Choose from Unwrite the Tide, Speakers of Moss, Spread and Writhe, Old Brindle."
        )
        .is_empty());
        assert!(named_languages("You can bread and write Mossic.").is_empty());
    }

    #[test]
    fn fillers_and_bare_language_lines_yield_every_printed_name() {
        assert_eq!(
            named_languages("You can speak, read, and write both Common and Mossic."),
            ["Common", "Mossic"]
        );
        assert_eq!(
            named_languages("You can speak, read, and write in both Common and Mossic."),
            ["Common", "Mossic"]
        );
        assert_eq!(
            named_languages("<p><em>Languages.</em> Common and Mossic.</p>"),
            ["Common", "Mossic"]
        );
        assert_eq!(
            named_languages("**Languages:** Common, Mossic, or Old Brindle"),
            ["Common", "Mossic", "Old Brindle"]
        );
        assert_eq!(named_languages("You can speak Mossic."), ["Mossic"]);
        // Lore about what a people speak, and a heading text that is prose, grant nothing.
        assert!(named_languages("Most of them speak Mossic among themselves.").is_empty());
        assert!(named_languages("Languages. Few of them learn others.").is_empty());
    }

    #[test]
    fn a_grant_the_sentence_prints_is_never_removed() {
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
        let race = |id: &str, text: &str| {
            json!({"id": id, "name": id, "type": "Race", "source": "Test", "description": text,
            "rules": [{"kind": "grant", "type": "Language", "id": "ID_LANGUAGE_COMMON"},
                {"kind": "grant", "type": "Language", "id": "ID_LANGUAGE_PRIMORDIAL"}]})
        };
        let mut d = Derived {
            docs: vec![
                race(
                    "ID_T_RACE_A",
                    "<p>You can speak, read, and write both Common and Mossic.</p>",
                ),
                race(
                    "ID_T_RACE_B",
                    "<p><em>Languages.</em> Common and Mossic.</p>",
                ),
            ],
            ..Derived::default()
        };
        languages(&plan, &store, &mut d);
        let mossic = str_at(&d.extra[0], "id").to_string();
        for i in 0..2 {
            // Common is printed: kept. Primordial stood in for the new language: removed.
            assert_eq!(
                language_grants(&d.docs[i]),
                ["ID_LANGUAGE_COMMON", mossic.as_str()],
                "{i}"
            );
        }
        // The guard alone: a printed bundled name the parser missed still keeps its grant.
        assert!(printed(
            "<p>You can speak, read, and write both Common and Mossic.</p>",
            "Common"
        ));
        assert!(!printed(
            "<p>You can speak, read, and write Mossic.</p>",
            "Common"
        ));
    }

    #[test]
    fn a_run_in_language_line_and_a_the_x_language_sentence_yield_the_name() {
        assert_eq!(
            named_languages("<p><strong>Bonus Language.</strong> Mossic (speak, read, write).</p>"),
            ["Mossic"]
        );
        assert_eq!(
            named_languages("<p>You can also speak, read, and write the Mossic language.</p>"),
            ["Mossic"]
        );
        assert_eq!(
            named_languages("You can speak, read, and write the Mossic and Old Brindle languages."),
            ["Mossic", "Old Brindle"]
        );
        assert_eq!(named_languages("Mossic (speak only)."), ["Mossic"]);
        // A choice stays a choice; a long head is prose, not a language run-in.
        assert!(
            named_languages("You can speak, read, and write the language of your choice.")
                .is_empty()
        );
        assert!(named_languages("The Story Of Their First Language. Mossic Is Old.").is_empty());
    }

    /// The parser makes the printed language and drops the bundled stand-in the model granted.
    #[test]
    fn a_run_in_language_the_library_lacks_replaces_the_models_substitute() {
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
        let mut d = Derived {
            docs: vec![
                json!({"id": "ID_T_SUBRACE_A", "name": "A", "type": "Sub Race",
                "source": "Test",
                "description": "<p><strong>Bonus Language.</strong> Mossic (speak, read, write).</p>",
                "rules": [{"kind": "grant", "type": "Language", "id": "ID_LANGUAGE_PRIMORDIAL"}]}),
            ],
            ..Derived::default()
        };
        languages(&plan, &store, &mut d);
        assert_eq!(str_at(&d.extra[0], "name"), "Mossic");
        let mossic = str_at(&d.extra[0], "id").to_string();
        assert_eq!(language_grants(&d.docs[0]), [mossic.as_str()]);
    }
}
