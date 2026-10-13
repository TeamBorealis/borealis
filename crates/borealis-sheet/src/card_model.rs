//! Card content: spell, attack, item and feature cards.
//!
//! All card kinds share one flowing 3x3 grid, nine cards per page, in the fixed group order spell,
//! attack, item, feature. Page breaks follow section 10.2. [`pages`] returns the cards grouped per
//! page; `layout::page_cards` draws them and `html_sheet` previews them.

use crate::{SheetContent, SheetOptions};
use borealis_engine::snapshot::SpellEntry;
use borealis_engine::CharacterSnapshot;

/// One generated card.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Card {
    pub title: String,
    pub subtitle: String,
    /// Short labelled facts printed under the subtitle.
    pub facts: Vec<(String, String)>,
    pub body: String,
    pub left_footer: String,
    pub right_footer: String,
}

/// A weight without trailing zeros ("2", "0.25").
pub(crate) fn fmt_num(n: f64) -> String {
    if (n - n.round()).abs() < 1e-9 {
        format!("{n:.0}")
    } else {
        format!("{n:.2}").trim_end_matches('0').to_string()
    }
}

/// Cards per page (3 x 3).
pub(crate) const PER_PAGE: usize = 9;
/// Element types that get feature cards.
const FEATURE_CARD_TYPES: &[&str] = &[
    "Vision",
    "Race",
    "Sub Race",
    "Race Variant",
    "Racial Trait",
    "Language Feature",
    "Class",
    "Class Feature",
    "Archetype",
    "Archetype Feature",
    "Feat",
    "Feat Feature",
];

/// Strip tags and collapse whitespace.
pub(crate) fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Left footer of a spell card (section 10.3).
pub(crate) fn spell_footer(sp: &SpellEntry, caster: &str) -> String {
    let Some(g) = &sp.granted_by else {
        return format!("Prepared ({caster})");
    };
    let mut footer = g.name.clone();
    if g.ty == "Racial Trait" {
        if let Some(outer) = &g.granted_by {
            footer = format!("{} ({outer})", g.name);
        }
    }
    if g.name.starts_with("Additional Spell,")
        || (g.name.starts_with("Additional ") && g.name.contains("Spell,"))
    {
        footer = g
            .name
            .replace(&sp.name, "")
            .trim_matches(|c| c == ' ' || c == ',')
            .to_string();
    }
    match &g.class_name {
        Some(class) if footer != *class => format!("{footer} ({class})"),
        _ => footer,
    }
}

/// Abbreviate the source prefixes the generator abbreviates on spell cards.
pub(crate) fn abbreviate_source(source: &str) -> String {
    source
        .replacen("Unearthed Arcana: ", "UA: ", 1)
        .replacen("Adventurers League: ", "AL: ", 1)
}

fn spell_cards(snap: &CharacterSnapshot, content: &SheetContent) -> Vec<Card> {
    // Every spell held (known, granted or prepared), once per element, whatever its caster.
    let mut seen: Vec<(&SpellEntry, &str)> = Vec::new();
    for c in &snap.spellcasting {
        for sp in c.spells.iter().filter(|s| s.known || s.prepared) {
            if !seen.iter().any(|(e, _)| e.id == sp.id) {
                seen.push((sp, c.name.as_str()));
            }
        }
    }
    seen.sort_by(|a, b| (a.0.level, &a.0.name).cmp(&(b.0.level, &b.0.name)));
    seen.into_iter()
        .map(|(sp, caster)| {
            let text = content.spells.get(&sp.id).cloned().unwrap_or_default();
            let subtitle = if text.subtitle.is_empty() {
                match sp.level {
                    0 => "Cantrip".to_string(),
                    n => format!("Level {n} spell"),
                }
            } else {
                text.subtitle
            };
            Card {
                title: sp.name.clone(),
                subtitle,
                body: text.body,
                left_footer: spell_footer(sp, caster),
                right_footer: abbreviate_source(&text.source),
                ..Card::default()
            }
        })
        .collect()
}

fn attack_cards(
    character: &borealis_character::Character,
    snap: &CharacterSnapshot,
    content: &SheetContent,
) -> Vec<Card> {
    let details = &character.details;
    let derived = snap.attacks.iter().filter_map(|a| {
        let line = snap.inventory.items.iter().find(|i| i.uid == a.uid);
        let on = details
            .auto_attack_cards
            .get(&a.uid)
            .copied()
            .unwrap_or_else(|| line.is_some_and(|i| i.show_card));
        on.then(|| Card {
            title: a.name.clone(),
            subtitle: line
                .and_then(|i| content.items.get(&i.item.id))
                .map(|t| t.subtitle.clone())
                .unwrap_or_default(),
            facts: vec![
                ("Range".into(), a.range.clone()),
                ("Attack".into(), format!("{:+}", a.attack)),
                ("Damage".into(), a.damage.clone()),
            ],
            ..Card::default()
        })
    });
    // Typed rows start with the flag off; rows hidden from the table never get a card.
    let manual = details
        .attacks
        .iter()
        .filter(|a| a.displayed && a.item_uid.is_none() && a.as_card == Some(true))
        .map(|a| Card {
            title: a.name.clone(),
            facts: vec![
                ("Range".into(), a.range.clone()),
                ("Attack".into(), a.attack.clone()),
                ("Damage".into(), a.damage.clone()),
            ],
            body: plain(&a.description),
            ..Card::default()
        });
    derived.chain(manual).collect()
}

fn item_cards(snap: &CharacterSnapshot, content: &SheetContent) -> Vec<Card> {
    snap.inventory
        .items
        .iter()
        .filter(|i| i.show_card)
        .map(|i| {
            let text = content.items.get(&i.item.id).cloned().unwrap_or_default();
            Card {
                title: i.name.clone(),
                subtitle: text.subtitle,
                body: text.body,
                left_footer: format!("{} lb", fmt_num(i.weight)),
                right_footer: text.source,
                ..Card::default()
            }
        })
        .collect()
}

fn feature_cards(snap: &CharacterSnapshot) -> Vec<Card> {
    snap.features
        .iter()
        .filter(|f| f.displayed && FEATURE_CARD_TYPES.contains(&f.ty.as_str()))
        .filter(|f| {
            !f.name.starts_with("Ability Score Increase")
                && !f.name.starts_with("Ability Score Improvement")
        })
        .map(|f| Card {
            title: f.name.clone(),
            subtitle: f.ty.clone(),
            body: plain(&f.description),
            left_footer: f.granted_by.clone().unwrap_or_default(),
            right_footer: f.source.clone(),
            ..Card::default()
        })
        .collect()
}

/// Flowing 3x3 grid with the page-break rule of section 10.2.
#[derive(Default)]
struct Flow {
    pages: Vec<Vec<Card>>,
    current: Vec<Card>,
    any_added: bool,
}

impl Flow {
    fn add(&mut self, card: Card) {
        if self.current.len() == PER_PAGE {
            self.pages.push(std::mem::take(&mut self.current));
        }
        self.current.push(card);
        self.any_added = true;
    }

    /// Add a whole group; `new_page` ends the current page first, but only when the group has
    /// cards and something precedes it (Borealis does not reproduce the blank-page quirk).
    fn group(&mut self, new_page: bool, cards: Vec<Card>) {
        if cards.is_empty() {
            return;
        }
        if new_page && self.any_added && !self.current.is_empty() {
            self.pages.push(std::mem::take(&mut self.current));
        }
        cards.into_iter().for_each(|c| self.add(c));
    }
}

/// The cards of the enabled groups, nine to a page, appended after all other pages.
pub(crate) fn pages(
    character: &borealis_character::Character,
    snap: &CharacterSnapshot,
    options: &SheetOptions,
    content: &SheetContent,
) -> Vec<Vec<Card>> {
    let k = options.cards;
    let b = options.cards_new_page;
    let mut flow = Flow::default();
    if k.spell {
        flow.group(false, spell_cards(snap, content));
    }
    if k.attack {
        flow.group(b.attack, attack_cards(character, snap, content));
    }
    if k.item {
        flow.group(b.item, item_cards(snap, content));
    }
    if k.feature {
        flow.group(b.feature, feature_cards(snap));
    }
    let Flow {
        mut pages, current, ..
    } = flow;
    if !current.is_empty() {
        pages.push(current);
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_engine::snapshot::SpellGrant;

    fn spell(name: &str, grant: Option<SpellGrant>) -> SpellEntry {
        SpellEntry {
            id: name.into(),
            name: name.into(),
            level: 1,
            prepared: true,
            always_prepared: false,
            known: true,
            damage: None,
            granted_by: grant,
        }
    }

    fn grant(name: &str, ty: &str, by: Option<&str>, class: Option<&str>) -> SpellGrant {
        SpellGrant {
            name: name.into(),
            ty: ty.into(),
            granted_by: by.map(Into::into),
            class_name: class.map(Into::into),
        }
    }

    #[test]
    fn footer_without_a_granting_element_names_the_preparing_class() {
        assert_eq!(
            spell_footer(&spell("Bless", None), "Cleric"),
            "Prepared (Cleric)"
        );
    }

    #[test]
    fn footer_names_the_granter_and_its_class_once() {
        let g = grant("Domain Spells", "Class Feature", None, Some("Cleric"));
        assert_eq!(
            spell_footer(&spell("Bless", Some(g)), "Cleric"),
            "Domain Spells (Cleric)"
        );
        let same = grant("Cleric", "Class", None, Some("Cleric"));
        assert_eq!(
            spell_footer(&spell("Bless", Some(same)), "Cleric"),
            "Cleric"
        );
    }

    #[test]
    fn racial_trait_footer_adds_the_racial_granter() {
        let g = grant("Infernal Legacy", "Racial Trait", Some("Tiefling"), None);
        assert_eq!(
            spell_footer(&spell("Thaumaturgy", Some(g)), "x"),
            "Infernal Legacy (Tiefling)"
        );
    }

    #[test]
    fn additional_spell_footer_drops_the_spell_name() {
        let g = grant("Additional Spell, Shield", "Feat Feature", None, None);
        assert_eq!(
            spell_footer(&spell("Shield", Some(g)), "x"),
            "Additional Spell"
        );
        let g = grant(
            "Additional Wizard Spell, Shield",
            "Feat Feature",
            None,
            None,
        );
        assert_eq!(
            spell_footer(&spell("Shield", Some(g)), "x"),
            "Additional Wizard Spell"
        );
    }

    #[test]
    fn source_prefixes_are_abbreviated() {
        assert_eq!(abbreviate_source("Unearthed Arcana: Foo"), "UA: Foo");
        assert_eq!(abbreviate_source("Adventurers League: Bar"), "AL: Bar");
        assert_eq!(abbreviate_source("Player's Handbook"), "Player's Handbook");
    }
}
