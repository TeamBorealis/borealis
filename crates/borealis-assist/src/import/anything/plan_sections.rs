//! Printed sections the outline and detection did not keep apart. A rules group named after
//! several headed sections ("Charms" for the printed "Minor Charms", "Major Charms" and "Woven
//! Charms") becomes one text-only group per printed section, so each keeps its own heading and
//! passage. A class feature whose heading is followed by spellcasting headings of its own
//! level ("Cantrips", "Preparing Spells": a section the PDF printed flat) is the class's
//! spellcasting section, with those headings as its parts. A text-only entry planned on a
//! category heading or a row inside another planned table is folded into that table.

use super::super::caster::is_caster_part;
use super::super::passage::page_marks::{page_furniture as furniture, PageFurniture as Furniture};
use super::super::passage::{owns_sub_sections, unplanned_sections_in};
use super::super::types::{
    Edition, ImportKind, ImportPlan, Outline, OutlinePart, PlanGroup, PlannedElement, Role,
};
use super::{norm, text_group};

/// Fewest printed sections a merged name must stand for before it is split.
const MIN_SECTIONS: usize = 2;
/// Most words before the shared name in a section heading ("Minor" in "Minor Charms").
const MAX_QUALIFIER_WORDS: usize = 2;

/// Heading titles of `source` (markdown `#` lines), as printed.
fn headings(source: &str) -> Vec<(usize, String)> {
    source
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let t = l.trim_start();
            let n = t.chars().take_while(|c| *c == '#').count();
            let title = t[n..].trim();
            (n > 0 && t[n..].starts_with(' ') && !title.is_empty()).then(|| (i, title.to_string()))
        })
        .collect()
}

/// Fewest spellcasting headings that make a flat run a feature's spellcasting section.
const MIN_CASTER_HEADINGS: usize = 2;

/// A heading title naming `key` (normalised), alone or with a level after it ("Grit 2nd").
fn titled(title: &str, key: &str) -> bool {
    let t = norm(title);
    t == key
        || t.strip_prefix(&format!("{key} "))
            .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
}

/// The headings of `feature`'s level that follow it up to the next heading naming one of
/// `stop` (or a higher-level heading), when at least [`MIN_CASTER_HEADINGS`] of them are
/// spellcasting headings. Headings naming one of `skip` (planned on their own) are passed over.
fn caster_run(source: &str, feature: &str, stop: &[String], skip: &[String]) -> Vec<String> {
    let key = norm(feature);
    let lines: Vec<&str> = source.lines().collect();
    let depth = |i: usize| {
        lines[i]
            .trim_start()
            .chars()
            .take_while(|c| *c == '#')
            .count()
    };
    let hs = headings(source);
    let Some(at) = hs.iter().position(|(_, t)| titled(t, &key)) else {
        return Vec::new();
    };
    let own = depth(hs[at].0);
    let mut run = Vec::new();
    for (i, title) in &hs[at + 1..] {
        let d = depth(*i);
        if d < own || (d == own && stop.iter().any(|s| titled(title, s))) {
            break;
        }
        if d == own && !skip.iter().any(|s| titled(title, s)) {
            run.push(title.clone());
        }
    }
    if run.iter().filter(|t| is_caster_part(t)).count() < MIN_CASTER_HEADINGS {
        run.clear();
    }
    run
}

/// A casting class whose outline has no spellcasting section: the first feature of level 3
/// or lower followed by a flat run of spellcasting headings gets them as parts (see the
/// module docs). `groups` are the plan's group names and its text-only group names (a
/// heading naming one of these is passed over, any other group's heading ends the run);
/// `is_section` tells an outline feature that already is one.
pub(crate) fn flat_caster_parts(
    o: &mut Outline,
    source: &str,
    groups: (&[String], &[String]),
    is_section: impl Fn(&super::super::types::OutlineFeature) -> bool,
) {
    if o.caster().is_none() || o.features.iter().any(|f| f.level <= 3 && is_section(f)) {
        return;
    }
    let (all, text) = groups;
    let skip: Vec<String> = text.iter().map(|n| norm(n)).collect();
    let stop: Vec<String> = o
        .features
        .iter()
        .flat_map(|f| {
            std::iter::once(&f.name).chain(f.parts.iter().chain(&f.options).map(|p| &p.name))
        })
        .chain(all.iter())
        .map(|n| norm(n))
        .filter(|n| !skip.contains(n))
        .collect();
    let mut early: Vec<usize> = (0..o.features.len())
        .filter(|&i| o.features[i].level <= 3)
        .collect();
    early.sort_by_key(|&i| o.features[i].level);
    for i in early {
        let run = caster_run(source, &o.features[i].name, &stop, &skip);
        if run.is_empty() {
            continue;
        }
        let f = &mut o.features[i];
        for name in run {
            if !f.parts.iter().any(|p| norm(&p.name) == norm(&name)) {
                f.parts.push(OutlinePart {
                    name,
                    summary: String::new(),
                    quote: String::new(),
                    level: None,
                });
            }
        }
        return;
    }
}

/// Whether the heading `key` is printed with prose of its own before the next heading.
fn has_own_prose(source: &str, key: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    headings(source)
        .iter()
        .filter(|(_, t)| norm(t) == key)
        .any(|(i, _)| {
            lines[i + 1..]
                .iter()
                .map(|l| l.trim())
                .find(|l| !l.is_empty())
                .is_some_and(|l| !l.starts_with('#'))
        })
}

/// The printed sections (headings ending in the group's name, not planned themselves) a rules
/// group stands for, when there are at least [`MIN_SECTIONS`].
fn merged_sections(g: &PlanGroup, taken: &[String], source: &str) -> Vec<String> {
    let key = norm(&g.name);
    // A heading of its own with prose under it is a section, not a merge.
    if g.kind != ImportKind::Rules || has_own_prose(source, &key) {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    for (_, title) in headings(source) {
        let n = norm(&title);
        let qualifier = n.strip_suffix(&format!(" {key}")).unwrap_or("");
        if !qualifier.is_empty()
            && qualifier.split(' ').count() <= MAX_QUALIFIER_WORDS
            && !title.trim_end().ends_with('?')
            && !taken.contains(&n)
            && !out.iter().any(|o| norm(o) == n)
        {
            out.push(title);
        }
    }
    if out.len() < MIN_SECTIONS {
        out.clear();
    }
    out
}

/// Split every rules group that merges printed sections (see the module docs) into them.
/// Returns a note per split.
pub(crate) fn split_merged(groups: &mut Vec<PlanGroup>, source: &str) -> Vec<String> {
    let mut notes = Vec::new();
    let mut out: Vec<PlanGroup> = Vec::with_capacity(groups.len());
    let mut taken: Vec<String> = groups.iter().map(|g| norm(&g.name)).collect();
    for g in std::mem::take(groups) {
        let sections = merged_sections(&g, &taken, source);
        if sections.is_empty() {
            out.push(g);
            continue;
        }
        let summary = g
            .outline
            .as_ref()
            .map(|o| o.summary.clone())
            .unwrap_or_default();
        notes.push(format!(
            "{}: the source prints it as separate sections ({}), each kept as its own element",
            g.name,
            sections.join(", ")
        ));
        taken.extend(sections.iter().map(|s| norm(s)));
        out.extend(sections.iter().map(|s| text_group(s, &summary, "")));
    }
    *groups = out;
    notes
}

/// Text-only groups (rules, lore, conditions from the detection's `other` list) named after a
/// feature, part or option an outline already plans: that section is the feature's, and a
/// text entry of the same name would print it twice. Returns a plan note per dropped group.
pub(crate) fn drop_outlined_texts(groups: &mut Vec<PlanGroup>) -> Vec<String> {
    let outlined: Vec<(String, String)> = groups
        .iter()
        .filter(|g| !is_text(g))
        .filter_map(|g| g.outline.as_ref().map(|o| (g, o)))
        .flat_map(|(g, o)| {
            o.features.iter().flat_map(move |f| {
                std::iter::once(&f.name)
                    .chain(f.parts.iter().map(|p| &p.name))
                    .chain(f.options.iter().map(|p| &p.name))
                    .map(move |n| (norm(n), g.name.clone()))
            })
        })
        .collect();
    let mut notes = Vec::new();
    groups.retain(|g| {
        let owner = is_text(g)
            .then(|| outlined.iter().find(|(n, _)| *n == norm(&g.name)))
            .flatten();
        if let Some((_, of)) = owner {
            notes.push(format!(
                "{}: planned in the outline of {of}, not as a text-only entry too",
                g.name
            ));
        }
        owner.is_none()
    });
    notes
}

fn is_text(g: &PlanGroup) -> bool {
    matches!(g.kind, ImportKind::Rules | ImportKind::Condition)
}

/// Most words of a bare name line in a table's category list ("Lamp of the moth*").
const MAX_NAME_LINE_WORDS: usize = 6;

fn heading_level(line: &str) -> Option<usize> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    (n > 0 && t[n..].starts_with(' ')).then_some(n)
}

/// A table row, or a bare name line of a table's category list (no sentence, no label).
fn table_line(line: &str) -> bool {
    let t = line.trim();
    t.contains(" | ")
        || (!t.contains(':')
            && !t.ends_with(['.', '!', '?'])
            && t.split_whitespace().count() <= MAX_NAME_LINE_WORDS)
}

/// The non-blank lines after the heading at `at`, up to the next heading, page furniture (a
/// page number, a running header or footer) left out.
fn heading_body<'a>(lines: &[&'a str], at: usize, furniture: &Furniture) -> Vec<&'a str> {
    lines
        .iter()
        .enumerate()
        .skip(at + 1)
        .take_while(|(_, l)| heading_level(l).is_none())
        .filter(|(i, l)| !l.trim().is_empty() && !furniture.has(*i))
        .map(|(_, l)| *l)
        .collect()
}

/// A planned text-only entry the detection listed as a table (its brief opens with "table").
fn is_table(p: &PlannedElement) -> bool {
    norm(&p.brief).split(' ').next() == Some("table")
}

/// 0-based first line and end (exclusive) of an element's located passage.
fn span(e: &PlannedElement) -> Option<(usize, usize)> {
    let x = e.excerpt.as_ref()?;
    let s = x.start_line.saturating_sub(1);
    Some((s, x.end_line.max(s + 1)))
}

/// The planned element a text-only entry `t` is only a piece of: the element whose passage
/// holds the table row `t` was located on; the element starting at `t`'s heading that the
/// heading names (not `t`); or the table `t`'s heading is a category of (a sub-heading of a
/// planned text-only table with no prose before its first sub-heading, over table lines only
/// (page furniture left out), when that entry is a table or `t`'s brief names it).
fn folded_into<'a>(
    t: &PlannedElement,
    all: &'a [PlannedElement],
    lines: &[&str],
    furniture: &Furniture,
) -> Option<&'a PlannedElement> {
    let (s, _) = span(t)?;
    let line = lines.get(s)?;
    let others = || all.iter().filter(|p| p.id != t.id);
    let row_text = |p: &PlannedElement| {
        p.role == Role::Text && span(p).is_some_and(|(ps, _)| lines[ps].contains(" | "))
    };
    if line.contains(" | ") {
        return others()
            .find(|p| !row_text(p) && span(p).is_some_and(|(ps, pe)| ps <= s && s < pe));
    }
    let level = heading_level(line)?;
    let title = norm(line.trim_start().trim_start_matches('#'));
    if let Some(p) = others().find(|p| {
        span(p).is_some_and(|(ps, _)| ps == s) && norm(&p.name) == title && norm(&t.name) != title
    }) {
        return Some(p);
    }
    let enclosing = (0..s)
        .rev()
        .find(|&k| heading_level(lines[k]).is_some_and(|h| h < level))?;
    let body = heading_body(lines, s, furniture);
    let brief = norm(&t.brief);
    others().find(|p| {
        p.role == Role::Text
            && span(p).is_some_and(|(ps, pe)| ps == enclosing && s < pe)
            && (brief.contains(&norm(&p.name)) || is_table(p))
            && !body.is_empty()
            && body.iter().all(|l| table_line(l))
            && heading_body(lines, enclosing, furniture)
                .iter()
                .all(|l| table_line(l))
    })
}

/// Text-only entries that are only a category heading or a row of another planned table (see
/// [`folded_into`]): their text is that table's. Returns each one's id and a plan note.
pub(crate) fn fold_table_headings(
    elements: &[PlannedElement],
    source: &str,
) -> Vec<(String, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let furniture = furniture(source);
    let folds: Vec<(String, String)> = elements
        .iter()
        .filter(|t| t.role == Role::Text)
        .filter_map(|t| {
            let p = folded_into(t, elements, &lines, &furniture)?;
            Some((
                t.id.clone(),
                format!(
                    "{}: a category heading or row of {}, kept in its text (not an entry of its own)",
                    t.name, p.name
                ),
            ))
        })
        .collect();
    let split = split_tables(elements, &folds, &lines);
    folds
        .into_iter()
        .filter(|(id, _)| !split.contains(id))
        .collect()
}

/// The table (0-based `[start, end)` run of `|` lines) the text entry `t` was located on a
/// row of.
fn row_table(t: &PlannedElement, lines: &[&str]) -> Option<(usize, usize)> {
    let (s, _) = span(t)?;
    let row = |i: usize| lines.get(i).is_some_and(|l| l.contains(" | "));
    if !row(s) {
        return None;
    }
    let start = (0..s).rev().take_while(|&i| row(i)).last().unwrap_or(s);
    let end = (s..lines.len()).take_while(|&i| row(i)).count() + s;
    Some((start, end))
}

/// Folded row entries of a table whose other row entries stay entries of their own: one table
/// gets one outcome, so these are kept as entries too. Returns their ids.
fn split_tables(
    elements: &[PlannedElement],
    folds: &[(String, String)],
    lines: &[&str],
) -> Vec<String> {
    let folded = |id: &str| folds.iter().any(|(f, _)| f == id);
    let rows: Vec<(&PlannedElement, (usize, usize))> = elements
        .iter()
        .filter(|t| t.role == Role::Text)
        .filter_map(|t| Some((t, row_table(t, lines)?)))
        .collect();
    rows.iter()
        .filter(|(t, table)| {
            folded(&t.id)
                && rows
                    .iter()
                    .any(|(o, other)| other == table && !folded(&o.id))
        })
        .map(|(t, _)| t.id.clone())
        .collect()
}

/// Text-only entries planned on a prose section a class or subclass root prints as its own
/// (a lore heading in its intro, or a sub-heading of its section that no feature claims; see
/// `passage::unplanned_sections_in`): the root keeps that text, so the section is no entry of its
/// own. Conditions stay entries. Returns each folded entry's id and a plan note.
pub(crate) fn fold_nested_lore(
    elements: &[PlannedElement],
    edition: Edition,
    source: &str,
) -> Vec<(String, String)> {
    let lines: Vec<&str> = source.lines().collect();
    // What each root would print with no text-only entries planned at all.
    let bare = ImportPlan {
        edition,
        groups: Vec::new(),
        elements: elements
            .iter()
            .filter(|e| e.role != Role::Text)
            .cloned()
            .collect(),
        notes: Vec::new(),
    };
    let condition = ImportKind::Condition.element_type();
    let mut out: Vec<(String, String)> = Vec::new();
    for root in elements.iter().filter(|e| owns_sub_sections(e, elements)) {
        // A text-only entry on the root's own heading is the root's whole section again.
        let own = span(root)
            .map(|(s, _)| s)
            .filter(|s| lines.get(*s).is_some_and(|l| heading_level(l).is_some()));
        let at_root = elements.iter().filter(|t| {
            t.role == Role::Text
                && t.ty != condition
                && own.is_some()
                && span(t).map(|(s, _)| s) == own
        });
        out.extend(at_root.map(|t| {
            (
                t.id.clone(),
                format!(
                    "{}: the section of {}, kept in its text (not an entry of its own)",
                    t.name, root.name
                ),
            )
        }));
        for sub in unplanned_sections_in(&lines, root, &bare) {
            let found: Vec<&PlannedElement> = elements
                .iter()
                .filter(|t| {
                    t.role == Role::Text
                        && t.ty != condition
                        && span(t).is_some_and(|(s, _)| s == sub.at)
                        && !out.iter().any(|(id, _)| *id == t.id)
                })
                .collect();
            out.extend(found.into_iter().map(|t| {
                (
                    t.id.clone(),
                    format!(
                        "{}: a section printed inside {}, kept in its text (not an entry of its own)",
                        t.name, root.name
                    ),
                )
            }));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::anything::plan::standalone;
    use crate::import::anything::types::Role;

    const SOURCE: &str = "# Charm Lore\n\n## Removing Old Charms\n\nScrub them off.\n\n\
## Charms\n\n### Minor Charms\n\nSmall charms.\n\n### Major Charms\n\nBig charms.\n\n\
Charms\nA running header line.\n\n## Woven Charms\n\nCloth charms.\n";

    #[test]
    fn a_rules_group_named_after_several_printed_sections_is_split() {
        let mut groups = vec![
            text_group("Removing Old Charms", "rules", ""),
            text_group("Charms", "rules", "Charm lists"),
        ];
        let notes = split_merged(&mut groups, SOURCE);
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Removing Old Charms",
                "Minor Charms",
                "Major Charms",
                "Woven Charms"
            ]
        );
        assert_eq!(notes.len(), 1);
        assert!(groups.iter().all(|g| g.kind == ImportKind::Rules));
    }

    #[test]
    fn a_name_with_prose_of_its_own_or_question_headings_is_not_split() {
        let source = SOURCE.replace("## Charms\n\n", "## Charms\n\nCharms are small spells.\n\n");
        let mut groups = vec![text_group("Charms", "rules", "")];
        assert!(split_merged(&mut groups, &source).is_empty());
        assert_eq!(groups.len(), 1);
        let faq = "## Lamps\n\n## Are lamps hot? Do they burn lamps\n\nYes.\n\n## Why so many lamps\n\nWe like lamps.\n";
        let mut g = vec![text_group("Lamps", "rules", "")];
        assert!(
            split_merged(&mut g, faq).is_empty(),
            "long qualifiers are not sections"
        );
    }

    #[test]
    fn category_headings_and_rows_of_a_planned_table_are_folded_into_it() {
        use crate::import::anything::fidelity::tests::planned;
        let src = "### Lamps by Rarity\n\n#### Common Item\n\nTin lamp\nReed lamp\n\n\
#### Rare Item\n\nGlass lamp\n7!\n\n### Kinds of Lamp\n\nd4 | Kind\n1 | Tin\n2 | Glass\n\n\
### Lamp Costs\n\n#### Oil Prices\n\nOil | Cost\nLamp oil | 1 sp\n\n#### Lamp Oils\n\nText.\n";
        let text = |id: &str, name: &str, line: usize, brief: &str| {
            let mut p = planned(id, name, "Rules Glossary Entry", Role::Text, Some(line));
            p.brief = brief.into();
            p
        };
        let mut table = text("t0", "Lamps by Rarity", 1, "table");
        table.excerpt.as_mut().unwrap().end_line = 11;
        let mut kinds = text("t3", "Kinds of Lamp", 13, "table");
        kinds.excerpt.as_mut().unwrap().end_line = 17;
        let mut costs = text("t5", "Lamp Costs", 19, "rule");
        costs.excerpt.as_mut().unwrap().end_line = 28;
        let elements = vec![
            table,
            text("t1", "Common", 3, "table. Lamps by Rarity subsection"),
            // The parent is a table: its category needs no brief naming it, and a page number
            // under the category is no prose.
            text("t2", "Rare", 8, "table. Rarity index subsection"),
            kinds,
            text("t4", "Tin", 16, "table row"),
            costs,
            // A table of its own under a rules section, its brief naming no parent: kept.
            text("t6", "Oil Prices", 21, "table"),
            // Located on another entry's heading that names that entry: folded.
            text("t7", "Oils", 26, "table"),
            text("t8", "Lamp Oils", 26, "rule"),
        ];
        let folded: Vec<String> = fold_table_headings(&elements, src)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(folded, ["t1", "t2", "t4", "t7"]);
    }

    #[test]
    fn the_rows_of_one_table_get_one_outcome() {
        use crate::import::anything::fidelity::tests::planned;
        let src =
            "### Ward Rules\n\nA ward lasts one hour.\n\nWard | Effect\nAsh Ward | Blocks smoke.\n\
Bone Ward | Blocks fear.\nSalt Ward | Blocks spirits.\nIron Ward | Blocks fey.\n";
        let rows = |rule_end: usize| {
            let mut rule = planned(
                "t0",
                "Ward Rules",
                "Rules Glossary Entry",
                Role::Text,
                Some(1),
            );
            rule.excerpt.as_mut().unwrap().end_line = rule_end;
            let mut out = vec![rule];
            for (k, name) in ["Ash Ward", "Bone Ward", "Salt Ward", "Iron Ward"]
                .iter()
                .enumerate()
            {
                let mut p = planned(
                    &format!("r{k}"),
                    name,
                    "Rules Glossary Entry",
                    Role::Text,
                    Some(6 + k),
                );
                p.brief = "table row".into();
                out.push(p);
            }
            out
        };
        let folded = |rule_end: usize| -> Vec<String> {
            fold_table_headings(&rows(rule_end), src)
                .into_iter()
                .map(|(id, _)| id)
                .collect()
        };
        // The rule's passage reaches into the table: its first two rows alone are not folded
        // while the last two stay entries.
        assert!(folded(7).is_empty(), "{:?}", folded(7));
        // The whole table inside the rule's passage: every row is its text.
        assert_eq!(folded(9), ["r0", "r1", "r2", "r3"]);
    }

    #[test]
    fn lore_printed_inside_a_class_or_subclass_is_left_to_it() {
        use crate::import::anything::fidelity::tests::planned;
        let lines = [
            "#### Lamplighter",                       // 1 class
            "A lighter of lamps.",                    // 2
            "#### Creating a Lamplighter",            // 3 lore in the flat intro: folded
            "Think about where your lamp came from.", // 4
            "#### Multiclassing",                     // 5 the class's rules block: kept
            "You need Wisdom 13.",                    // 6
            "#### Lamp Chart",                        // 7 a table only: kept
            "Level | Wicks",                          // 8
            "1st | 2",                                // 9
            "#### Wick Trim",                         // 10 class feature
            "You trim wicks.",                        // 11
            "#### Keeper of Flames",                  // 12 subclass
            "Keepers guard the flame.",               // 13
            "#### Vows of Flames",                    // 14 lore in the subclass intro: folded
            "Keepers vow to never let a flame die.",  // 15
            "#### Soot Sick",                         // 16 a condition: kept
            "A soot-sick creature coughs.",           // 17
            "#### Keeper Spells",                     // 18 subclass feature
            "You know light.",                        // 19
            "#### Old Roads",                         // 20 after the features: kept
            "Roads were older then.",                 // 21
        ];
        let src = lines.join("\n");
        let mut feature = planned(
            "f",
            "Wick Trim",
            "Class Feature",
            Role::ClassFeature,
            Some(10),
        );
        feature.owner = Some("c".into());
        let mut spells = planned(
            "s1",
            "Keeper Spells",
            "Archetype Feature",
            Role::SubclassFeature,
            Some(18),
        );
        spells.owner = Some("s".into());
        let text = |id: &str, name: &str, line: usize| {
            planned(id, name, "Rules Glossary Entry", Role::Text, Some(line))
        };
        let elements = vec![
            planned("c", "Lamplighter", "Class", Role::Class, Some(1)),
            text("t1", "Creating a Lamplighter", 3),
            text("t2", "Multiclassing", 5),
            text("t3", "Lamp Chart", 7),
            feature,
            planned(
                "s",
                "Keeper of Flames",
                "Archetype",
                Role::Subclass,
                Some(12),
            ),
            text("t4", "Vows of Flames", 14),
            planned("t5", "Soot Sick", "Condition", Role::Text, Some(16)),
            spells,
            text("t6", "Old Roads", 20),
        ];
        let folded = fold_nested_lore(&elements, Edition::Y2014, &src);
        let ids: Vec<&str> = folded.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["t1", "t4"]);
        assert!(folded[1].1.contains("printed inside Keeper of Flames"));
    }

    #[test]
    fn a_text_entry_named_after_an_outlined_feature_is_dropped() {
        let mut bg = text_group("Lamp Keeper", "background", "");
        bg.kind = ImportKind::Background;
        bg.outline.as_mut().unwrap().features = vec![serde_json::from_value(
            serde_json::json!({"name": "Wick Tie", "parts": [{"name": "Oil Debt"}]}),
        )
        .unwrap()];
        let mut groups = vec![
            bg,
            text_group("Wick tie", "table", "d8 table of the keeper's ties"),
            text_group("Oil Debt", "rules", ""),
            text_group("Old Roads", "lore", ""),
        ];
        let notes = drop_outlined_texts(&mut groups);
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Lamp Keeper", "Old Roads"]);
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("Lamp Keeper"), "{notes:?}");
    }

    #[test]
    fn one_section_or_another_kind_is_left_alone() {
        let mut groups = vec![
            text_group("Minor Charms", "rules", ""),
            standalone(ImportKind::Feat, "Charms", None),
        ];
        let before = groups.clone();
        assert!(split_merged(&mut groups, SOURCE).is_empty());
        assert_eq!(groups, before);
        let one = "## Charms\n\n### Minor Charms\n\nSmall.\n";
        let mut g = vec![text_group("Charms", "rules", "")];
        assert!(split_merged(&mut g, one).is_empty());
    }
}
