//! Choices as the engine will offer them (wave 13b). A `select` whose options are written as
//! names ("Elvish|Dwarvish") instead of element ids is pointed at those elements when every
//! name resolves to one element of the choice's type (the library or the import, the import's
//! edition first). A choice the engine would offer nothing for (options that match no element
//! of its type, in any choice kind: size, language, skill, ability, list) fails
//! [`CHOICE_EMPTY`], naming the printed options it holds: an empty choice never ships
//! silently.

use std::collections::HashMap;

use borealis_content::{ContentStore, Rule};
use borealis_engine::custom_rules::alternatives_of;
use borealis_engine::eval::{supports_cond_match, NoDynamics};
use borealis_engine::universe::{type_fits, URule, Universe};
use serde_json::Value;

use super::{failed, quoted_list, DraftElement, ElementCheck, ElementStatus, ImportDraft};

/// Code: a choice whose options match nothing of its type (the sheet offers nothing to pick).
pub(crate) const CHOICE_EMPTY: &str = "choice_empty";

/// Characters of a supports expression that is more than a plain `|` list of ids or tags
/// (`,` and, `!` not, groups, `$(...)` dynamic tokens, `[...]` ranges).
const EXPR_CHARS: [char; 7] = [',', '(', ')', '!', '&', '$', '['];

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn live(e: &DraftElement) -> bool {
    !e.json.is_null() && e.status != ElementStatus::Failed
}

/// The tokens of a plain `a|b|c` supports list (`None` for any other expression).
fn plain_list(supports: &str) -> Option<Vec<&str>> {
    if supports.contains(EXPR_CHARS) || supports.contains("||") {
        return None;
    }
    let tokens: Vec<&str> = supports
        .split('|')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    (!tokens.is_empty()).then_some(tokens)
}

/// An element of the import: (type, name, id).
type ImportEntry = (String, String, String);

fn import_entries(draft: &ImportDraft) -> Vec<ImportEntry> {
    draft
        .elements
        .iter()
        .filter(|e| live(e))
        .map(|e| {
            (
                str_at(&e.json, "type").to_string(),
                str_at(&e.json, "name").to_string(),
                e.id.clone(),
            )
        })
        .collect()
}

/// `token` already means something for a select of type `ty`: an id (library or import) or
/// a support tag some element of that type carries.
fn resolves(uni: &Universe, import: &[ImportEntry], ty: &str, token: &str) -> bool {
    if uni.get(token).is_some() || import.iter().any(|(_, _, id)| id == token) {
        return true;
    }
    uni.candidates_for(ty)
        .any(|c| c.supports.iter().any(|s| s == token))
}

/// The name a token stands for: itself, or for an id-shaped token the words after the
/// choice type's word ("ID_X_LANGUAGE_ABYSSAL" -> "abyssal").
fn token_name(token: &str, ty: &str) -> String {
    let word = format!("_{}_", ty.to_uppercase().replace(' ', "_"));
    let name = if token.starts_with("ID_") {
        token
            .rfind(&word)
            .map_or("", |i| &token[i + word.len()..])
            .replace('_', " ")
    } else {
        token.to_string()
    };
    name.trim().to_lowercase()
}

/// `name` is the element's name, or the part in brackets of a "Kind (Name)" name.
fn names(element_name: &str, name: &str) -> bool {
    let n = element_name.trim().to_lowercase();
    n == name || n.ends_with(&format!("({name})"))
}

/// The one element of type `ty` named by `token`: the import's first, then the library's of the
/// import's edition, then any library one; `None` when no or several elements match.
pub(super) fn by_name(
    uni: &Universe,
    import: &[ImportEntry],
    ty: &str,
    token: &str,
) -> Option<String> {
    let name = token_name(token, ty);
    if name.is_empty() {
        return None;
    }
    let own: Vec<&str> = import
        .iter()
        .filter(|(t, n, _)| type_fits(ty, t) && names(n, &name))
        .map(|(_, _, id)| id.as_str())
        .collect();
    if !own.is_empty() {
        return (own.len() == 1).then(|| own[0].to_string());
    }
    let found: Vec<_> = uni
        .candidates_for(ty)
        .filter(|c| c.edition.is_accepted() && names(&c.el.name, &name))
        .collect();
    let native: Vec<_> = found.iter().filter(|c| !c.edition.is_foreign()).collect();
    match (native.len(), found.len()) {
        (1, _) => Some(native[0].el.id.clone()),
        (0, 1) => Some(found[0].el.id.clone()),
        _ => None,
    }
}

/// Point the name-written options of every plain-list `select` at their elements, when every
/// unresolved option resolves by name (a partial rewrite would mix guesses with printed
/// text). Lists of one option are left alone: a single id is read as a tag by the engine.
/// Each change is recorded in the element's `normalised` notes.
pub(super) fn resolve_names(uni: &Universe, draft: &mut ImportDraft) {
    let import = import_entries(draft);
    for e in draft.elements.iter_mut().filter(|e| live(e)) {
        let Some(rules) = e.json.get_mut("rules").and_then(Value::as_array_mut) else {
            continue;
        };
        let mut notes = Vec::new();
        for r in rules.iter_mut().filter(|r| str_at(r, "kind") == "select") {
            let ty = str_at(r, "type").to_string();
            let Some(tokens) = plain_list(str_at(r, "supports")) else {
                continue;
            };
            if tokens.len() < 2 {
                continue;
            }
            let mut out = Vec::new();
            let mut changed = Vec::new();
            for t in &tokens {
                if resolves(uni, &import, &ty, t) {
                    out.push((*t).to_string());
                } else if let Some(id) = by_name(uni, &import, &ty, t) {
                    changed.push(format!("{t} -> {id}"));
                    out.push(id);
                } else {
                    changed.clear();
                    break;
                }
            }
            if changed.is_empty() || out.len() != tokens.len() {
                continue;
            }
            r["supports"] = Value::String(out.join("|"));
            notes.push(format!(
                "choice \"{}\": options written as names point at their elements ({})",
                str_at(r, "name"),
                changed.join(", ")
            ));
        }
        for n in notes {
            if !e.normalised.contains(&n) {
                e.normalised.push(n);
            }
        }
    }
}

/// The options of rule `r` as written (a list kind's `options`, a select's `supports`).
fn printed_options(r: &Value) -> Vec<String> {
    if let Some(a) = r.get("options").and_then(Value::as_array) {
        return a
            .iter()
            .map(|o| o.as_str().map_or_else(|| o.to_string(), str::to_string))
            .collect();
    }
    str_at(r, "supports")
        .split(['|', ','])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// [`CHOICE_EMPTY`] per element id: every choice of the live elements, loaded into the
/// universe of `store` (the library with the derived import), that offers no element of its
/// type. Choices with dynamic options (`$(...)`), alternatives or a list of their own are
/// skipped: their options are not known before play. `docs` are the derived documents by id,
/// whose rules hold the options as written.
pub(super) fn empty_choices(
    store: &ContentStore,
    draft: &ImportDraft,
    docs: &HashMap<&str, &Value>,
) -> HashMap<String, Vec<ElementCheck>> {
    let uni = Universe::new(store, draft.edition.ruleset());
    let mut out: HashMap<String, Vec<ElementCheck>> = HashMap::new();
    for e in draft.elements.iter().filter(|e| live(e)) {
        let Some(u) = uni.get(&e.id) else {
            continue;
        };
        let raw: Vec<&Value> = docs
            .get(e.id.as_str())
            .and_then(|d| d.get("rules"))
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .unwrap_or_default();
        for (i, rule) in u.el.rules.iter().enumerate() {
            let (Rule::Select(s), Some(URule::Select { supports, .. })) = (rule, u.rules.get(i))
            else {
                continue;
            };
            if !s.items.is_empty()
                || s.supports.as_deref().is_some_and(|t| t.contains('$'))
                || !alternatives_of(s).is_empty()
            {
                continue;
            }
            let offered = uni
                .candidates_for(&s.ty)
                .any(|c| c.edition.is_accepted() && supports_cond_match(supports, c, &NoDynamics));
            if offered {
                continue;
            }
            let printed = raw.get(i).map(|r| printed_options(r)).unwrap_or_default();
            let why = if printed.is_empty() {
                "it lists no options. Give it the printed options".to_string()
            } else {
                format!(
                    "its options {} match no {} of this import or your rules. Point it at the printed options",
                    quoted_list(&printed),
                    s.ty
                )
            };
            out.entry(e.id.clone()).or_default().push(failed(
                CHOICE_EMPTY,
                format!("Its choice \"{}\" offers nothing to pick: {why}", s.name),
            ));
        }
    }
    out
}
