//! A feature the class table lists once per tier ("Lore (6th level)", "Lore (7th level)") is
//! one section heading in the text ("Lore"). The bundled packs model it as ONE feature granted
//! at its first level whose rules carry each tier's `level` (Mystic Arcanum, Indomitable), so
//! the outline rows of one base name are merged into that feature.

use super::types::{OutlineFeature, OutlinePart, PickCount};

fn norm(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The name before a trailing parenthetical ("Lore (6th level)" -> "Lore"), if there is one.
fn qualified_base(name: &str) -> Option<&str> {
    let t = name.trim();
    let open = t.strip_suffix(')')?.rfind('(')?;
    let base = t[..open].trim();
    (!base.is_empty()).then_some(base)
}

fn base_key(name: &str) -> String {
    norm(qualified_base(name).unwrap_or(name))
}

fn push_unique(into: &mut Vec<OutlinePart>, from: &[OutlinePart]) {
    for p in from {
        if !into.iter().any(|q| norm(&q.name) == norm(&p.name)) {
            into.push(p.clone());
        }
    }
}

/// One feature for the rows of one base name (sorted by level, at least one qualified).
fn merged(rows: &[&OutlineFeature]) -> OutlineFeature {
    let plain = rows.iter().find(|f| qualified_base(&f.name).is_none());
    let name = plain.map_or_else(
        || {
            qualified_base(&rows[0].name)
                .unwrap_or(&rows[0].name)
                .to_string()
        },
        |f| f.name.clone(),
    );
    let tiers: Vec<String> = rows
        .iter()
        .map(|f| format!("{} at level {}", f.name, f.level))
        .collect();
    let levels: Vec<String> = rows.iter().map(|f| f.level.to_string()).collect();
    let summary = rows
        .iter()
        .map(|f| f.summary.trim())
        .find(|s| !s.is_empty())
        .unwrap_or_default();
    let mut out = OutlineFeature {
        level: rows[0].level,
        name,
        summary: format!(
            "{summary} One feature, granted at level {}; the class table lists its tiers as {}. Write every tier in this element as its own rules, each with its `level` ({}).",
            rows[0].level,
            tiers.join(", "),
            levels.join(", ")
        )
        .trim()
        .to_string(),
        // The tier rows quote the class table; the heading is found by the name.
        quote: plain.map(|f| f.quote.clone()).unwrap_or_default(),
        parts: Vec::new(),
        options: Vec::new(),
        choose: Vec::new(),
        spells: Vec::new(),
    };
    for f in rows {
        push_unique(&mut out.parts, &f.parts);
        push_unique(&mut out.options, &f.options);
        out.choose.extend(f.choose.iter().copied());
        out.spells.extend(f.spells.iter().cloned());
    }
    out.choose.sort_by_key(|c: &PickCount| c.level);
    out
}

/// The features with every base name listed at two or more levels, at least once with a
/// qualifier, merged into one feature at its first level (the first row's place).
pub(crate) fn merge_tiers(features: &[OutlineFeature]) -> Vec<OutlineFeature> {
    let qualified = |key: &str| {
        let rows: Vec<&OutlineFeature> = features
            .iter()
            .filter(|f| base_key(&f.name) == key)
            .collect();
        let tiered = rows.len() > 1
            && rows.iter().any(|f| qualified_base(&f.name).is_some())
            && rows.windows(2).any(|w| w[0].level != w[1].level);
        tiered.then_some(rows)
    };
    let mut done: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for f in features {
        let key = base_key(&f.name);
        if done.contains(&key) {
            continue;
        }
        match qualified(&key) {
            Some(mut rows) => {
                rows.sort_by_key(|r| r.level);
                done.push(key);
                out.push(merged(&rows));
            }
            None => out.push(f.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(level: u32, name: &str) -> OutlineFeature {
        OutlineFeature {
            level,
            name: name.into(),
            summary: format!("{name} summary"),
            quote: format!("{name} quote"),
            parts: Vec::new(),
            options: Vec::new(),
            choose: Vec::new(),
            spells: Vec::new(),
        }
    }

    #[test]
    fn tier_rows_of_one_heading_become_one_feature_at_its_first_level() {
        let rows = vec![
            row(2, "Ember Sense"),
            row(13, "Deep Lore (7th level)"),
            row(11, "Deep Lore (6th level)"),
            row(15, "Deep Lore (8th level)"),
            row(4, "Ability Score Improvement"),
            row(8, "Ability Score Improvement"),
            row(6, "Ward (Variant)"),
        ];
        let got = merge_tiers(&rows);
        let names: Vec<&str> = got.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Ember Sense",
                "Deep Lore",
                "Ability Score Improvement",
                "Ability Score Improvement",
                "Ward (Variant)"
            ]
        );
        let lore = &got[1];
        assert_eq!(lore.level, 11);
        assert!(lore.quote.is_empty());
        assert!(lore.summary.contains("Deep Lore (6th level) at level 11"));
        assert!(lore.summary.contains("(11, 13, 15)"));
    }

    #[test]
    fn an_unqualified_row_names_the_merged_feature() {
        let got = merge_tiers(&[
            row(2, "Rally Call"),
            row(6, "Rally Call (2/rest)"),
            row(18, "Rally Call (3/rest)"),
        ]);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].name.as_str(), got[0].level), ("Rally Call", 2));
        assert_eq!(got[0].quote, "Rally Call quote");
    }
}
