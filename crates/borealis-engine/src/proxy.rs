//! Aurora's internal proxy items (`decisions-engine.md` B-06).
//!
//! Aurora generates, for every Feat, Language, Proficiency, ability-score improvement and Spell
//! of a non-`Core`/`Internal` source, a hidden Item ("Additional Feat, Grappler") whose only rule
//! grants that element (`InternalElementsGenerator`, called from `DataManager` after loading the
//! content). Spells get one such item per spellcasting name (plus one without), and the grant of
//! a named one carries `spellcasting="<name>"`. The item has slot `proxy`; equipping it enables
//! the grant. Saved characters keep these items in their inventory and element tree, so an
//! imported character needs them to keep the feat, spell or proficiency.
//!
//! The ids follow Aurora's formula (`ID_<SOURCE ABBR>_INTERNAL_ITEM_FEAT_PROXY_<rest of the feat
//! id after its last _FEAT_>`, upper-cased, see [`proxy_id`]). Borealis does not add tens of
//! thousands of such items to every universe: [`ProxyTable`] indexes the targets once, on the
//! first lookup of an id of that shape, and builds each item the first time it is asked for.
//! Proxies are therefore resolvable by id ([`crate::universe::Universe::get`]) but are never
//! listed by type, offered, or counted.

use crate::universe::{UElem, Universe};
use borealis_content::{Compendium, Element, GrantRule, Rule, Setter, Sheet};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Marker every proxy id contains.
const ITEM_MARKER: &str = "_INTERNAL_ITEM_";
/// The spellcasting block Aurora leaves out of the spell proxies.
const MYSTIC_PSIONICS: &str = "ID_WOTC_UA20170313_CLASS_FEATURE_MYSTIC_PSIONICS";
/// The six ability-score improvement elements Aurora makes proxies for.
const ASI_IDS: [&str; 6] = [
    "ID_INTERNAL_ASI_STRENGTH",
    "ID_INTERNAL_ASI_DEXTERITY",
    "ID_INTERNAL_ASI_CONSTITUTION",
    "ID_INTERNAL_ASI_INTELLIGENCE",
    "ID_INTERNAL_ASI_WISDOM",
    "ID_INTERNAL_ASI_CHARISMA",
];

/// The kinds of proxy Aurora generates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Feat,
    Language,
    Proficiency,
    Asi,
    Spell,
}

impl Kind {
    /// The part of the target id Aurora splits on (the proxy keeps what follows its last match).
    fn split(self) -> &'static str {
        match self {
            Self::Feat => "_FEAT_",
            Self::Language => "_LANGUAGE_",
            Self::Proficiency => "_PROFICIENCY_",
            Self::Asi => "_ASI_",
            Self::Spell => "_SPELL_",
        }
    }

    /// What follows `_INTERNAL_ITEM_` in the proxy id, before the kept part.
    fn infix(self) -> &'static str {
        match self {
            Self::Feat => "FEAT_PROXY_",
            Self::Language => "LANGUAGE_PROXY_LANGUAGE_",
            Self::Proficiency => "PROFICIENCY_PROXY_PROFICIENCY_",
            Self::Asi => "PROXY_ASI_",
            Self::Spell => "_SPELL_PROXY_SPELL_",
        }
    }

    /// The Aurora element types (and, for ability-score improvements, ids) that get a proxy.
    fn of(el: &Element) -> Option<Self> {
        let asi = (el.ty == "Ability Score Improvement" && el.id == ASI_IDS[0])
            || ASI_IDS[1..].contains(&el.id.as_str());
        match el.ty.as_str() {
            _ if asi => Some(Self::Asi),
            "Feat" => Some(Self::Feat),
            "Language" => Some(Self::Language),
            "Proficiency" => Some(Self::Proficiency),
            "Spell" => Some(Self::Spell),
            _ => None,
        }
    }
}

/// True for an id of Aurora's proxy shape (the only ids [`ProxyTable`] is consulted for).
#[must_use]
pub fn is_proxy_id(id: &str) -> bool {
    id.contains(ITEM_MARKER) && id.contains("PROXY")
}

/// Aurora's id of the proxy item for `target` (source abbreviation `abbr`, spellcasting name
/// `cast`, empty for none), or `None` when Aurora makes no proxy for it.
#[must_use]
pub fn proxy_id(target: &Element, abbr: &str, cast: &str) -> Option<String> {
    let kind = Kind::of(target)?;
    let rest = kept_part(target, kind)?;
    let middle = match kind {
        Kind::Spell => format!("{}{}", cast.replace(' ', "_"), kind.infix()),
        _ => kind.infix().to_string(),
    };
    Some(format!("ID_{abbr}{ITEM_MARKER}{middle}{rest}").to_uppercase())
}

/// The part of the target id the proxy id keeps, when Aurora makes a proxy for the element.
fn kept_part(el: &Element, kind: Kind) -> Option<&str> {
    if is_core(el) {
        return None;
    }
    split_part(el, kind)
}

/// Aurora skips elements of these sources.
fn is_core(el: &Element) -> bool {
    el.source == "Core" || el.source == "Internal"
}

/// What follows the last `kind.split()` in the id (`None` when the id lacks it).
fn split_part(el: &Element, kind: Kind) -> Option<&str> {
    if !el.id.contains(kind.split()) {
        return None;
    }
    el.id.rsplit(kind.split()).next()
}

/// A proxy id taken apart: kind, upper-cased source abbreviation, spellcasting key, kept part.
fn parse(id: &str) -> Option<(Kind, &str, &str, &str)> {
    let rest = id.strip_prefix("ID_")?;
    let at = rest.find(ITEM_MARKER)?;
    let (abbr, tail) = (&rest[..at], &rest[at + ITEM_MARKER.len()..]);
    for kind in [Kind::Feat, Kind::Language, Kind::Proficiency, Kind::Asi] {
        if let Some(kept) = tail.strip_prefix(kind.infix()) {
            return Some((kind, abbr, "", kept));
        }
    }
    let at = tail.find(Kind::Spell.infix())?;
    Some((
        Kind::Spell,
        abbr,
        &tail[..at],
        &tail[at + Kind::Spell.infix().len()..],
    ))
}

/// Index of the proxy targets of one universe, and the proxies built so far.
#[derive(Debug, Default)]
pub struct ProxyTable {
    /// `(kind, ABBR, KEPT PART)` -> (target element, first slot in `built`). The first element
    /// with a key wins, as Aurora's id lookup returns the first match.
    targets: HashMap<(Kind, String, String), (usize, usize)>,
    /// `(kind, KEPT PART)` -> the one element with it (`None` = several), `Core`/`Internal`
    /// ones included. A proxy whose exact key is absent resolves to that element: Borealis's
    /// core pack sources its catalogue (skills, saves, languages...) as `Core` (B-13) where
    /// Aurora's own copies name their book, and installed content may lack the `Source`
    /// element that gives the abbreviation, or name another book.
    by_kept: HashMap<(Kind, String), Option<(usize, usize)>>,
    /// Spellcasting names (index 0 = none) and their id keys (`Battle Mage` ->
    /// `BATTLE_MAGE`).
    casts: Vec<String>,
    cast_keys: HashMap<String, usize>,
    /// One slot per target and spellcasting name (non-spell targets have one).
    built: Vec<OnceLock<Box<UElem>>>,
}

impl ProxyTable {
    /// Index every element of `uni` that Aurora makes a proxy for.
    pub(crate) fn build(uni: &Universe) -> Self {
        let mut abbrs: HashMap<&str, String> = HashMap::new();
        for s in uni.by_type("Source") {
            abbrs
                .entry(s.el.name.as_str())
                .or_insert_with(|| s.el.setter_value("abbreviation").unwrap_or("").to_string());
        }
        let mut casts = vec![String::new()];
        for e in uni.iter() {
            let Some(sc) = e.el.spellcasting.as_ref() else {
                continue;
            };
            if sc.extend == Some(true) || e.el.id == MYSTIC_PSIONICS {
                continue;
            }
            let name = sc.name.clone().unwrap_or_default();
            if !casts.contains(&name) {
                casts.push(name);
            }
        }
        let cast_keys = casts
            .iter()
            .enumerate()
            .map(|(i, c)| (c.replace(' ', "_").to_uppercase(), i))
            .collect();
        let mut table = Self {
            casts,
            cast_keys,
            ..Self::default()
        };
        for (i, e) in uni.iter().enumerate() {
            let Some(kind) = Kind::of(&e.el) else {
                continue;
            };
            let Some(kept) = split_part(&e.el, kind) else {
                continue;
            };
            let at = table.slots_for(kind);
            let loose = (kind, kept.to_uppercase());
            match table.by_kept.get_mut(&loose) {
                Some(seen) => *seen = None,
                None => {
                    table.by_kept.insert(loose, Some((i, at)));
                }
            }
            if is_core(&e.el) {
                continue;
            }
            let abbr = source_abbreviation(&abbrs, &e.el, kind);
            table
                .targets
                .entry((kind, abbr.to_uppercase(), kept.to_uppercase()))
                .or_insert((i, at));
        }
        table
    }

    /// Reserve the build slots of one target; returns the first.
    fn slots_for(&mut self, kind: Kind) -> usize {
        let at = self.built.len();
        let n = if kind == Kind::Spell {
            self.casts.len()
        } else {
            1
        };
        self.built.extend((0..n).map(|_| OnceLock::new()));
        at
    }

    /// The proxy item with this id, built on first use.
    pub(crate) fn get<'u>(&'u self, uni: &'u Universe, id: &str) -> Option<&'u UElem> {
        let (kind, abbr, cast_key, kept) = parse(id)?;
        let cast = if kind == Kind::Spell {
            *self.cast_keys.get(cast_key)?
        } else {
            0
        };
        let &(target, first) = match self
            .targets
            .get(&(kind, abbr.to_string(), kept.to_string()))
        {
            Some(t) => t,
            None => self.by_kept.get(&(kind, kept.to_string()))?.as_ref()?,
        };
        let target = uni.at(target)?;
        let slot = self.built.get(first + cast)?;
        Some(slot.get_or_init(|| Box::new(make(target, kind, id, &self.casts[cast]))))
    }
}

/// Aurora's source abbreviation of an element: the `abbreviation` of the first Source element
/// with the element's source name; a spell with no abbreviation uses the first letter of the
/// source name when that Source exists.
fn source_abbreviation(abbrs: &HashMap<&str, String>, el: &Element, kind: Kind) -> String {
    match abbrs.get(el.source.as_str()) {
        Some(a) if a.is_empty() && kind == Kind::Spell => el
            .source
            .chars()
            .next()
            .map(String::from)
            .unwrap_or_default(),
        Some(a) => a.clone(),
        None => String::new(),
    }
}

/// The proxy item for `target` (Aurora's names, setters and single grant rule).
fn make(target: &UElem, kind: Kind, id: &str, cast: &str) -> UElem {
    let t = &target.el;
    let shown = match kind {
        Kind::Proficiency if t.supports.iter().any(|s| s == "Skill") => {
            format!("Skill Proficiency ({})", t.name)
        }
        _ => t.name.clone(),
    };
    let category = if cast.is_empty() {
        format!("Additional {}", t.ty)
    } else {
        format!("Additional {cast} {}", t.ty)
    };
    let mut el = Element::new(id, format!("{category}, {shown}"), "Item", t.source.clone());
    el.ruleset = t.ruleset;
    el.description = format!(
        "<p>Aurora proxy item: equip it to enable it. It grants {shown} and stays hidden from the \
         inventory on the character sheet.</p>"
    );
    el.setters = vec![
        Setter::new("slot", "proxy"),
        Setter::new("inventory-hidden", "true"),
        Setter::new("category", category.clone()),
    ];
    if kind == Kind::Spell {
        el.sheet = Some(Sheet {
            alt: Some(el.name.replacen("Additional ", "", 1)),
            ..Sheet::default()
        });
    }
    el.compendium = Some(Compendium {
        display: Some(false),
        ..Compendium::default()
    });
    el.rules = vec![Rule::Grant(GrantRule {
        id: t.id.clone(),
        ty: Some(t.ty.clone()),
        spellcasting: (!cast.is_empty()).then(|| cast.to_string()),
        ..GrantRule::default()
    })];
    UElem::build(el, 0, true, target.edition.clone(), Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(id: &str, ty: &str, source: &str) -> Element {
        Element::new(id, "Fog Cloud", ty, source)
    }

    #[test]
    fn proxy_ids_follow_auroras_formula() {
        let spell = el("ID_PHB_SPELL_FOG_CLOUD", "Spell", "Player's Handbook");
        assert_eq!(
            proxy_id(&spell, "PHB", "").as_deref(),
            Some("ID_PHB_INTERNAL_ITEM__SPELL_PROXY_SPELL_FOG_CLOUD")
        );
        assert_eq!(
            proxy_id(&spell, "PHB", "Battle Mage").as_deref(),
            Some("ID_PHB_INTERNAL_ITEM_BATTLE_MAGE_SPELL_PROXY_SPELL_FOG_CLOUD")
        );
        let feat = el("ID_ZZ_BOOK_FEAT_LOOKOUT", "Feat", "Invented Book");
        assert_eq!(
            proxy_id(&feat, "ZZB", "").as_deref(),
            Some("ID_ZZB_INTERNAL_ITEM_FEAT_PROXY_LOOKOUT")
        );
        let skill = el(
            "ID_PROFICIENCY_SKILL_STEALTH",
            "Proficiency",
            "Player's Handbook",
        );
        assert_eq!(
            proxy_id(&skill, "PHB", "").as_deref(),
            Some("ID_PHB_INTERNAL_ITEM_PROFICIENCY_PROXY_PROFICIENCY_SKILL_STEALTH")
        );
        let lang = el("ID_ZZ_BOOK_LANGUAGE_OLDTONGUE", "Language", "Invented Book");
        assert_eq!(
            proxy_id(&lang, "ZZB", "").as_deref(),
            Some("ID_ZZB_INTERNAL_ITEM_LANGUAGE_PROXY_LANGUAGE_OLDTONGUE")
        );
    }

    #[test]
    fn core_and_internal_elements_and_odd_ids_get_no_proxy() {
        assert_eq!(proxy_id(&el("ID_X_FEAT_A", "Feat", "Core"), "", ""), None);
        assert_eq!(
            proxy_id(&el("ID_X_FEAT_A", "Feat", "Internal"), "", ""),
            None
        );
        assert_eq!(proxy_id(&el("ID_X_TALENT_A", "Feat", "Book"), "", ""), None);
        assert_eq!(proxy_id(&el("ID_X_FEAT_A", "Class", "Book"), "", ""), None);
    }

    #[test]
    fn parsing_inverts_the_formula() {
        assert_eq!(
            parse("ID_PHB_INTERNAL_ITEM__SPELL_PROXY_SPELL_FOG_CLOUD"),
            Some((Kind::Spell, "PHB", "", "FOG_CLOUD"))
        );
        assert_eq!(
            parse("ID_PHB_INTERNAL_ITEM_CLERIC_SPELL_PROXY_SPELL_BLESS"),
            Some((Kind::Spell, "PHB", "CLERIC", "BLESS"))
        );
        assert_eq!(
            parse("ID__INTERNAL_ITEM_FEAT_PROXY_LOOKOUT"),
            Some((Kind::Feat, "", "", "LOOKOUT"))
        );
        assert_eq!(parse("ID_INTERNAL_GRANTS_X"), None);
        assert!(is_proxy_id("ID_ZZB_INTERNAL_ITEM_FEAT_PROXY_LOOKOUT"));
        assert!(!is_proxy_id("ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE"));
    }
}
