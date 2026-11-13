//! The generic "Other element" form (spec 4.12): the types it offers and the setter names those types use, read from
//! loaded content through the form context's [`Lookup`]. Without loaded elements (a lookup of names only) the lists
//! hold the fixed open types and no setter names: the Type combo and the setter names then take free text.

use super::kinds::{kind, kind_for_type, KindGroup, OTHER_KIND};
use super::{FormState, Lookup};
use borealis_content::validate::required_setters;
use std::collections::BTreeMap;

/// The picker type (`homebrew_reference_options`) whose rows are the open element types of loaded content: one row
/// per type, its name as id and name. Not an element type.
pub const ELEMENT_TYPE_PICKER: &str = "Element Type";

/// The picker type whose rows are the setter names elements of a type use: the target's other `types` name the
/// element type (`["Setter Name", "Companion"]`). Not an element type.
pub const SETTER_NAME_PICKER: &str = "Setter Name";

/// The picker type whose rows are the categories loaded items use (`Adventuring Gear`, `Wondrous Items`): the item
/// forms' Category suggestions (spec 4.9). Not an element type.
pub const ITEM_CATEGORY_PICKER: &str = "Item Category";

/// The picker type whose rows are the tags loaded elements of a type carry: the target's other `types` name the
/// element types (`["Tag", "Feat"]`), the shared Tags field's suggestions (spec 3.2). Not an element type.
pub const TAG_PICKER: &str = "Tag";

/// Words in their first spelling, each once (any case), sorted case-insensitively.
fn first_spellings(words: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for w in words {
        let w = w.trim().to_string();
        if !w.is_empty() {
            out.entry(w.to_lowercase()).or_insert(w);
        }
    }
    out.into_values().collect()
}

/// The `category` setter values of loaded elements of the types the item forms write (spec 4.9).
#[must_use]
pub fn item_categories(lookup: &dyn Lookup) -> Vec<String> {
    let types: Vec<&str> = super::item_kinds::item_kinds()
        .iter()
        .filter(|k| {
            k.fields
                .iter()
                .any(|f| f.codec == super::codec::Codec::SetterText("category"))
        })
        .flat_map(|k| k.element_types.iter().copied())
        .collect();
    let mut seen: Vec<&str> = Vec::new();
    let values = types
        .into_iter()
        .filter(|t| {
            let new = !seen.contains(t);
            seen.push(t);
            new
        })
        .flat_map(|t| lookup.loaded_of_type(t))
        .filter_map(|e| e.setter_value("category").map(str::to_string));
    first_spellings(values)
}

/// The tags loaded elements of type `ty` carry (their `supports`, no element ids).
#[must_use]
pub fn loaded_tags(lookup: &dyn Lookup, ty: &str) -> Vec<String> {
    let tags = lookup
        .loaded_of_type(ty)
        .into_iter()
        .flat_map(|e| e.supports.iter())
        .filter(|t| !t.starts_with("ID_"))
        .cloned();
    first_spellings(tags)
}

/// The open types the generic form writes (spec 3.1), offered even when no loaded content uses them.
pub const OPEN_TYPES: &[(&str, &str)] = &[
    ("Companion", "Companion"),
    ("Companion Action", "Companion Action"),
    ("Companion Feature", "Companion Feature"),
    ("Companion Reaction", "Companion Reaction"),
    ("Companion Trait", "Companion Trait"),
    ("Familiar", "Familiar"),
    ("Familiar Action", "Familiar Action"),
    ("Familiar Feature", "Familiar Feature"),
    ("Familiar Trait", "Familiar Trait"),
    ("Property", "Property"),
];

/// Types the creator manages itself (spec 3.1): one `Source` per pack, written from the pack settings, and the
/// `Multiclass` section a class writes.
const MANAGED: &[&str] = &["Source", "Multiclass"];

/// Engine and core plumbing (spec 3.1 "Not authored"): loaded elements of these types still open in the generic
/// form, but it does not offer them for new elements.
const NOT_AUTHORED: &[&str] = &[
    "Core",
    "Internal",
    "Ignore",
    "Level",
    "Support",
    "Grants",
    "Ability Score Improvement",
    "Monster Action",
    "Monster Trait",
];

/// The type has a form of its own (a spell, a language...).
fn has_own_form(ty: &str) -> bool {
    kind_for_type(ty).id != OTHER_KIND
}

/// The generic form offers `ty` for a new element.
fn offered(ty: &str) -> bool {
    !ty.trim().is_empty()
        && !has_own_form(ty)
        && !MANAGED.contains(&ty)
        && !NOT_AUTHORED.contains(&ty)
}

/// The types the Type combo offers: the fixed open types, then every other open type loaded content uses, sorted.
#[must_use]
pub fn open_types(lookup: &dyn Lookup) -> Vec<String> {
    let mut out: Vec<String> = OPEN_TYPES.iter().map(|(t, _)| (*t).to_string()).collect();
    let mut found: Vec<String> = lookup
        .loaded_types()
        .into_iter()
        .filter(|t| offered(t) && !out.contains(t))
        .collect();
    found.sort();
    found.dedup();
    out.extend(found);
    out
}

/// The setter names elements of type `ty` use: the ones the content format requires of the type (a Companion's
/// abilities and size) and the ones loaded elements of the type have, in their first spelling, sorted
/// case-insensitively.
#[must_use]
pub fn setter_names(lookup: &dyn Lookup, ty: &str) -> Vec<String> {
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let loaded = lookup
        .loaded_of_type(ty)
        .into_iter()
        .flat_map(|e| e.setters.iter().map(|s| s.name.clone()));
    let required = required_setters(ty).iter().map(|s| (*s).to_string());
    for name in required.chain(loaded) {
        names.entry(name.to_lowercase()).or_insert(name);
    }
    names.into_values().collect()
}

/// Why the generic form can't write the type its form names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRefusal {
    /// The type as written.
    pub ty: String,
    /// Where elements of the type are made ("New element > Spell", "the Feat form, inside it"), or `None` for a
    /// type the pack manages itself (Source, Multiclass).
    pub made_with: Option<String>,
}

/// Where elements of a type with a form of its own are made: New element, or inside a parent's form.
fn made_with(ty: &str) -> String {
    let k = kind_for_type(ty);
    match k.parent_kinds.first().and_then(|p| kind(p)) {
        Some(parent) if k.group == KindGroup::Child => {
            format!("the {} form, inside it", parent.label)
        }
        _ => format!("New element > {}", k.label),
    }
}

/// The refusal for a new generic form, or one whose type was changed, that names a type it can't write: one with a
/// form of its own, or one the pack manages. A stored element keeps its type while the field is unchanged (a
/// house-rule `Option`, a pack's `Source`). Live checks only: they see the form as read, with its memo.
#[must_use]
pub fn type_refusal(form: &FormState) -> Option<TypeRefusal> {
    if form.kind != OTHER_KIND {
        return None;
    }
    let value = form.fields.get("type")?;
    if form
        .memo
        .fields
        .get("type")
        .is_some_and(|m| m.value == *value)
    {
        return None;
    }
    let ty = value.as_str().map(str::trim).unwrap_or_default();
    let made_with = if MANAGED.contains(&ty) {
        None
    } else if has_own_form(ty) {
        Some(made_with(ty))
    } else {
        return None;
    };
    Some(TypeRefusal {
        ty: ty.to_string(),
        made_with,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use borealis_content::Element;

    struct Loaded(Vec<Element>);

    impl Lookup for Loaded {
        fn find(&self, _id: &str) -> Option<super::super::RefInfo> {
            None
        }
        fn loaded_of_type(&self, ty: &str) -> Vec<&Element> {
            self.0.iter().filter(|e| e.ty == ty).collect()
        }
        fn loaded_types(&self) -> Vec<String> {
            self.0.iter().map(|e| e.ty.clone()).collect()
        }
    }

    fn el(ty: &str, setters: &[(&str, &str)]) -> Element {
        let mut e = Element::new(format!("ID_{}", ty.len()), "X", ty, "S");
        e.setters = setters
            .iter()
            .map(|(n, v)| borealis_content::Setter::new(*n, *v))
            .collect();
        e
    }

    #[test]
    fn open_types_are_the_fixed_ones_then_loaded_types_without_a_form() {
        let loaded = Loaded(vec![
            el("Spell", &[]),
            el("Vehicle", &[("speed", "30")]),
            el("Support", &[]),
            el("Source", &[]),
            el("Monster", &[]),
            el("Companion", &[]),
            el("Vehicle", &[("Speed", "40"), ("crew", "4")]),
        ]);
        let types = open_types(&loaded);
        assert_eq!(types.len(), OPEN_TYPES.len() + 1);
        assert_eq!(types.last().map(String::as_str), Some("Vehicle"));
        assert_eq!(types[0], "Companion");
        assert_eq!(setter_names(&loaded, "Vehicle"), ["crew", "speed"]);
        // a lookup of names only: the fixed types, no setter names
        assert_eq!(open_types(&super::super::NoLookup).len(), OPEN_TYPES.len());
        assert!(setter_names(&super::super::NoLookup, "Vehicle").is_empty());
    }

    #[test]
    fn item_categories_and_tags_are_read_from_loaded_content() {
        let mut feat = el("Feat", &[]);
        feat.supports = vec!["Origin Feat".into(), "ID_X_SUPPORT".into(), "Feat".into()];
        let mut other = el("Feat", &[]);
        other.supports = vec!["origin feat".into(), "General Feat".into()];
        let mut spell = el("Spell", &[]);
        spell.supports = vec!["Wizard".into()];
        let loaded = Loaded(vec![
            el("Item", &[("category", "Adventuring Gear")]),
            el("Weapon", &[("category", "Weapons")]),
            el("Magic Item", &[("category", "Wondrous Items")]),
            el("Item", &[("category", "adventuring gear")]),
            el("Feat", &[("category", "Origin")]),
            feat,
            other,
            spell,
        ]);
        assert_eq!(
            item_categories(&loaded),
            ["Adventuring Gear", "Weapons", "Wondrous Items"],
            "the item kinds' categories, first spelling, sorted; a feat's category is not an item's"
        );
        assert_eq!(
            loaded_tags(&loaded, "Feat"),
            ["Feat", "General Feat", "Origin Feat"],
            "a type's tags, first spelling, sorted; ids are not tags"
        );
        assert!(loaded_tags(&super::super::NoLookup, "Feat").is_empty());
    }

    #[test]
    fn an_edited_type_with_a_form_of_its_own_or_managed_is_refused() {
        let form = |ty: &str| FormState::new("e", OTHER_KIND).with_field("type", ty.into());
        let r = type_refusal(&form("Spell")).expect("a spell has its own form");
        assert_eq!(r.made_with.as_deref(), Some("New element > Spell"));
        let child = type_refusal(&form("Feat Feature")).expect("a feat feature has its own form");
        assert_eq!(child.made_with.as_deref(), Some("the Feat form, inside it"));
        assert_eq!(
            type_refusal(&form("Source")).map(|r| r.made_with),
            Some(None)
        );
        assert!(type_refusal(&form("Companion")).is_none());
        assert!(type_refusal(&form("Vehicle")).is_none());
        // a stored element of a type with a form (a house-rule Option) keeps its type while it is unchanged
        let mut stored = form("Option");
        stored.memo.fields.insert(
            "type".into(),
            super::super::FieldMemo {
                value: "Option".into(),
                raw: "Option".into(),
                slots: super::super::Slots::default(),
            },
        );
        assert!(type_refusal(&stored).is_none());
        assert!(type_refusal(&stored.with_field("type", "Spell".into())).is_some());
    }

    #[test]
    fn a_type_s_required_setters_are_offered_too() {
        let names = setter_names(&super::super::NoLookup, "Companion");
        assert!(names.iter().any(|n| n == "challenge"), "{names:?}");
        assert!(setter_names(&super::super::NoLookup, "Property").is_empty());
    }
}
