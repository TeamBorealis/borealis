//! Opt-in feature-upgrade folding (`SheetOptions::fold_feature_upgrades`): a feature and the later
//! features that improve it ("Divine Smite" and "Improved Divine Smite") print as one entry, the
//! upgrade's text appended as a paragraph led by its name and level. Off by default: the sheet
//! otherwise lists them separately, as the reference app does.

use crate::model_text::{class_index, section_rank, short_summary};
use crate::model_types::{FeatureData, FeatureGroup};
use borealis_engine::CharacterSnapshot;

/// Words naming a later, stronger version of a feature ("Improved Divine Smite").
const UPGRADE_WORDS: &[&str] = &["improved ", "greater ", "superior "];

/// Words that say a feature named just before them gets stronger ("Your Aura of Protection is
/// now a 30-foot Emanation").
const GROWTH_WORDS: &[&str] = &[
    "improve",
    "improves",
    "increase",
    "increases",
    "grow",
    "grows",
    "become",
    "becomes",
    "now",
];
/// Words allowed between a feature's name and its growth word.
const GROWTH_GAP: usize = 5;
/// Shortest feature name a description is searched for (shorter ones are common words).
const MIN_HOST_NAME: usize = 4;

/// A name without a leading "Level N: " (2024 content).
fn without_level_lead(name: &str) -> &str {
    let n = name.trim();
    n.get(..6)
        .filter(|head| head.eq_ignore_ascii_case("level "))
        .and_then(|_| n[6..].split_once(':'))
        .filter(|(num, _)| !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()))
        .map_or(n, |(_, after)| after.trim())
}

/// The name a feature and its upgrades share, lower case
/// ("Level 14: Improved Blessed Strikes" -> "blessed strikes").
fn upgrade_stem(name: &str) -> String {
    let n = without_level_lead(name).to_lowercase();
    UPGRADE_WORDS
        .iter()
        .find_map(|w| n.strip_prefix(w))
        .map_or_else(|| n.clone(), |rest| rest.trim().to_string())
}

/// True when `name` carries an upgrade word ("Improved Critical", "Greater Divine Intervention").
fn is_upgrade_name(name: &str) -> bool {
    let n = without_level_lead(name).to_lowercase();
    UPGRADE_WORDS.iter().any(|w| n.starts_with(w))
}

/// Lower case, typographic apostrophes made plain, whitespace collapsed: the form names and
/// descriptions are compared in.
fn fold_form(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .replace(['\u{2019}', '\u{2018}', '\u{fffd}'], "'")
}

/// Byte offset of the first place `probe` (in [`fold_form`]) says the feature `host` (in
/// [`fold_form`]) improves: "your/the <host> [feature] ... <growth word>" within one clause, or
/// "improves/enhances your/the <host>".
fn growth_mention(probe: &str, host: &str) -> Option<usize> {
    if host.chars().count() < MIN_HOST_NAME {
        return None;
    }
    let is_word = |c: char| c.is_alphanumeric();
    for (pos, _) in probe.match_indices(host) {
        let end = pos + host.len();
        let before = &probe[..pos];
        let bounded = !before.chars().next_back().is_some_and(is_word)
            && !probe[end..].chars().next().is_some_and(is_word);
        if !bounded {
            continue;
        }
        let improves = [
            "improves your ",
            "improves the ",
            "enhances your ",
            "enhances the ",
        ]
        .iter()
        .any(|p| before.ends_with(p));
        if improves {
            return Some(pos);
        }
        if !(before.ends_with("your ") || before.ends_with("the ")) {
            continue;
        }
        // up to the clause end: "While your Rage is active, you become ..." is a condition
        let clause = probe[end..]
            .split(['.', '!', '?', ';', ',', ':'])
            .next()
            .unwrap_or("");
        let grows = clause
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !is_word(c)))
            .filter(|w| !matches!(*w, "feature" | "features"))
            .take(GROWTH_GAP + 1)
            .any(|w| GROWTH_WORDS.contains(&w));
        if grows {
            return Some(pos);
        }
    }
    None
}

/// One sheet entry while upgrades are folded: its class section, and its description in
/// [`fold_form`] (what an upgrade says about the feature it improves).
struct FoldEntry {
    scope: (u8, usize),
    probe: String,
    data: FeatureData,
}

/// The entry an upgrade is folded into, among the earlier entries of its class section.
///
/// - Name: "Improved X", "Greater X" or "Superior X" goes to X ("Superior Critical" ->
///   "Improved Critical"), or to a feature whose name ends with X when the upgrade's description
///   names that feature or, for a "Group: Option" name, the option ("Superior Inspiration" ->
///   "Bardic Inspiration").
/// - Text (class and subclass features only): a feature whose description says an earlier one
///   improves ("Aura Expansion": "Your Aura of Protection is now ...") goes to the one it names
///   first.
fn upgrade_host(out: &[FoldEntry], up: &FoldEntry) -> Option<usize> {
    let peers = || out.iter().enumerate().filter(|(_, h)| h.scope == up.scope);
    if is_upgrade_name(&up.data.name) {
        let stem = upgrade_stem(&up.data.name);
        if let Some((i, _)) = peers().find(|(_, h)| upgrade_stem(&h.data.name) == stem) {
            return Some(i);
        }
        let suffix = format!(" {stem}");
        let named = peers().find(|(_, h)| {
            let host = fold_form(without_level_lead(&h.data.name));
            // the text may name an option by itself ("Option" for "Group: Option")
            let option = host.rsplit_once(": ").map_or(host.as_str(), |(_, o)| o);
            host.ends_with(&suffix) && (up.probe.contains(&host) || up.probe.contains(option))
        });
        if let Some((i, _)) = named {
            return Some(i);
        }
    }
    if up.scope.0 != section_rank(FeatureGroup::Class) {
        return None;
    }
    peers()
        .filter_map(|(i, h)| {
            growth_mention(&up.probe, &fold_form(without_level_lead(&h.data.name)))
                .map(|pos| (pos, i))
        })
        .min()
        .map(|(_, i)| i)
}

/// Appends an upgrade's text to the feature it improves: a paragraph led by the upgrade's name
/// and level, its tables moved to the lines they now occupy. Nothing of either is dropped.
fn fold_into(host: &mut FeatureData, up: FeatureData) {
    let offset = if host.text.is_empty() {
        0
    } else {
        host.text.split('\n').count()
    };
    let name = without_level_lead(&up.name);
    let lead = if up.level > 0 {
        format!("{name} (level {})", up.level)
    } else {
        name.to_string()
    };
    // a table on the first line keeps its rows intact: the lead goes on a line of its own
    let own_line = up.tables.iter().any(|t| t.lines.start == 0) || up.text.is_empty();
    let block = if own_line {
        if up.text.is_empty() {
            format!("{lead}.")
        } else {
            format!("{lead}.\n{}", up.text)
        }
    } else {
        format!("{lead}. {}", up.text)
    };
    let shift = offset + usize::from(own_line && !up.text.is_empty());
    host.tables.extend(up.tables.into_iter().map(|mut t| {
        t.lines = t.lines.start + shift..t.lines.end + shift;
        t
    }));
    if host.text.is_empty() {
        host.summary = short_summary(&block);
        host.text = block;
    } else {
        host.text.push('\n');
        host.text.push_str(&block);
    }
    for s in up.sources {
        if !host.sources.contains(&s) {
            host.sources.push(s);
        }
    }
    host.prerequisites_unmet |= up.prerequisites_unmet;
}

/// The sheet's features (in sheet order) with each upgrade folded into the feature it improves
/// ([`upgrade_host`]), so a feature that grows reads as one entry.
pub(crate) fn fold_feature_upgrades(
    features: Vec<FeatureData>,
    snap: &CharacterSnapshot,
) -> Vec<FeatureData> {
    let mut out: Vec<FoldEntry> = Vec::new();
    for data in features {
        let probe = snap
            .features
            .iter()
            .find(|f| f.id == data.id)
            .map(|f| {
                fold_form(&format!(
                    "{} {}",
                    crate::card_model::plain_paragraphs(&f.description),
                    f.sheet_summary.as_deref().unwrap_or_default()
                ))
            })
            .unwrap_or_default();
        let entry = FoldEntry {
            scope: (
                section_rank(data.group),
                class_index(&snap.classes, data.sources.first().map(String::as_str)),
            ),
            probe,
            data,
        };
        match upgrade_host(&out, &entry) {
            Some(i) => fold_into(&mut out[i].data, entry.data),
            None => out.push(entry),
        }
    }
    out.into_iter().map(|f| f.data).collect()
}

#[cfg(test)]
mod tests {
    use super::fold_feature_upgrades;
    use crate::model_fixtures::fighter_wizard_2014;
    use crate::model_types::{FeatureGroup, SpellFactsMap};
    use borealis_engine::snapshot::FeatureDetail;

    /// The fighter/wizard fixture with `extra` features (copies of its Second Wind with the given
    /// id, name, level, type and description), as sheet data before and after folding.
    fn folded(
        extra: &[(&str, &str, u32, &str, &str)],
    ) -> (
        Vec<crate::model_types::FeatureData>,
        Vec<crate::model_types::FeatureData>,
    ) {
        let (c, mut snap) = fighter_wizard_2014();
        let base: FeatureDetail = snap
            .features
            .iter()
            .find(|f| f.name == "Second Wind")
            .expect("fighter feature")
            .clone();
        for (id, name, level, ty, html) in extra {
            snap.features.push(FeatureDetail {
                id: (*id).into(),
                name: (*name).into(),
                ty: (*ty).into(),
                level: *level,
                description: (*html).into(),
                sheet_summary: None,
                ..base.clone()
            });
        }
        let plain = crate::sheet_data(&c, &snap, &SpellFactsMap::new()).features;
        let fold = fold_feature_upgrades(plain.clone(), &snap);
        (plain, fold)
    }

    fn text<'a>(fs: &'a [crate::model_types::FeatureData], name: &str) -> &'a str {
        &fs.iter().find(|f| f.name == name).unwrap().text
    }

    const CLASS: &str = "Class Feature";

    #[test]
    fn named_upgrades_fold_and_keep_their_text() {
        let (plain, fold) = folded(&[
            (
                "ID_T_IMPROVED",
                "Improved Second Wind",
                10,
                CLASS,
                "<p>You regain 2d10 hit points instead.</p>",
            ),
            (
                "ID_T_GREATER",
                "Level 18: Greater Second Wind",
                18,
                CLASS,
                "<p>You regain 3d10 hit points instead.</p>",
            ),
            // another section's upgrade is its own entry
            (
                "ID_T_TRAIT",
                "Superior Second Wind",
                1,
                "Racial Trait",
                "<p>A species version.</p>",
            ),
        ]);
        let base = text(&plain, "Second Wind");
        let host = text(&fold, "Second Wind");
        assert!(host.starts_with(base), "{host}");
        assert!(
            host.contains("\nImproved Second Wind (level 10). You regain 2d10 hit points instead."),
            "{host}"
        );
        assert!(
            host.ends_with("\nGreater Second Wind (level 18). You regain 3d10 hit points instead."),
            "{host}"
        );
        let names: Vec<_> = fold.iter().map(|f| f.name.as_str()).collect();
        assert!(!names.contains(&"Improved Second Wind"), "{names:?}");
        assert!(fold
            .iter()
            .any(|f| f.group == FeatureGroup::Species && f.name == "Superior Second Wind"));
        assert_eq!(fold.len(), plain.len() - 2);
    }

    #[test]
    fn upgrades_named_otherwise_fold_by_what_they_say() {
        let (plain, fold) = folded(&[
            (
                "ID_T_GROUP",
                "Battle Cry: Rallying Shout",
                2,
                CLASS,
                "<p>You shout.</p>",
            ),
            // "Group: Option" named by its option alone
            (
                "ID_T_IMP_SHOUT",
                "Improved Shout",
                17,
                CLASS,
                "<p>Two creatures hear your Rallying Shout.</p>",
            ),
            (
                "ID_T_WARD",
                "Arcane Ward Guardian",
                3,
                CLASS,
                "<p>A ward protects you.</p>",
            ),
            // "Improved X" whose X ends a longer name the text spells out
            (
                "ID_T_IMP_GUARD",
                "Improved Guardian",
                15,
                CLASS,
                "<p>Your Arcane Ward Guardian gains 5 hit points.</p>",
            ),
            // a name sharing nothing, a description that says the earlier feature improves
            (
                "ID_T_EXPANSION",
                "Wind Expansion",
                18,
                CLASS,
                "<p>18th-level feature. Your Second Wind now also ends one condition.</p>",
            ),
            // a condition on the earlier feature, not an improvement of it
            (
                "ID_T_CALM",
                "Calm Mind",
                7,
                CLASS,
                "<p>While in your Second Wind, you become calm.</p>",
            ),
        ]);
        let names: Vec<_> = fold.iter().map(|f| f.name.as_str()).collect();
        for gone in ["Improved Shout", "Improved Guardian", "Wind Expansion"] {
            assert!(!names.contains(&gone), "{gone} still listed: {names:?}");
        }
        assert!(names.contains(&"Calm Mind"), "{names:?}");
        assert!(text(&fold, "Battle Cry: Rallying Shout").contains("\nImproved Shout (level 17). "));
        assert!(text(&fold, "Arcane Ward Guardian").contains("\nImproved Guardian (level 15). "));
        assert!(text(&fold, "Second Wind").contains("\nWind Expansion (level 18). "));
        assert_eq!(fold.len(), plain.len() - 3);
    }

    #[test]
    fn nothing_folds_without_an_upgrade() {
        let (plain, fold) = folded(&[]);
        assert_eq!(plain, fold);
    }
}
