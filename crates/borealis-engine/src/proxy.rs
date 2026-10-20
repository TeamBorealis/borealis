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
//! first lookup of an id of that shape (or the first listing), and builds each item the first
//! time it is asked for. Proxies are resolvable by id ([`crate::universe::Universe::get`]) and
//! listed only by [`Universe::proxies`] (the equipment screen's generated "Additional ..."
//! categories, `specs/proxy-items.md`); they are never listed by type, offered, or counted.

use crate::universe::{UElem, Universe};
use borealis_content::{Compendium, Element, GrantRule, Rule, Ruleset, Setter, Sheet};
use std::collections::hash_map::Entry;
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
/// Abbreviation under which `Core`-sourced targets are listed: Aurora's own copies of the skill,
/// language, save, tool and ASI catalogue name the edition's base book (IM13 D2).
const BASE_BOOK_ABBR: &str = "PHB";
/// Proficiencies Aurora itself sources `Core`, so it lists no proxy for them (IM13 D2).
const SPELL_FOCUS_GROUPS: &str = "ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_";
/// Every proxy name and category starts with this.
const ADDITIONAL: &str = "Additional ";
/// The kinds in the order of their generated categories (`specs/proxy-items.md` 1.3).
const KIND_ORDER: [Kind; 5] = [
    Kind::Feat,
    Kind::Language,
    Kind::Proficiency,
    Kind::Asi,
    Kind::Spell,
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

    /// The target type as the proxy's name and category spell it.
    fn label(self) -> &'static str {
        match self {
            Self::Feat => "Feat",
            Self::Language => "Language",
            Self::Proficiency => "Proficiency",
            Self::Asi => "Ability Score Improvement",
            Self::Spell => "Spell",
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
    Some(format_id(kind, abbr, cast, rest))
}

/// Aurora's proxy id from its parts.
fn format_id(kind: Kind, abbr: &str, cast: &str, rest: &str) -> String {
    let middle = match kind {
        Kind::Spell => format!("{}{}", cast.replace(' ', "_"), kind.infix()),
        _ => kind.infix().to_string(),
    };
    format!("ID_{abbr}{ITEM_MARKER}{middle}{rest}").to_uppercase()
}

/// The generated category of a proxy kind and spellcasting name (empty for none).
fn category_name(kind: Kind, cast: &str) -> String {
    if cast.is_empty() {
        format!("{ADDITIONAL}{}", kind.label())
    } else {
        format!("{ADDITIONAL}{cast} {}", kind.label())
    }
}

/// How a proxy names its target ("Skill Proficiency (Stealth)" for a skill).
fn shown_name(target: &UElem, kind: Kind) -> String {
    match kind {
        Kind::Proficiency if target.el.supports.iter().any(|s| s == "Skill") => {
            format!("Skill Proficiency ({})", target.el.name)
        }
        _ => target.el.name.clone(),
    }
}

/// Aurora's keywords of a proxy: the target's, and for a spell the spellcasting name and every
/// support of the spell.
fn proxy_keywords(target: &UElem, kind: Kind, cast: &str) -> Vec<String> {
    let mut out: Vec<String> = target
        .el
        .setters
        .iter()
        .filter(|s| s.name == "keywords")
        .flat_map(|s| s.value.split(','))
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_string)
        .collect();
    if kind == Kind::Spell {
        if !cast.is_empty() {
            out.push(cast.to_string());
        }
        out.extend(target.supports.iter().cloned());
    }
    out
}

/// Is this `Core`-sourced element one Aurora's own (book-sourced) copy would list (IM13 D2)?
fn listed_as_base_book(el: &Element) -> bool {
    el.source == "Core" && !el.id.starts_with(SPELL_FOCUS_GROUPS)
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
    /// The targets Aurora lists a proxy for, each proxy id once (the first element wins), in
    /// content order, then the `Core` ones listed as the base book (IM13 D2).
    listed: Vec<Listed>,
}

/// One listed proxy target: its kind, upper-cased abbreviation and element index.
#[derive(Debug)]
struct Listed {
    kind: Kind,
    abbr: String,
    target: usize,
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
        let mut base_book: Vec<(Kind, String, usize, usize)> = Vec::new();
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
                if listed_as_base_book(&e.el) {
                    base_book.push((kind, kept.to_uppercase(), i, at));
                }
                continue;
            }
            let abbr = source_abbreviation(&abbrs, &e.el, kind).to_uppercase();
            table.list((kind, abbr, kept.to_uppercase()), (i, at));
        }
        // An id that no book element claims resolves to the Core element it is listed for; the
        // loose fallback above already did so whenever that element was the only one.
        for (kind, kept, i, at) in base_book {
            table.list((kind, BASE_BOOK_ABBR.to_string(), kept), (i, at));
        }
        table
    }

    /// Index a target under its exact key and list it, unless an earlier one has that key.
    fn list(&mut self, key: (Kind, String, String), at: (usize, usize)) {
        if let Entry::Vacant(v) = self.targets.entry(key) {
            let (kind, abbr, _) = v.key().clone();
            v.insert(at);
            self.listed.push(Listed {
                kind,
                abbr,
                target: at.0,
            });
        }
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

/// One listed proxy item, described without building it (`specs/proxy-items.md` 1.1).
#[derive(Debug, Clone)]
pub struct ProxyEntry<'u> {
    /// Aurora's proxy id (resolves through [`Universe::get`]).
    pub id: String,
    /// "Additional Feat, Grappler".
    pub name: String,
    /// "Additional Feat", "Additional Wizard Spell".
    pub category: String,
    /// The target's source (Aurora lists a proxy under its target's source).
    pub source: &'u str,
    /// The target's ruleset.
    pub ruleset: Option<Ruleset>,
    /// The element the proxy grants.
    pub target: &'u UElem,
    kind: Kind,
    cast: &'u str,
}

impl ProxyEntry<'_> {
    /// Aurora's keywords of the proxy (the target's; a spell adds its spellcasting name and
    /// supports), which the equipment search matches besides the name.
    #[must_use]
    pub fn keywords(&self) -> Vec<String> {
        proxy_keywords(self.target, self.kind, self.cast)
    }
}

/// The proxy items a universe lists (the equipment screen's generated categories).
#[derive(Clone, Copy)]
pub struct Proxies<'u> {
    table: &'u ProxyTable,
    uni: &'u Universe,
}

impl<'u> Proxies<'u> {
    /// The generated category names in Aurora's order: Feat, Language, Proficiency, Ability
    /// Score Improvement, Spell, then one spell category per spellcasting name.
    #[must_use]
    pub fn categories(&self) -> Vec<String> {
        let plain = KIND_ORDER.iter().map(|k| category_name(*k, ""));
        let cast = self.table.casts[1..]
            .iter()
            .map(|c| category_name(Kind::Spell, c));
        plain.chain(cast).collect()
    }

    /// Is `name` one of the generated categories?
    #[must_use]
    pub fn is_category(&self, name: &str) -> bool {
        self.category(name).is_some()
    }

    /// The kind and spellcasting index of a generated category.
    fn category(&self, name: &str) -> Option<(Kind, usize)> {
        let rest = name.strip_prefix(ADDITIONAL)?;
        if let Some(k) = KIND_ORDER.iter().find(|k| k.label() == rest) {
            return Some((*k, 0));
        }
        let cast = rest.strip_suffix(Kind::Spell.label())?.strip_suffix(' ')?;
        let at = self.table.casts.iter().skip(1).position(|c| c == cast)?;
        Some((Kind::Spell, at + 1))
    }

    /// Every listed proxy: each target once, and each spell once per spellcasting name.
    pub fn all(self) -> impl Iterator<Item = ProxyEntry<'u>> + 'u {
        let casts = self.table.casts.len();
        self.table.listed.iter().flat_map(move |l| {
            let n = if l.kind == Kind::Spell { casts } else { 1 };
            (0..n).filter_map(move |c| self.entry(l, c))
        })
    }

    /// The proxies of one generated category (none for any other name).
    pub fn in_category(self, name: &str) -> impl Iterator<Item = ProxyEntry<'u>> + 'u {
        let wanted = self.category(name);
        self.table
            .listed
            .iter()
            .filter(move |l| wanted.is_some_and(|(k, _)| k == l.kind))
            .filter_map(move |l| self.entry(l, wanted.map_or(0, |(_, c)| c)))
    }

    fn entry(&self, l: &Listed, cast: usize) -> Option<ProxyEntry<'u>> {
        let target = self.uni.at(l.target)?;
        let rest = split_part(&target.el, l.kind)?;
        let cast_name = self.table.casts.get(cast)?.as_str();
        let category = category_name(l.kind, cast_name);
        Some(ProxyEntry {
            id: format_id(l.kind, &l.abbr, cast_name, rest),
            name: format!("{category}, {}", shown_name(target, l.kind)),
            category,
            source: target.el.source.as_str(),
            ruleset: target.el.ruleset,
            target,
            kind: l.kind,
            cast: cast_name,
        })
    }
}

impl Universe {
    /// The proxy items this universe lists ([`Proxies`]); they are built only when resolved.
    #[must_use]
    pub fn proxies(&self) -> Proxies<'_> {
        Proxies {
            table: self.proxy_table(),
            uni: self,
        }
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
    let shown = shown_name(target, kind);
    let category = category_name(kind, cast);
    let mut el = Element::new(id, format!("{category}, {shown}"), "Item", t.source.clone());
    el.ruleset = t.ruleset;
    // Borealis's own hint, then the target's description (the reference Aurora embeds).
    el.description = format!(
        "<p>Equip this item to gain {shown}. It is left out of the inventory on the character \
         sheet.</p>{}",
        t.description
    );
    el.setters = vec![
        Setter::new("slot", "proxy"),
        Setter::new("inventory-hidden", "true"),
        Setter::new("category", category.clone()),
    ];
    let keywords = proxy_keywords(target, kind, cast);
    if !keywords.is_empty() {
        el.setters
            .push(Setter::new("keywords", keywords.join(", ")));
    }
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
