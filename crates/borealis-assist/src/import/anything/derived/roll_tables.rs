//! A background's printed roll tables ("d8 | Personality Trait", "d6 | Ideal", or a table of its
//! own under a heading of its own) become List selects on the background, the way the bundled
//! SRD backgrounds carry their suggested characteristics: one select per table, an item per
//! printed entry. Deterministic, whichever element the model wrote the table on: a select the
//! model already wrote on the background is kept, the same table written on one of its parts is
//! moved to the background.

use serde_json::{json, Value};

use super::{rules_mut, str_at, Derived};
use crate::import::anything::types::{ImportPlan, PlannedElement};

/// Fewest entries that make a roll table.
const MIN_ENTRIES: usize = 2;
/// Most lines from a heading to the table it captions.
const CAPTION_LINES: usize = 12;
/// Column labels of the suggested characteristics, as the SRD backgrounds name their selects.
const CHARACTERISTICS: [&str; 4] = ["Personality Trait", "Ideal", "Bond", "Flaw"];

/// A printed roll table: its line (0-based), the label of its entry column and its entries.
#[derive(Debug, Clone, PartialEq)]
struct RollTable {
    line: usize,
    label: String,
    entries: Vec<(u32, String)>,
}

/// The cells of a table line ("| a | b |" and "a | b" alike), `None` for a non-row.
fn cells(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    if !t.contains(" | ") && !(t.starts_with('|') && t.ends_with('|') && t.len() > 1) {
        return None;
    }
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    Some(t.split('|').map(|c| c.trim().to_string()).collect())
}

/// A die cell of a roll table's header ("d8", "D12").
fn die(cell: &str) -> bool {
    cell.strip_prefix(['d', 'D'])
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// A separator row ("|---|---|").
fn separator(cells: &[String]) -> bool {
    cells
        .iter()
        .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

/// The first number of a roll cell ("3", "01-10").
fn roll(cell: &str) -> Option<u32> {
    let digits: String = cell.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// An entry's text without bold marks.
fn entry_text(cell: &str) -> String {
    cell.replace("**", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The roll table whose header row is `lines[at]` ("d8 | Tie", or "d8 | Tie | d8 | Tie" printed
/// in two column pairs), with the rows that follow it (blank lines and separators skipped).
fn table_at(lines: &[&str], at: usize) -> Option<RollTable> {
    let head = cells(lines[at])?;
    if head.len() < 2 || head.len() % 2 != 0 {
        return None;
    }
    let pairs: Vec<&[String]> = head.chunks(2).collect();
    if !pairs.iter().all(|p| die(&p[0]) && !p[1].is_empty()) {
        return None;
    }
    let label = pairs[0][1].clone();
    let mut entries = Vec::new();
    for line in lines.iter().skip(at + 1) {
        if line.trim().is_empty() {
            continue;
        }
        let Some(row) = cells(line) else { break };
        if separator(&row) {
            continue;
        }
        // Another table's header, or a row of another shape, ends it.
        if row.len() != head.len() || die(&row[0]) {
            break;
        }
        for p in row.chunks(2) {
            if let Some(n) = roll(&p[0]).filter(|_| !p[1].is_empty()) {
                entries.push((n, entry_text(&p[1])));
            }
        }
    }
    entries.sort_by_key(|(n, _)| *n);
    (entries.len() >= MIN_ENTRIES).then_some(RollTable {
        line: at,
        label,
        entries,
    })
}

fn heading(line: &str) -> Option<String> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    (n > 0 && t[n..].starts_with(' ')).then(|| t[n..].trim().trim_end_matches('#').trim().into())
}

fn same(a: &str, b: &str) -> bool {
    let n = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
            .trim_end_matches('s')
            .to_string()
    };
    n(a) == n(b)
}

/// The select name of a table: a suggested characteristic by its column label; any other by the
/// heading it is printed under when that heading captions this table alone, else its label.
fn select_name(lines: &[&str], tables: &[RollTable], t: &RollTable, start: usize) -> String {
    if let Some(c) = CHARACTERISTICS.iter().find(|c| same(c, &t.label)) {
        return (*c).to_string();
    }
    let caption = (start + 1..t.line)
        .rev()
        .take(CAPTION_LINES)
        .find_map(|k| heading(lines[k]).map(|h| (k, h)));
    let Some((k, title)) = caption else {
        return t.label.clone();
    };
    let section_end = (k + 1..lines.len())
        .find(|&j| heading(lines[j]).is_some())
        .unwrap_or(lines.len());
    let in_section = tables
        .iter()
        .filter(|o| o.line > k && o.line < section_end)
        .count();
    if in_section == 1 {
        title
    } else {
        t.label.clone()
    }
}

fn list_selects(doc: &Value) -> impl Iterator<Item = &Value> {
    doc.get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|r| str_at(r, "kind") == "select" && str_at(r, "type") == "List")
}

/// The select holds the table's entries (same count, same first entry).
fn holds(select: &Value, t: &RollTable) -> bool {
    let items = select.get("items").and_then(Value::as_array);
    items.is_some_and(|i| {
        i.len() == t.entries.len()
            && i.first()
                .map(|f| entry_text(str_at(f, "text")))
                .is_some_and(|f| same(&f, &t.entries[0].1))
    })
}

fn doc_index(d: &Derived, id: &str) -> Option<usize> {
    d.docs.iter().position(|v| str_at(v, "id") == id)
}

/// Every roll table printed in a background's located passage becomes a List select on it.
pub(super) fn roll_tables(plan: &ImportPlan, source: &str, d: &mut Derived) {
    let lines: Vec<&str> = source.lines().collect();
    for bg in plan.elements.iter().filter(|e| e.ty == "Background") {
        let Some(ex) = bg.excerpt.as_ref() else {
            continue;
        };
        let start = ex.start_line.saturating_sub(1);
        let end = ex.end_line.min(lines.len());
        let tables: Vec<RollTable> = (start..end).filter_map(|k| table_at(&lines, k)).collect();
        for t in &tables {
            let name = select_name(&lines, &tables, t, start);
            add_select(plan, bg, d, t, &name);
        }
    }
}

fn add_select(plan: &ImportPlan, bg: &PlannedElement, d: &mut Derived, t: &RollTable, name: &str) {
    let Some(at) = doc_index(d, &bg.id) else {
        return;
    };
    if list_selects(&d.docs[at]).any(|s| same(str_at(s, "name"), name) || holds(s, t)) {
        return;
    }
    let select = json!({"kind": "select", "type": "List", "name": name,
        "items": t.entries.iter().map(|(n, text)| json!({"id": n, "text": text})).collect::<Vec<_>>()});
    // The same table the model wrote on one of the background's parts moves to the background.
    for part in plan
        .elements
        .iter()
        .filter(|e| e.parent.as_deref() == Some(&bg.id))
    {
        if let Some(rules) = doc_index(d, &part.id).and_then(|i| rules_mut(&mut d.docs[i])) {
            rules.retain(|r| {
                !(str_at(r, "kind") == "select" && str_at(r, "type") == "List" && holds(r, t))
            });
        }
    }
    if let Some(rules) = rules_mut(&mut d.docs[at]) {
        rules.push(select);
    }
    d.note(
        &bg.id,
        format!(
            "\"{name}\": a List select with the {} entries of the printed roll table",
            t.entries.len()
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::anything::fidelity::tests::planned;
    use crate::import::anything::types::{Edition, Role};

    const SRC: &str = "## Lamp Keeper\nYou kept the lamps.\n\n\
**Skill Proficiencies:** Insight\n\n### Wick Tie\nHow you know the wick.\n\n\
d4 | Tie | d4 | Tie\n1 | **Family.** Blood | 3 | Friend\n2 | Debt | 4 | Stranger\n\n\
### Feature: Long Night\nYou stay up.\n\n### Suggested Characteristics\nKeepers are calm.\n\n\
d4 | Personality Trait\n1 | I hum.\n2 | I count lamps.\n3 | I sleep late.\n4 | I fear dark.\n\n\
| d2 | Ideal |\n\n|---|---|\n\n| 1 | Light. (Good) |\n\n| 2 | Order. (Lawful) |\n\n\
## Next Background\n";

    fn plan() -> ImportPlan {
        let mut bg = planned("b0", "Lamp Keeper", "Background", Role::Standalone, Some(1));
        bg.excerpt.as_mut().unwrap().end_line = 27;
        let mut tie = planned("b1", "Wick Tie", "Background Feature", Role::Part, Some(6));
        tie.parent = Some("b0".into());
        ImportPlan {
            edition: Edition::Y2014,
            groups: Vec::new(),
            elements: vec![bg, tie],
            notes: Vec::new(),
        }
    }

    fn derived(bg_rules: Value, tie_rules: Value) -> Derived {
        Derived {
            docs: vec![
                json!({"id": "b0", "name": "Lamp Keeper", "type": "Background", "rules": bg_rules}),
                json!({"id": "b1", "name": "Wick Tie", "type": "Background Feature", "rules": tie_rules}),
            ],
            ..Derived::default()
        }
    }

    fn lists(doc: &Value) -> Vec<(String, usize)> {
        list_selects(doc)
            .map(|s| {
                let n = s["items"].as_array().map_or(0, Vec::len);
                (str_at(s, "name").to_string(), n)
            })
            .collect()
    }

    #[test]
    fn a_backgrounds_roll_tables_become_list_selects_on_it() {
        let mut d = derived(json!([]), json!([]));
        roll_tables(&plan(), SRC, &mut d);
        assert_eq!(
            lists(&d.docs[0]),
            [
                ("Wick Tie".to_string(), 4),
                ("Personality Trait".to_string(), 4),
                ("Ideal".to_string(), 2)
            ]
        );
        let tie = list_selects(&d.docs[0]).next().unwrap();
        assert_eq!(tie["items"][0], json!({"id": 1, "text": "Family. Blood"}));
        assert_eq!(tie["items"][2], json!({"id": 3, "text": "Friend"}));
        assert_eq!(d.notes.len(), 3);
    }

    #[test]
    fn a_select_the_model_wrote_is_kept_or_moved_to_the_background() {
        let trait_select = json!({"kind": "select", "type": "List", "name": "Personality Trait",
            "items": [{"id": 1, "text": "I hum."}]});
        let tie_select = json!({"kind": "select", "type": "List", "name": "Ties",
            "items": [{"id": 1, "text": "Family. Blood"}, {"id": 2, "text": "Debt"},
                      {"id": 3, "text": "Friend"}, {"id": 4, "text": "Stranger"}]});
        let mut d = derived(json!([trait_select]), json!([tie_select]));
        roll_tables(&plan(), SRC, &mut d);
        let names: Vec<String> = lists(&d.docs[0]).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["Personality Trait", "Wick Tie", "Ideal"]);
        assert!(lists(&d.docs[1]).is_empty(), "moved off the part");
    }

    #[test]
    fn a_table_without_a_die_column_is_no_roll_table() {
        let src = "## Lamp Keeper\n\nItem | Cost\nLamp | 2 gp\nOil | 1 sp\n";
        let mut p = plan();
        p.elements[0].excerpt.as_mut().unwrap().end_line = 5;
        let mut d = derived(json!([]), json!([]));
        roll_tables(&p, src, &mut d);
        assert!(lists(&d.docs[0]).is_empty());
    }
}
