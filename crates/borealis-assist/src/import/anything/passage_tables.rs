//! Tables printed inside one element's own passage (an item's table of variant effects, a
//! feature's component list) are that element's text: a generated description that left their
//! rows out gets them back as a table, and a text-only entry planned on such a table (on a row,
//! or on the caption heading right over it) is a piece of the owner, not an entry of its own.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::super::fidelity::plain;
use super::super::types::{ImportDraft, ImportPlan, PlannedElement, Role};
use super::{content, end_matter_start, heading, html_of, share, SAME_CONTENT};

/// Roles whose own passage may print a table of theirs (a class's level table is its
/// progression, not description text).
fn owner_role(role: Role) -> bool {
    matches!(
        role,
        Role::Standalone | Role::Option | Role::Part | Role::ClassFeature | Role::SubclassFeature
    )
}

fn table_line(line: &str) -> bool {
    let t = line.trim();
    t.contains(" | ") || (t.starts_with('|') && t.len() > 1)
}

fn start(p: &PlannedElement) -> Option<usize> {
    p.excerpt.as_ref().map(|x| x.start_line.saturating_sub(1))
}

/// The 0-based lines `[from, to)` an owner's own passage spans: its heading's section (or its
/// located passage without a heading), never past the next element generated on its own (a
/// part or feature inside it carries its own tables) or the document's end matter. `None` for
/// a non-owner role, an unlocated element, or a row element (its passage is a table row).
fn owner_range(lines: &[&str], p: &PlannedElement, plan: &ImportPlan) -> Option<(usize, usize)> {
    if !owner_role(p.role) {
        return None;
    }
    let s = start(p).filter(|s| *s < lines.len())?;
    if table_line(lines[s]) {
        return None;
    }
    let next = plan
        .elements
        .iter()
        .filter(|o| o.id != p.id && o.role != Role::Text)
        .filter_map(start)
        .filter(|l| *l > s)
        .min()
        .unwrap_or(lines.len());
    let end = match heading(lines[s]) {
        Some(h) => (s + 1..lines.len())
            .find(|&k| heading(lines[k]).is_some_and(|k| k <= h))
            .unwrap_or(lines.len()),
        None => p.excerpt.as_ref().map_or(s + 1, |x| x.end_line),
    };
    let tail = end_matter_start(lines, plan)
        .filter(|t| *t > s)
        .unwrap_or(lines.len());
    Some((s, end.min(next).min(tail).min(lines.len())))
}

/// The tables in `[from, to)`: each one's 0-based first and end (exclusive) line, a wrapped
/// cell (a lower-case line right under a row) inside it. Only tables of a header and a row or
/// more.
fn tables(lines: &[&str], from: usize, to: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut k = from;
    while k < to {
        if !table_line(lines[k]) {
            k += 1;
            continue;
        }
        let first = k;
        k += 1;
        while k < to
            && (table_line(lines[k]) || lines[k].trim_start().starts_with(char::is_lowercase))
        {
            k += 1;
        }
        if (first..k).filter(|&i| table_line(lines[i])).count() >= 2 {
            out.push((first, k));
        }
    }
    out
}

/// A table's rows (header left out) whose words `have` (a description's content words) lacks.
fn missing_rows(lines: &[&str], (first, end): (usize, usize), have: &HashSet<String>) -> usize {
    (first + 1..end)
        .filter(|&i| table_line(lines[i]))
        .filter(|&i| share(&content(lines[i]), have) < SAME_CONTENT)
        .count()
}

/// Puts back the rows of each generated owner's own tables its description left out (the
/// whole table, header first, after the description). Returns the ids changed.
pub(crate) fn add_owner_tables(draft: &mut ImportDraft) -> Vec<String> {
    let lines: Vec<&str> = draft.source_text.lines().collect();
    let mut changed = Vec::new();
    for i in 0..draft.elements.len() {
        let e = &draft.elements[i];
        if e.json.is_null() || e.revisions > 0 || !owner_role(e.role) {
            continue;
        }
        let Some(planned) = draft.plan.elements.iter().find(|p| p.id == e.id) else {
            continue;
        };
        let Some((s, end)) = owner_range(&lines, planned, &draft.plan) else {
            continue;
        };
        let mut desc = e
            .json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let mut added = 0;
        for t in tables(&lines, s + 1, end) {
            let have = content(&plain(&desc));
            let missing = missing_rows(&lines, t, &have);
            if missing == 0 {
                continue;
            }
            let rows: Vec<String> = lines[t.0..t.1].iter().map(|l| l.to_string()).collect();
            desc.push_str(&html_of(&rows));
            added += missing;
        }
        if added == 0 {
            continue;
        }
        let e = &mut draft.elements[i];
        e.json["description"] = json!(desc);
        e.normalised.push(format!(
            "description: its printed table added ({added} row(s) the generated text left out)"
        ));
        changed.push(e.id.clone());
    }
    changed
}

/// Text-only entries planned on a table inside another element's own passage (on one of its
/// rows, or on the caption heading right over it with only table lines under it): pieces of
/// that owner, whose description carries the table (see [`add_owner_tables`]). Returns each
/// entry's id and a plan note, for the planner to fold.
pub(crate) fn owner_table_entries(plan: &ImportPlan, source: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let owned: Vec<(&PlannedElement, (usize, usize))> = plan
        .elements
        .iter()
        .filter_map(|p| {
            let (s, end) = owner_range(&lines, p, plan)?;
            Some(tables(&lines, s + 1, end).into_iter().map(move |t| (p, t)))
        })
        .flatten()
        .collect();
    let caption_over = |at: usize, (first, _): (usize, usize)| {
        heading(lines[at]).is_some()
            && at < first
            && (at + 1..first).all(|k| lines[k].trim().is_empty())
    };
    plan.elements
        .iter()
        .filter(|t| t.role == Role::Text)
        .filter_map(|t| {
            let at = start(t)?;
            let (owner, _) = owned
                .iter()
                .find(|(_, tb)| (at >= tb.0 && at < tb.1) || caption_over(at, *tb))?;
            Some((
                t.id.clone(),
                format!(
                    "{}: a table or row of {}, kept in its text (not an entry of its own)",
                    t.name, owner.name
                ),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::anything::fidelity::tests::{draft, planned};

    const WAND: &str = "### Wand of Weather\n\
Wondrous item, rare\n\
\n\
This wand of driftwood calls the weather of a far coast. Roll on the table each dawn.\n\
\n\
#### Weather Effects\n\
\n\
d4 | Effect\n\
1 | **Fog.** A bank of fog rolls in.\n\
2 | **Hail.** Hail falls for a\n\
minute.\n\
3 | **Calm.** Nothing stirs.\n\
\n\
### Lamp of Ash\n\
Wondrous item, common\n\
\n\
This lamp burns ash instead of oil and never smokes.\n";

    fn wand_plan() -> Vec<PlannedElement> {
        let mut wand = planned(
            "w",
            "Wand of Weather",
            "Magic Item",
            Role::Standalone,
            Some(1),
        );
        wand.excerpt.as_mut().unwrap().end_line = 12;
        let lamp = planned("l", "Lamp of Ash", "Magic Item", Role::Standalone, Some(14));
        vec![
            wand,
            lamp,
            planned(
                "t1",
                "Weather Effects",
                "Rules Glossary Entry",
                Role::Text,
                Some(6),
            ),
            planned("t2", "Hail", "Rules Glossary Entry", Role::Text, Some(10)),
        ]
    }

    #[test]
    fn an_items_own_table_left_out_of_its_description_is_put_back() {
        let mut d = draft(
            WAND,
            wand_plan(),
            &[
                "<p>This wand of driftwood calls the weather of a far coast. Roll on the table each dawn.</p>",
                "<p>This lamp burns ash instead of oil and never smokes.</p>",
                "<p>d4 Effect</p>",
                "<p>Hail falls.</p>",
            ],
        );
        assert_eq!(add_owner_tables(&mut d), vec!["w".to_string()]);
        let desc = d.elements[0].json["description"].as_str().unwrap();
        assert!(desc.contains("<table>"), "{desc}");
        assert!(
            desc.contains("<td><strong>Hail.</strong> Hail falls for a minute.</td>"),
            "{desc}"
        );
        // A description that has the rows is left alone.
        assert!(add_owner_tables(&mut d).is_empty());
    }

    #[test]
    fn entries_planned_on_an_owners_table_are_its_pieces() {
        let d = draft(WAND, wand_plan(), &[]);
        let ids: Vec<String> = owner_table_entries(&d.plan, WAND)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids, ["t1", "t2"]);
        // A table in no element's own passage belongs to no owner.
        let mut lone = wand_plan();
        lone.retain(|p| p.id != "w");
        assert!(owner_table_entries(
            &ImportPlan {
                elements: lone,
                ..d.plan.clone()
            },
            WAND
        )
        .is_empty());
    }
}
