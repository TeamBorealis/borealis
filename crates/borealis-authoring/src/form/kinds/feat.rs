//! The feat form (spec 4.2) for 2014 and 2024 feats: the 2024 category, the prerequisite (one condition builder for
//! `requirements`, `data.prerequisite` and the display sentence), taking it again, then the parts every form shares.
//! What the feat gives (an ability score increase, proficiencies, spells, choices) is its rule blocks. The Feat
//! Feature form edits a feat's sub-features (2014 feats with options).

use super::super::codec::{Codec, FeatField};
use super::{field, head, tail, Control, FieldDef, KindDef, KindGroup, BOTH, GENERAL, TABS};

const FEAT: &str = "Feat";
const PREREQUISITE: &str = "Prerequisite";
const AGAIN: &str = "Taking it again";
const DETAILS: &str = "Details";

/// The 2024 categories (the tag a 2024 feat choice offers by is `"<Category> Feat"`).
const CATEGORIES: &[(&str, &str)] = &[
    ("Origin", "Origin"),
    ("General", "General"),
    ("Fighting Style", "Fighting Style"),
    ("Epic Boon", "Epic Boon"),
];

/// The shared fields a feat binds in its own way (spec 4.2).
const OWN_SHARED: &[&str] = &["requirements", "prerequisiteText", "allowDuplicate"];

fn feat(f: FeatField) -> Codec {
    Codec::Feat(f)
}

fn own_fields() -> Vec<FieldDef> {
    vec![
        field(
            "category",
            "Category",
            GENERAL,
            Control::Combo,
            feat(FeatField::Category),
        )
        .choices(CATEGORIES, true)
        .section(FEAT)
        .help("2024 feats: the feat choices of this category offer it. Leave it empty for a 2014 feat."),
        field(
            "requirements",
            "Prerequisite",
            GENERAL,
            Control::Condition,
            feat(FeatField::Prerequisite),
        )
        .section(PREREQUISITE),
        field(
            "prerequisiteText",
            "Prerequisite as shown",
            GENERAL,
            Control::Text,
            feat(FeatField::Shown),
        )
        .section(PREREQUISITE)
        .default("\"\"")
        .help("Written from the prerequisite while left empty or as written; type to reword it."),
        field(
            "repeatable",
            "Can be taken more than once",
            GENERAL,
            Control::Check,
            feat(FeatField::Repeatable),
        )
        .section(AGAIN)
        .default("false"),
        field(
            "repeatDistinct",
            "Each time with a different",
            GENERAL,
            Control::Text,
            feat(FeatField::RepeatDistinct),
        )
        .section(AGAIN)
        .help("The choice that must differ each time, such as Spell List. Leave it empty when any choice may repeat."),
    ]
}

/// The feat form.
pub(super) fn feat_kind() -> KindDef {
    let shared = tail()
        .into_iter()
        .filter(|f| !OWN_SHARED.contains(&f.key))
        .map(|f| {
            if f.tab == GENERAL {
                f.section(DETAILS)
            } else {
                f
            }
        });
    KindDef {
        id: "feat",
        label: "Feat",
        plural: "Feats",
        description: "A feat, 2014 or 2024, with its prerequisite and what it gives.",
        group: KindGroup::Main,
        element_types: &["Feat"],
        parent_kinds: &[],
        editions: BOTH,
        tabs: TABS,
        fields: head()
            .into_iter()
            .chain(own_fields())
            .chain(shared)
            .collect(),
    }
}

/// The Feat Feature form: a sub-feature or option of a feat, with the parts every form shares.
pub(super) fn feat_feature_kind() -> KindDef {
    KindDef {
        id: "featFeature",
        label: "Feat feature",
        plural: "Feat features",
        description: "An option or part of a feat, edited inside the feat.",
        group: KindGroup::Child,
        element_types: &["Feat Feature"],
        parent_kinds: &["feat"],
        editions: BOTH,
        tabs: TABS,
        fields: head().into_iter().chain(tail()).collect(),
    }
}
