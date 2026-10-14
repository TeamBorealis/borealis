//! Printed sections the outline and detection did not keep apart. A rules group named after
//! several headed sections ("Charms" for the printed "Minor Charms", "Major Charms" and "Woven
//! Charms") becomes one text-only group per printed section, so each keeps its own heading and
//! passage. A class feature whose heading is followed by spellcasting headings of its own
//! level ("Cantrips", "Preparing Spells": a section the PDF printed flat) is the class's
//! spellcasting section, with those headings as its parts.

use super::super::caster::is_caster_part;
use super::super::types::{ImportKind, Outline, OutlinePart, PlanGroup};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::anything::plan::standalone;

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
