//! Semantic validation of a loaded [`ContentStore`](crate::store::ContentStore).
//!
//! Reports problems with file and element context. Dangling references are warnings
//! (a pack may legitimately lean on content that is not loaded); structural problems
//! (bad ids, missing required setters, unparsable expressions) are errors or warnings as
//! documented on [`IssueCode`](crate::issue::IssueCode).

use crate::element::Element;
use crate::expr::{BoolExpr, RequirementLeaf, RequirementsExpr, SupportsExpr};
use crate::issue::{Issue, IssueCode, Severity};
use crate::monster::MonsterData;
use crate::rules::Rule;
use crate::setter;
use crate::store::{ContentStore, StoredElement};
use std::collections::{BTreeSet, HashSet};

/// Characters that are operators in the expression languages and so cannot be in an id.
pub const ID_FORBIDDEN: &[char] = &['(', ')', ',', '&', '|', '!', '[', ']', ':', '\'', '’'];

/// Id prefixes/suffixes of elements the loader synthesizes (never stored in packs).
const SYNTHESIZED_PREFIXES: &[&str] = &[
    "ID_INTERNAL_CLASS_FEATURE_ASI_",
    "ID_INTERNAL_CLASS_FEATURE_FEAT_",
    "ID_INTERNAL_MAGIC_ITEM_SPELL_SCROLL_",
];

/// True if `id` has the shape of an element the loader generates after parsing
/// (ASI/feat class features, spell scrolls, proxy items, ignore markers).
#[must_use]
pub fn is_synthesized_id(id: &str) -> bool {
    SYNTHESIZED_PREFIXES.iter().any(|p| id.starts_with(p))
        || id.ends_with("_IGNORE")
        || (id.contains("_INTERNAL_ITEM_") && id.contains("_PROXY"))
}

/// Ids a requirement uses only under an odd number of `!`: an unknown one is never held, so the
/// test is always met and nothing the element needs is missing (AuroraLegacy's races read
/// `!(ID_WOTC_WGTE_GRANTS_DARKMARKED||...)`, a marker of content the user may not have).
fn only_negated(expr: &BoolExpr<RequirementLeaf>) -> HashSet<&str> {
    fn walk<'e>(
        e: &'e BoolExpr<RequirementLeaf>,
        negated: bool,
        neg: &mut HashSet<&'e str>,
        pos: &mut HashSet<&'e str>,
    ) {
        match e {
            BoolExpr::Leaf(RequirementLeaf::Id(id)) => {
                if negated {
                    neg.insert(id.as_str());
                } else {
                    pos.insert(id.as_str());
                }
            }
            BoolExpr::Leaf(RequirementLeaf::Test(_)) => {}
            BoolExpr::Not(inner) => walk(inner, !negated, neg, pos),
            BoolExpr::And(items) | BoolExpr::Or(items) => {
                items.iter().for_each(|i| walk(i, negated, neg, pos));
            }
        }
    }
    let (mut neg, mut pos) = (HashSet::new(), HashSet::new());
    walk(expr, false, &mut neg, &mut pos);
    neg.retain(|id| !pos.contains(id));
    neg
}

/// Unknown ids that sit directly in an `||` beside an operand whose ids are all known: the
/// requirement still holds through that operand, so the unknown alternative is merely never met
/// (e.g. a class that cannot multiclass, named with its would-be multiclass id).
fn optional_alternatives<'e>(
    expr: &'e BoolExpr<RequirementLeaf>,
    known: &dyn Fn(&str) -> bool,
) -> HashSet<&'e str> {
    let mut out = HashSet::new();
    collect_optional(expr, known, &mut out);
    out
}

fn collect_optional<'e>(
    expr: &'e BoolExpr<RequirementLeaf>,
    known: &dyn Fn(&str) -> bool,
    out: &mut HashSet<&'e str>,
) {
    let resolves = |e: &BoolExpr<RequirementLeaf>| {
        e.leaves().into_iter().all(|l| match l {
            RequirementLeaf::Id(id) => known(id),
            RequirementLeaf::Test(_) => true,
        })
    };
    match expr {
        BoolExpr::Leaf(_) => {}
        BoolExpr::Not(inner) => collect_optional(inner, known, out),
        BoolExpr::And(items) => items.iter().for_each(|i| collect_optional(i, known, out)),
        BoolExpr::Or(items) => {
            for (i, item) in items.iter().enumerate() {
                match item {
                    BoolExpr::Leaf(RequirementLeaf::Id(id))
                        if !known(id)
                            && items
                                .iter()
                                .enumerate()
                                .any(|(j, other)| j != i && resolves(other)) =>
                    {
                        out.insert(id.as_str());
                    }
                    _ => collect_optional(item, known, out),
                }
            }
        }
    }
}

/// Setters a type's parser refuses to load without.
#[must_use]
pub fn required_setters(ty: &str) -> &'static [&'static str] {
    match ty {
        "Class" => &["hd"],
        "Level" => &["level", "experience"],
        "Deity" => &["alignment", "domains", "symbol"],
        "Source" => &["url", "abbreviation"],
        "Spell" => &["level", "school", "time", "duration", "range"],
        "Monster" => &[
            "size",
            "type",
            "alignment",
            "str",
            "dex",
            "con",
            "int",
            "wis",
            "cha",
            "ac",
            "hp",
            "hd",
        ],
        "Companion" => &[
            "strength",
            "dexterity",
            "constitution",
            "intelligence",
            "wisdom",
            "charisma",
            "type",
            "size",
            "challenge",
            "alignment",
            "ac",
        ],
        _ => &[],
    }
}

/// Custom rule fields that name a spell element by id, as `(kind, field)`: a grant of a spell the
/// pack does not have never reaches the sheet, so it is a dangling reference like a `grant`'s.
const CUSTOM_SPELL_REFS: [(&str, &str); 2] =
    [("grant_spell", "spell"), ("spell_cast_alt_cost", "spell")];

/// Id segments (after `ID_`) searched for a book abbreviation: `ID_PHB_...`, `ID_WOTC_XGE_...`.
const ABBREVIATION_SEGMENTS: usize = 2;

/// The first id segments of `id` that may carry a book abbreviation (`ID_WOTC_PSA_X`: WOTC, PSA).
pub fn abbreviation_segments(id: &str) -> impl Iterator<Item = &str> {
    id.strip_prefix("ID_")
        .unwrap_or_default()
        .split('_')
        .take(ABBREVIATION_SEGMENTS)
}

/// Upper-cased `abbreviation` setters of the store's `Source` elements.
fn book_abbreviations(store: &ContentStore) -> HashSet<String> {
    store
        .by_type("Source")
        .filter_map(|s| s.element.setter_value("abbreviation"))
        .map(|a| a.trim().to_ascii_uppercase())
        .filter(|a| !a.is_empty())
        .collect()
}

struct Ctx<'a> {
    store: &'a ContentStore,
    stored: &'a StoredElement,
    /// [`book_abbreviations`] of the store.
    books: &'a HashSet<String>,
    issues: Vec<Issue>,
}

impl Ctx<'_> {
    fn push(&mut self, issue: Issue) {
        self.issues.push(
            issue
                .in_file(self.stored.file.to_string())
                .at_element(self.stored.element.id.clone()),
        );
    }

    fn check_ref(&mut self, id: &str, path: &str, what: &str) {
        if id.is_empty() || self.store.contains_id(id) || is_synthesized_id(id) {
            return;
        }
        self.push(
            Issue::warning(
                IssueCode::DanglingReference,
                format!("{what} references unknown element {id}"),
            )
            .at_path(path),
        );
    }

    fn check_requirements(&mut self, text: &str, path: &str) {
        match RequirementsExpr::parse(text) {
            Ok(Some(expr)) => {
                let store = self.store;
                let known = |id: &str| store.contains_id(id) || is_synthesized_id(id);
                let optional = optional_alternatives(expr.root(), &known);
                let absent = only_negated(expr.root());
                for id in expr.referenced_ids() {
                    // an unknown id naming a loaded book is a misspelling (review M2): the test
                    // always passes, so what it meant to exclude is offered; a warning
                    let names_loaded_book = abbreviation_segments(id)
                        .any(|seg| self.books.contains(&seg.to_ascii_uppercase()));
                    if absent.contains(id) && !known(id) && !names_loaded_book {
                        self.push(
                            Issue::new(
                                Severity::Info,
                                IssueCode::DanglingReference,
                                format!(
                                    "requirements test the absence of an element no loaded content defines (this absence test always passes), references unknown element {id}"
                                ),
                            )
                            .at_path(path),
                        );
                    } else if optional.contains(id) {
                        self.push(
                            Issue::new(
                                Severity::Info,
                                IssueCode::DanglingReference,
                                format!(
                                    "requirements alternative is never met (the other alternatives still apply), references unknown element {id}"
                                ),
                            )
                            .at_path(path),
                        );
                    } else {
                        self.check_ref(id, path, "requirements");
                    }
                }
            }
            Ok(None) => {}
            Err(e) => self.push(
                Issue::warning(IssueCode::InvalidExpression, format!("requirements: {e}"))
                    .at_path(path),
            ),
        }
    }

    fn check_supports(&mut self, text: &str, path: &str, id_refs: bool) {
        match SupportsExpr::parse(text) {
            Ok(Some(expr)) => {
                if id_refs && expr.is_id_range_mode() {
                    for tag in expr.tags() {
                        if tag.starts_with("ID_") {
                            self.check_ref(tag, path, "supports");
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(e) => self.push(
                Issue::warning(IssueCode::InvalidExpression, format!("supports: {e}"))
                    .at_path(path),
            ),
        }
    }

    fn check_rule(&mut self, rule: &Rule, path: &str) {
        match rule {
            Rule::Grant(g) => {
                self.check_ref(&g.id, path, "grant");
                if let Some(r) = &g.requirements {
                    self.check_requirements(r, path);
                }
            }
            Rule::Select(s) => {
                if let Some(d) = &s.default_id {
                    self.check_ref(d, path, "select default");
                }
                if let Some(r) = &s.requirements {
                    self.check_requirements(r, path);
                }
                if let Some(sup) = &s.supports {
                    self.check_supports(sup, path, true);
                }
            }
            Rule::Stat(s) => {
                if let Some(r) = &s.requirements {
                    self.check_requirements(r, path);
                }
                if let Some(eq) = &s.equipped {
                    if let Err(e) = crate::expr::EquippedExpr::parse(eq) {
                        self.push(
                            Issue::warning(IssueCode::InvalidExpression, format!("equipped: {e}"))
                                .at_path(path),
                        );
                    }
                }
            }
            Rule::Custom(c) => {
                for field in CUSTOM_SPELL_REFS
                    .iter()
                    .filter(|(kind, _)| *kind == c.kind)
                    .map(|(_, field)| *field)
                {
                    if let Some(id) = c.fields.get(field).and_then(serde_json::Value::as_str) {
                        self.check_ref(id, path, &c.kind);
                    }
                }
            }
            Rule::Unknown(_) => {}
        }
    }

    fn check_rules<'r>(&mut self, rules: impl Iterator<Item = &'r Rule>, base: &str) {
        // Choices are keyed by rule index; an imported Aurora save finds its select by name, type,
        // level and a checksum over the supports, and fills selects that agree on all of those in
        // order. Only such indistinguishable selects are noted (Info): Aurora's own Arcane
        // Trickster repeats a name with different supports.
        let mut select_names = BTreeSet::new();
        for (i, rule) in rules.enumerate() {
            let path = format!("{base}[{i}]");
            if let Rule::Select(s) = rule {
                let key = (
                    s.name.clone(),
                    s.ty.clone(),
                    s.level.unwrap_or(1),
                    s.supports.clone().unwrap_or_default(),
                );
                if !select_names.insert(key) {
                    self.push(
                        Issue::new(
                            Severity::Info,
                            IssueCode::DuplicateSelectName,
                            format!(
                                "select name {:?} is used more than once at level {} with the same type and supports; the choices fill these selects in order",
                                s.name,
                                s.level.unwrap_or(1)
                            ),
                        )
                        .at_path(path.clone()),
                    );
                }
            }
            self.check_rule(rule, &path);
        }
    }

    fn check_id(&mut self, e: &Element) {
        if e.id.is_empty() {
            self.push(Issue::error(IssueCode::InvalidId, "element id is empty"));
        } else if e.id.contains(ID_FORBIDDEN) {
            self.push(Issue::error(
                IssueCode::InvalidId,
                "id contains characters that are operators in the expression languages",
            ));
        } else if !e.id.starts_with("ID_")
            || !e
                .id
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            self.push(Issue::warning(
                IssueCode::NonStandardId,
                "id does not follow the ID_UPPER_CASE convention",
            ));
        }
        if e.ty.is_empty() {
            self.push(Issue::error(IssueCode::InvalidId, "element type is empty"));
        }
    }

    /// Monsters carrying a typed stat block in `data` are checked by the monster model instead of
    /// the legacy setters. Returns true when it handled the element.
    fn check_monster_data(&mut self, e: &Element) -> bool {
        if e.ty != "Monster" || e.data.is_empty() {
            return false;
        }
        match MonsterData::from_element(e) {
            Err(err) => self.push(Issue::error(IssueCode::InvalidMonsterData, err.to_string())),
            Ok(m) => {
                for i in m.validate() {
                    self.push(
                        Issue::error(IssueCode::InvalidMonsterData, i.message.clone())
                            .at_path(format!("data.{}", i.field)),
                    );
                }
                for i in m.warnings() {
                    self.push(
                        Issue::warning(IssueCode::NonStandardMonsterXp, i.message.clone())
                            .at_path(format!("data.{}", i.field)),
                    );
                }
            }
        }
        true
    }

    fn check_setters(&mut self, e: &Element) {
        if self.check_monster_data(e) {
            return;
        }
        for name in required_setters(&e.ty) {
            if setter::find(&e.setters, name).is_none() {
                self.push(Issue::error(
                    IssueCode::MissingRequiredSetter,
                    format!("{} element requires setter {name:?}", e.ty),
                ));
            }
        }
        if e.ty == "Race" {
            for (i, s) in e.setters.iter().enumerate() {
                if s.name.eq_ignore_ascii_case("names") && s.attribute("type").is_none() {
                    self.push(
                        Issue::error(
                            IssueCode::MissingRequiredSetter,
                            "Race `names` setter requires a `type` attribute",
                        )
                        .at_path(format!("setters[{i}]")),
                    );
                }
            }
        }
        if e.ty.contains("Item") || e.ty == "Weapon" || e.ty == "Armor" {
            for (i, s) in e.setters.iter().enumerate() {
                if s.name.eq_ignore_ascii_case("proficiency") && s.value.starts_with("ID_") {
                    self.check_ref(&s.value, &format!("setters[{i}]"), "proficiency setter");
                }
            }
        }
    }

    fn check_element(&mut self, e: &Element) {
        self.check_id(e);
        self.check_setters(e);
        if !e.requirements.is_empty() {
            self.check_requirements(&e.requirements, "requirements");
        }
        self.check_rules(e.rules.iter(), "rules");

        if let Some(m) = &e.multiclass {
            if m.id == e.id {
                self.push(Issue::error(
                    IssueCode::MulticlassIdSameAsClass,
                    "multiclass id must differ from the class id (rename _CLASS_ to _MULTICLASS_)",
                ));
            } else if self.store.get(&m.id).is_some() {
                self.push(Issue::error(
                    IssueCode::DuplicateId,
                    format!("multiclass id {} collides with an element id", m.id),
                ));
            }
            if !m.requirements.is_empty() {
                self.check_requirements(&m.requirements, "multiclass.requirements");
            }
            self.check_rules(m.rules.iter(), "multiclass.rules");
        }

        if let Some(sc) = &e.spellcasting {
            if let Some(list) = &sc.list {
                self.check_supports(&list.supports, "spellcasting.list", false);
            }
            for (i, ext) in sc.extends.iter().enumerate() {
                let path = format!("spellcasting.extends[{i}]");
                if ext.is_spell_id() {
                    self.check_ref(ext.supports.trim(), &path, "spellcasting extend");
                } else {
                    self.check_supports(&ext.supports, &path, false);
                }
            }
        }

        if let Some(x) = &e.extract {
            for (i, item) in x.items.iter().enumerate() {
                self.check_ref(&item.id, &format!("extract.items[{i}]"), "extract item");
            }
        }
    }
}

/// Validate every element of the store and report problems with context.
///
/// Includes the store's own load diagnostics (duplicates, failed appends) first.
#[must_use]
pub fn validate_store(store: &ContentStore) -> Vec<Issue> {
    let mut issues: Vec<Issue> = store.diagnostics().to_vec();
    let mut seen_ids: HashSet<&str> = HashSet::new();
    let books = book_abbreviations(store);
    for stored in store.iter() {
        if !seen_ids.insert(stored.element.id.as_str()) {
            continue;
        }
        let mut ctx = Ctx {
            store,
            stored,
            books: &books,
            issues: Vec::new(),
        };
        ctx.check_element(&stored.element);
        issues.extend(ctx.issues);
    }
    for name in store.undefined_sources() {
        issues.push(Issue::info(
            IssueCode::UndefinedSource,
            format!("no Source element defines {name:?}"),
        ));
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::{ContentFile, PackOrigin};
    use crate::issue::Severity;
    use crate::rules::{GrantRule, SelectRule, StatRule};
    use crate::setter::Setter;
    use crate::store::ContentStoreBuilder;

    fn store(elements: Vec<Element>) -> ContentStore {
        ContentStoreBuilder::new()
            .add_file("t.json", PackOrigin::Custom, ContentFile::new(elements))
            .build()
    }

    fn codes(issues: &[Issue]) -> Vec<IssueCode> {
        issues.iter().map(|i| i.code).collect()
    }

    #[test]
    fn dangling_grant_default_and_requirement_are_reported_with_context() {
        let mut e = Element::new("ID_A", "A", "Race", "Core");
        e.requirements = "ID_MISSING_ONE".into();
        e.rules = vec![
            Rule::Grant(GrantRule {
                id: "ID_NOPE".into(),
                ..GrantRule::default()
            }),
            Rule::Select(SelectRule {
                ty: "T".into(),
                name: "n".into(),
                default_id: Some("ID_NOPE2".into()),
                supports: Some("ID_AA|ID_BB".into()),
                ..SelectRule::default()
            }),
            Rule::Stat(StatRule {
                name: "x".into(),
                value: "1".into(),
                requirements: Some("![str:13]&&ID_GONE".into()),
                ..StatRule::default()
            }),
        ];
        let issues = store(vec![e]).validate();
        let dangling: Vec<_> = issues
            .iter()
            .filter(|i| i.code == IssueCode::DanglingReference)
            .collect();
        assert_eq!(dangling.len(), 6, "{issues:#?}");
        assert!(dangling
            .iter()
            .all(|i| i.file.as_deref() == Some("t.json") && i.element.as_deref() == Some("ID_A")));
        assert!(dangling
            .iter()
            .any(|i| i.path.as_deref() == Some("rules[2]")));
        assert!(dangling.iter().all(|i| i.severity == Severity::Warning));
    }

    #[test]
    fn custom_spell_grants_to_unknown_spells_are_dangling() {
        let custom = |kind: &str, spell: &str| {
            Rule::Custom(crate::rules::CustomRule {
                kind: kind.into(),
                fields: serde_json::json!({"spell": spell, "mode": "known_cantrip"})
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            })
        };
        let spell = Element::new("ID_S", "S", "Spell", "Core");
        let mut e = Element::new("ID_A", "A", "Magic Item", "Core");
        e.rules = vec![
            custom("grant_spell", "ID_S"),
            custom("grant_spell", "ID_GUESSED_SPELL_X"),
            custom("spell_cast_alt_cost", "ID_GUESSED_SPELL_Y"),
            custom("roll_advantage", "ID_NOT_A_REFERENCE"),
        ];
        let issues = store(vec![spell, e]).validate();
        let dangling: Vec<&str> = issues
            .iter()
            .filter(|i| i.code == IssueCode::DanglingReference)
            .map(|i| i.message.as_str())
            .collect();
        assert_eq!(dangling.len(), 2, "{issues:#?}");
        assert!(dangling[0].contains("ID_GUESSED_SPELL_X"));
        assert!(dangling[1].starts_with("spell_cast_alt_cost"));
    }

    #[test]
    fn resolved_references_multiclass_ids_and_synthesized_ids_pass() {
        let mut class = Element::new("ID_C", "C", "Class", "Core");
        class.setters.push(Setter::new("hd", "d8"));
        class.multiclass = Some(crate::element::Multiclass {
            id: "ID_MC".into(),
            ..Default::default()
        });
        let mut other = Element::new("ID_O", "O", "Race", "Core");
        other.rules = vec![
            Rule::Grant(GrantRule {
                id: "ID_C".into(),
                ..GrantRule::default()
            }),
            Rule::Grant(GrantRule {
                id: "ID_INTERNAL_CLASS_FEATURE_ASI_4_FIGHTER".into(),
                ..GrantRule::default()
            }),
            Rule::Stat(StatRule {
                name: "x".into(),
                value: "1".into(),
                requirements: Some("!ID_MC".into()),
                ..StatRule::default()
            }),
        ];
        let issues = store(vec![class, other]).validate();
        assert!(
            !codes(&issues).contains(&IssueCode::DanglingReference),
            "{issues:#?}"
        );
    }

    #[test]
    fn required_setters_ids_and_multiclass_rules() {
        let mut class = Element::new("ID_C", "C", "Class", "Core");
        class.multiclass = Some(crate::element::Multiclass {
            id: "ID_C".into(),
            ..Default::default()
        });
        let bad_id = Element::new("ID_A(B)", "x", "Item", "Core");
        let lower = Element::new("ID_abc", "x", "Item", "Core");
        let mut race = Element::new("ID_R", "R", "Race", "Core");
        race.setters.push(Setter::new("names", "a, b"));
        let level = Element::new("ID_L", "L", "Level", "Core");
        let issues = store(vec![class, bad_id, lower, race, level]).validate();
        let c = codes(&issues);
        assert!(c.contains(&IssueCode::MulticlassIdSameAsClass));
        assert!(c.contains(&IssueCode::InvalidId));
        assert!(c.contains(&IssueCode::NonStandardId));
        assert_eq!(
            c.iter()
                .filter(|c| **c == IssueCode::MissingRequiredSetter)
                .count(),
            1 + 1 + 2
        );
    }

    #[test]
    fn invalid_expressions_and_duplicate_select_names() {
        let mut e = Element::new("ID_A", "A", "Feat", "Core");
        e.requirements = "Strength 13 or higher".into();
        let sel = |supports: &str| {
            Rule::Select(SelectRule {
                ty: "T".into(),
                name: "same".into(),
                supports: Some(supports.into()),
                ..SelectRule::default()
            })
        };
        e.rules = vec![sel("Smith’s Tools"), sel("Ok"), sel("Ok")];
        let issues = store(vec![e]).validate();
        let c = codes(&issues);
        assert_eq!(
            c.iter()
                .filter(|c| **c == IssueCode::InvalidExpression)
                .count(),
            2
        );
        // only the two selects nothing tells apart (same name, type, level and supports), as a
        // note: the importer fills them in order
        let dup: Vec<_> = issues
            .iter()
            .filter(|i| i.code == IssueCode::DuplicateSelectName)
            .collect();
        assert_eq!(dup.len(), 1, "{issues:#?}");
        assert_eq!(dup[0].severity, Severity::Info);
    }

    #[test]
    fn same_named_selects_told_apart_by_supports_are_not_flagged() {
        // Aurora's own Arcane Trickster: two "Spell (Arcane Trickster)" selects at level 3, one
        // limited to enchantment/illusion. Choices are keyed by rule index and an Aurora save is
        // matched by its select checksum, which includes the supports.
        let mut e = Element::new("ID_A", "A", "Archetype", "Core");
        let sel = |supports: &str, number: u32| {
            Rule::Select(SelectRule {
                ty: "Spell".into(),
                name: "Spell (Arcane Trickster)".into(),
                supports: Some(supports.into()),
                level: Some(3),
                number: Some(number),
                ..SelectRule::default()
            })
        };
        e.rules = vec![sel("Wizard, 1", 1), sel("Wizard, 1, Enchantment", 2)];
        let issues = store(vec![e]).validate();
        assert!(
            !codes(&issues).contains(&IssueCode::DuplicateSelectName),
            "{issues:#?}"
        );
    }

    #[test]
    fn an_unknown_alternative_beside_a_known_one_is_info_not_warning() {
        // a class that cannot multiclass (its multiclass block is commented out) still names
        // the multiclass id as an alternative: the requirement holds through the class
        let class = Element::new("ID_C_CLASS_X", "X", "Class", "Core");
        let mut e = Element::new("ID_C_DISCIPLINE", "D", "Feat", "Core");
        e.requirements = "ID_C_CLASS_X||ID_C_MULTICLASS_X".into();
        let mut lone = Element::new("ID_C_LONE", "L", "Feat", "Core");
        lone.requirements = "ID_GONE_A||ID_GONE_B".into();
        let issues = store(vec![class, e, lone]).validate();
        let dangling = |el: &str| -> Vec<Severity> {
            issues
                .iter()
                .filter(|i| {
                    i.code == IssueCode::DanglingReference && i.element.as_deref() == Some(el)
                })
                .map(|i| i.severity)
                .collect()
        };
        assert_eq!(
            dangling("ID_C_DISCIPLINE"),
            vec![Severity::Info],
            "{issues:#?}"
        );
        let alt = issues
            .iter()
            .find(|i| {
                i.code == IssueCode::DanglingReference
                    && i.element.as_deref() == Some("ID_C_DISCIPLINE")
            })
            .expect("issue");
        // the preview reads the unresolved target from the message's last word
        assert!(
            alt.message.ends_with(" ID_C_MULTICLASS_X"),
            "{}",
            alt.message
        );
        // no alternative resolves: still a warning for each
        assert_eq!(
            dangling("ID_C_LONE"),
            vec![Severity::Warning, Severity::Warning]
        );
    }

    #[test]
    fn an_unknown_id_a_requirement_only_tests_the_absence_of_is_info() {
        // issue 8: AuroraLegacy's races read `!(ID_WOTC_WGTE_GRANTS_DARKMARKED||...)`; without the
        // Wildemount content the test is always true, so nothing is missing
        let mut e = Element::new("ID_R", "R", "Race", "Core");
        e.requirements = "!ID_GONE_NOT".into();
        e.rules = vec![Rule::Stat(crate::rules::StatRule {
            name: "strength".into(),
            value: "2".into(),
            requirements: Some("!(ID_GONE_GROUP||ID_GONE_TWICE),!!ID_GONE_POSITIVE".into()),
            ..Default::default()
        })];
        let mut mixed = Element::new("ID_M", "M", "Feat", "Core");
        mixed.requirements = "!ID_GONE_BOTH||ID_GONE_BOTH".into();
        let issues = store(vec![e, mixed]).validate();
        let severity_of = |id: &str| -> Vec<Severity> {
            issues
                .iter()
                .filter(|i| {
                    i.code == IssueCode::DanglingReference && i.message.ends_with(&format!(" {id}"))
                })
                .map(|i| i.severity)
                .collect()
        };
        for id in ["ID_GONE_NOT", "ID_GONE_GROUP", "ID_GONE_TWICE"] {
            assert_eq!(severity_of(id), vec![Severity::Info], "{id}: {issues:#?}");
        }
        // two negations make a positive test, and so does a positive use beside a negated one
        assert_eq!(severity_of("ID_GONE_POSITIVE"), vec![Severity::Warning]);
        let both = severity_of("ID_GONE_BOTH");
        assert!(
            !both.is_empty() && both.iter().all(|s| *s == Severity::Warning),
            "{both:?}"
        );
        let note = issues
            .iter()
            .find(|i| i.message.ends_with(" ID_GONE_NOT"))
            .expect("issue");
        assert!(
            note.message.contains("this absence test always passes"),
            "{}",
            note.message
        );
    }

    #[test]
    fn an_absence_test_of_an_id_naming_a_loaded_book_stays_a_warning() {
        // review M2: `!ID_WOCT_PSA_...` (WOCT for WOTC) names the loaded Plane Shift: Amonkhet
        // (abbreviation PSA) but no element: a misspelling that makes the test always pass,
        // so the element it meant to exclude is offered; not a harmless condition
        let mut book = Element::new("ID_SRC_PSA", "Plane Shift: Amonkhet", "Source", "Core");
        book.setters
            .push(crate::setter::Setter::new("abbreviation", "PSA"));
        let mut e = Element::new(
            "ID_WOTC_PSA_BACKGROUND_X",
            "X",
            "Background",
            "Plane Shift: Amonkhet",
        );
        e.requirements = "!ID_WOCT_PSA_BACKGROUND_FEATURE_TYPO".into();
        let issues = store(vec![book, e]).validate();
        let dangling: Vec<_> = issues
            .iter()
            .filter(|i| i.code == IssueCode::DanglingReference)
            .map(|i| i.severity)
            .collect();
        assert_eq!(dangling, vec![Severity::Warning], "{issues:#?}");
    }

    #[test]
    fn spellcasting_extract_and_undefined_sources() {
        let mut e = Element::new("ID_A", "A", "Item", "Mystery Book");
        e.spellcasting = Some(crate::spellcasting::Spellcasting {
            extends: vec![crate::spellcasting::SpellcastingList::new("ID_PHB_SPELL_X")],
            ..Default::default()
        });
        e.extract = Some(crate::element::Extract {
            items: vec![crate::element::ExtractItem {
                id: "ID_NO_ITEM".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        let issues = store(vec![e]).validate();
        assert_eq!(
            issues
                .iter()
                .filter(|i| i.code == IssueCode::DanglingReference)
                .count(),
            2
        );
        assert!(issues.iter().any(|i| i.code == IssueCode::UndefinedSource));
    }

    #[test]
    fn synthesized_id_shapes() {
        assert!(is_synthesized_id(
            "ID_INTERNAL_MAGIC_ITEM_SPELL_SCROLL_FIREBALL"
        ));
        assert!(is_synthesized_id("ID_WOTC_PHB_RACIAL_TRAIT_X_IGNORE"));
        assert!(is_synthesized_id("ID_PHB_INTERNAL_ITEM_FEAT_PROXY_X"));
        assert!(!is_synthesized_id("ID_PHB_SPELL_FIREBALL"));
    }
}
