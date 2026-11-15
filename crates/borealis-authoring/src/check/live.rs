//! The live pipeline of the homebrew creator (spec 8.1 steps 1-5), run on every autosave of a draft:
//!
//! 1. **Form checks**: required fields of the form and its blocks, ranges, dice, closed choices (`fields.rs`), and
//!    the builder and codec problems of the form (`form::check_fields`).
//! 2. **Build** the draft's elements (the root and its children, with the ids they have or would get now).
//! 3. **Schema**: the elements as a content file against the content schema.
//! 4. **Store**: the content validator's element checks with references resolved against the loaded content plus the
//!    draft ([`borealis_content::validate::validate_overlay`]: no store is built, so it stays fast with a large
//!    library), and the rule registry's problems (`rule_kinds::check_custom`).
//! 5. **Creator lints** (`lints.rs`).
//!
//! Every problem carries a plain sentence (`messages.rs`) and the form path of its field. The engine checks (step 6)
//! and balance hints (step 7) are heavier and run apart (`deep.rs`).

use super::fields::form_findings;
use super::lints::lints;
pub use super::lints::PackElement;
use super::messages::{issue_code, issue_message, rule_problem_message, Finding};
use super::problem::{dedupe, Problem, ProblemSeverity};
use crate::form::{
    check_fields, form_path_for_issue, form_path_for_location, form_to_elements, FormContext,
    FormState, Lookup, RefInfo, APPEND_KIND, INCOMPLETE,
};
use borealis_content::file::ContentFile;
use borealis_content::issue::{Issue, IssueCode};
use borealis_content::rule_kinds::check_custom;
use borealis_content::validate::validate_overlay;
use borealis_content::{ContentStore, Element, MonsterData, Rule};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// What the live pipeline looks at.
pub struct LiveInput<'a> {
    /// The draft's form, with the ids its elements have or would get now (`form::assign_ids` on a copy).
    pub form: &'a FormState,
    /// The form context over the loaded content.
    pub cx: &'a FormContext<'a>,
    /// The loaded content.
    pub store: &'a ContentStore,
    /// The pack's content file (for the validator's locations).
    pub file: &'a str,
    /// Saved elements the draft started with and no longer has (removed children).
    pub hidden: &'a HashSet<&'a str>,
    /// The pack's elements as saved (for duplicate names).
    pub pack: &'a [PackElement],
}

/// The problems of a draft, in pipeline order, without repeats.
#[must_use]
pub fn live_problems(input: &LiveInput<'_>) -> Vec<Problem> {
    let form = input.form;
    let cx = input.cx;
    let mut out: Vec<Problem> = form_findings(form, cx)
        .into_iter()
        .map(|(path, f)| finding(&path, element_at(form, &path), &f))
        .collect();
    // a block that can't make a rule yet is told by its missing required fields
    let told: HashSet<String> = out
        .iter()
        .filter(|p| p.code == "block.required")
        .filter_map(|p| p.path.as_deref())
        .flat_map(|p| [p.split_once(".fields."), p.rsplit_once(".fields.")])
        .flatten()
        .map(|(block, _)| block.to_string())
        .collect();
    out.extend(
        check_fields(form, cx)
            .into_iter()
            .filter(|p| {
                !(p.message == INCOMPLETE && told.contains(p.path.trim_end_matches(".fields")))
            })
            .map(|p| {
                Problem::new(
                    ProblemSeverity::Error,
                    "form",
                    p.message,
                    p.path.clone(),
                    element_at(form, &p.path),
                )
            }),
    );
    if form.kind == APPEND_KIND {
        return dedupe(out);
    }
    let Ok(elements) = form_to_elements(form, cx) else {
        return dedupe(out);
    };
    let names = NameBook::new(form, &elements, cx.lookup);
    out.extend(schema_problems(form, &elements, cx, &names));
    let issues = validate_overlay(input.store, input.file, &elements, input.hidden);
    out.extend(issue_problems(&issues, form, &elements, cx));
    let pairs = pairs(form, &elements);
    for (prefix, f, e) in &pairs {
        for (i, rule) in e.rules.iter().enumerate() {
            let Rule::Custom(c) = rule else {
                continue;
            };
            for p in check_custom(c) {
                let path = form_path_for_location(f, e, &format!("rules[{i}]"), cx)
                    .map(|p| format!("{prefix}{p}"))
                    .unwrap_or_else(|| prefix.trim_end_matches('.').to_string());
                out.push(Problem::new(
                    ProblemSeverity::Warning,
                    p.code(),
                    rule_problem_message(&p),
                    path,
                    Some(&e.id),
                ));
            }
        }
    }
    out.extend(
        lints(&pairs, input.pack, input.store, cx)
            .into_iter()
            .map(|(path, id, f)| finding(&path, id.as_deref(), &f)),
    );
    dedupe(out)
}

fn finding(path: &str, element: Option<&str>, f: &Finding) -> Problem {
    Problem::new(f.severity(), f.code(), f.message(), path, element)
}

/// Each form of the draft with its path prefix (`""`, `children.0.`) and the element it writes (same order as
/// `form_to_elements`).
pub(super) fn pairs<'f>(
    form: &'f FormState,
    elements: &'f [Element],
) -> Vec<(String, &'f FormState, &'f Element)> {
    fn walk<'f>(form: &'f FormState, prefix: String, out: &mut Vec<(String, &'f FormState)>) {
        out.push((prefix.clone(), form));
        for (i, c) in form.children.iter().enumerate() {
            walk(c, format!("{prefix}children.{i}."), out);
        }
    }
    let mut forms = Vec::new();
    walk(form, String::new(), &mut forms);
    forms
        .into_iter()
        .zip(elements)
        .map(|((p, f), e)| (p, f, e))
        .collect()
}

/// The id of the element a form path belongs to (`children.1.fields.name` -> the second child's).
fn element_at<'f>(form: &'f FormState, path: &str) -> Option<&'f str> {
    let mut f = form;
    let mut rest = path;
    while let Some(after) = rest.strip_prefix("children.") {
        let (i, tail) = after.split_once('.').unwrap_or((after, ""));
        f = f.children.get(i.parse::<usize>().ok()?)?;
        rest = tail;
    }
    f.id()
}

/// Names of the elements a draft refers to: the last names its reference values hold, its own elements, then the
/// loaded content.
pub(super) struct NameBook<'a> {
    own: HashMap<String, String>,
    loaded: &'a dyn Lookup,
}

impl<'a> NameBook<'a> {
    pub(super) fn new(form: &FormState, elements: &[Element], loaded: &'a dyn Lookup) -> Self {
        let mut own = HashMap::new();
        for f in form.flatten() {
            for v in f.fields.values() {
                collect_names(v, &mut own);
            }
            for b in &f.blocks {
                for v in b.fields.values() {
                    collect_names(v, &mut own);
                }
            }
        }
        for e in elements {
            own.insert(e.id.clone(), e.name.clone());
        }
        Self { own, loaded }
    }
}

impl Lookup for NameBook<'_> {
    fn find(&self, id: &str) -> Option<RefInfo> {
        self.own
            .get(id)
            .map(|n| RefInfo {
                name: n.clone(),
                ty: String::new(),
            })
            .or_else(|| self.loaded.find(id))
    }
}

/// Every `{id, name}` pair in a form value (reference values keep the last name they were picked with).
fn collect_names(v: &Value, out: &mut HashMap<String, String>) {
    match v {
        Value::Object(m) => {
            if let (Some(Value::String(id)), Some(Value::String(name))) =
                (m.get("id"), m.get("name"))
            {
                if !name.trim().is_empty() {
                    out.entry(id.clone()).or_insert_with(|| name.clone());
                }
            }
            m.values().for_each(|x| collect_names(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| collect_names(x, out)),
        _ => {}
    }
}

/// `/elements/0/rules/2/kind` -> (0, `rules[2]`); `/elements/1/data/rarity` -> (1, `data.rarity`).
fn schema_location(pointer: &str) -> Option<(usize, Option<String>)> {
    let mut parts = pointer.trim_start_matches('/').split('/');
    if parts.next()? != "elements" {
        return None;
    }
    let index = parts.next()?.parse().ok()?;
    let Some(top) = parts.next() else {
        return Some((index, None));
    };
    let location = match (top, parts.next()) {
        ("data", Some(key)) => format!("data.{key}"),
        (list, Some(n)) if n.parse::<usize>().is_ok() => format!("{list}[{n}]"),
        (field, _) => field.to_string(),
    };
    Some((index, Some(location)))
}

/// A schema violation inside a creature's stat block whose own validator found problems (`monster_validate.rs`): it
/// tells them with a sentence at each part (`fields.armorClass`), so the schema's general one would say it twice.
fn told_by_stat_block(elements: &[Element], pointer: &str) -> bool {
    let Some((i, Some(location))) = schema_location(pointer) else {
        return false;
    };
    location.starts_with("data")
        && elements
            .get(i)
            .and_then(|e| MonsterData::from_element(e).ok())
            .is_some_and(|m| !m.validate().is_empty())
}

fn schema_problems(
    form: &FormState,
    elements: &[Element],
    cx: &FormContext<'_>,
    names: &NameBook<'_>,
) -> Vec<Problem> {
    let Ok(doc) = serde_json::to_value(ContentFile::new(elements.to_vec())) else {
        return Vec::new();
    };
    let Ok(violations) = borealis_content::schema::validate_content(&doc) else {
        return Vec::new();
    };
    violations
        .into_iter()
        .filter(|v| !told_by_stat_block(elements, &v.pointer))
        .map(|v| {
            let (el, location) = schema_location(&v.pointer)
                .and_then(|(i, l)| elements.get(i).map(|e| (Some(e), l)))
                .unwrap_or((None, None));
            let id = el.map(|e| e.id.as_str());
            let path = id
                .and_then(|id| form_path_for_issue(form, elements, id, location.as_deref(), cx))
                .unwrap_or_default();
            let issue = Issue::error(IssueCode::SchemaViolation, v.message);
            Problem::new(
                ProblemSeverity::Error,
                issue_code(issue.code),
                issue_message(&issue, el.map_or("", |e| e.ty.as_str()), names),
                path,
                id,
            )
        })
        .collect()
}

/// The content validator's issues on a draft's elements as problems: a plain sentence, the form path (a missing
/// required setter points at its field), the severity the validator gave.
#[must_use]
pub fn issue_problems(
    issues: &[Issue],
    form: &FormState,
    elements: &[Element],
    cx: &FormContext<'_>,
) -> Vec<Problem> {
    let names = NameBook::new(form, elements, cx.lookup);
    issues
        .iter()
        .map(|i| {
            let id = i.element.as_deref().unwrap_or_default();
            let ty = elements
                .iter()
                .find(|e| e.id == id)
                .map_or("", |e| e.ty.as_str());
            let setter = (i.code == IssueCode::MissingRequiredSetter && i.path.is_none())
                .then(|| quoted_setter(&i.message))
                .flatten();
            let location = setter.or_else(|| i.path.clone());
            let path = form_path_for_issue(form, elements, id, location.as_deref(), cx)
                .or_else(|| form_path_for_issue(form, elements, id, None, cx))
                .unwrap_or_default();
            Problem::new(
                ProblemSeverity::of_issue(i.severity),
                issue_code(i.code),
                issue_message(i, ty, &names),
                path,
                i.element.as_deref(),
            )
        })
        .collect()
}

/// `setter:school` from `Spell element requires setter "school"`.
fn quoted_setter(message: &str) -> Option<String> {
    let start = message.find('"')? + 1;
    let len = message[start..].find('"')?;
    Some(format!("setter:{}", &message[start..start + len]))
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
