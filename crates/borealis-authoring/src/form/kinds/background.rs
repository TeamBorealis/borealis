//! The background form (spec 4.3), one compact form per edition: the fields of the other edition are hidden
//! (`FieldDef::editions`), so a 2024 background shows its ability scores, origin feat, two skills, tool and starting
//! equipment, and a 2014 one its proficiencies, language and tool choices, feature, characteristics, equipment and
//! variants. Its children: the Background Feature (granted), the variants (offered by an optional choice) and the
//! starting-equipment package, each edited inside it. The SRD stores the characteristics as `List` choices on the
//! background, so they are fields here; a stored Background Characteristics element has its own small form.

use super::super::codec::{BackgroundField, Characteristic, Codec, ABILITY_CHOICES};
use super::super::FormState;
use super::{
    field, head, tail, Control, FieldDef, KindDef, KindGroup, BOTH, DESCRIPTION, GENERAL,
    ONLY_2024, RULES, TABS,
};
use borealis_content::Ruleset;
use serde_json::json;

/// The background form's id.
pub const BACKGROUND_KIND: &str = "background";
const FEATURE_KIND: &str = "backgroundFeature";
const VARIANT_KIND: &str = "backgroundVariant";
const CHARACTERISTICS_KIND: &str = "backgroundCharacteristics";

const CHARACTERISTICS: &str = "characteristics";
const EQUIPMENT: &str = "equipment";
const TABS_OF_BACKGROUND: &[(&str, &str)] = &[
    (GENERAL, "GENERAL"),
    (CHARACTERISTICS, "CHARACTERISTICS"),
    (EQUIPMENT, "EQUIPMENT"),
    (RULES, "RULES"),
    (DESCRIPTION, "DESCRIPTION"),
];
const ONLY_2014: &[Ruleset] = &[Ruleset::Dnd2014];

/// Shared fields a background doesn't use (a character has one background).
const NOT_FOR_BACKGROUNDS: &[&str] = &["allowDuplicate"];

fn bg(f: BackgroundField) -> Codec {
    Codec::Background(f)
}

fn own_fields() -> Vec<FieldDef> {
    vec![
        field(
            "abilityScores",
            "Ability scores",
            GENERAL,
            Control::Chips,
            bg(BackgroundField::AbilityScores),
        )
        .choices(ABILITY_CHOICES, false)
        .editions(ONLY_2024)
        .section("Ability scores")
        .default("[]")
        .help("Pick three. The character adds 2 to one and 1 to another, or 1 to each (up to 20)."),
        field(
            "originFeat",
            "Origin feat",
            GENERAL,
            Control::FeatGrant,
            bg(BackgroundField::OriginFeat),
        )
        .picker(&["Feat"])
        .picker_tags(&["Origin Feat"])
        .editions(ONLY_2024)
        .section("Origin feat")
        .default("{\"feat\":null,\"params\":{}}")
        .help("A choice of the feat the background makes (Magic Initiate's spell list); the others stay the character's."),
        field(
            "skills",
            "Skill proficiencies",
            GENERAL,
            Control::References,
            bg(BackgroundField::Skills),
        )
        .picker(&["Proficiency"])
        .picker_tags(&["Skill"])
        .section("Proficiencies")
        .default("[]"),
        field(
            "tools",
            "Tool proficiencies",
            GENERAL,
            Control::References,
            bg(BackgroundField::Tools),
        )
        .picker(&["Proficiency"])
        .section("Proficiencies")
        .default("[]")
        .help("A 2024 background gives one tool, or a choice of one (below)."),
        field(
            "languages",
            "Languages",
            GENERAL,
            Control::References,
            bg(BackgroundField::Languages),
        )
        .picker(&["Language"])
        .editions(ONLY_2014)
        .section("Proficiencies")
        .default("[]"),
        field(
            "choices",
            "Language and tool choices",
            GENERAL,
            Control::Choices,
            bg(BackgroundField::Choices),
        )
        .section("Proficiencies")
        .default("[]")
        .help("Languages or tools the character chooses, from the ones with these tags."),
        field(
            "feature",
            "Background feature",
            GENERAL,
            Control::Children,
            Codec::Children {
                ty: "Background Feature",
                levelled: false,
            },
        )
        .picker(&["Background Feature"])
        .editions(ONLY_2014)
        .section("Feature")
        .default("[]")
        .help("The background gives each one; make it here."),
        field(
            "variants",
            "Variants",
            GENERAL,
            Control::Children,
            bg(BackgroundField::Variants),
        )
        .picker(&["Background Variant"])
        .editions(ONLY_2014)
        .section("Feature")
        .default("[]")
        .help("Versions of the background a character may take instead (each with its own rules)."),
    ]
}

fn characteristic(key: &'static str, label: &'static str, c: Characteristic) -> FieldDef {
    let default = if c == Characteristic::PersonalityTrait {
        "{\"count\":2,\"items\":[]}"
    } else {
        "{\"count\":1,\"items\":[]}"
    };
    field(
        key,
        label,
        CHARACTERISTICS,
        Control::WrittenList,
        bg(BackgroundField::Characteristic(c)),
    )
    .default(default)
}

fn characteristics(tab: &'static str, editions: &'static [Ruleset]) -> Vec<FieldDef> {
    [
        characteristic(
            "personalityTraits",
            "Personality traits",
            Characteristic::PersonalityTrait,
        )
        .help("The character picks this many; numbered 1, 2, 3... in order."),
        characteristic("ideals", "Ideals", Characteristic::Ideal)
            .help("The character picks one; numbered 1, 2, 3... in order."),
        characteristic("bonds", "Bonds", Characteristic::Bond)
            .help("The character picks one; numbered 1, 2, 3... in order."),
        characteristic("flaws", "Flaws", Characteristic::Flaw)
            .help("The character picks one; numbered 1, 2, 3... in order."),
    ]
    .map(|f| {
        FieldDef { tab, ..f }
            .editions(editions)
            .section("Suggested characteristics")
    })
    .to_vec()
}

fn equipment() -> Vec<FieldDef> {
    vec![
        field(
            "equipment",
            "Starting equipment",
            EQUIPMENT,
            Control::Package,
            bg(BackgroundField::Equipment),
        )
        .picker(&["Equipment Package"])
        .default("[]")
        .help("The lettered options a character with this background chooses from."),
        field(
            "equipmentNote",
            "Equipment as shown",
            EQUIPMENT,
            Control::RichText,
            bg(BackgroundField::EquipmentNote),
        )
        .editions(ONLY_2014)
        .help("A note of the equipment, shown with the background."),
    ]
}

/// The background form.
fn background_kind() -> KindDef {
    let shared = tail()
        .into_iter()
        .filter(|f| !NOT_FOR_BACKGROUNDS.contains(&f.key))
        .map(|f| match f.tab {
            GENERAL => f.section("Details"),
            RULES => FieldDef {
                label: "Other rules",
                help: Some("Rules of the background that no field above holds."),
                ..f
            },
            _ => f,
        });
    KindDef {
        id: BACKGROUND_KIND,
        label: "Background",
        plural: "Backgrounds",
        description: "A background: 2024 ability scores, origin feat, skills, tool and equipment; 2014 feature and characteristics.",
        group: KindGroup::Main,
        element_types: &["Background"],
        parent_kinds: &[],
        editions: BOTH,
        tabs: TABS_OF_BACKGROUND,
        fields: head()
            .into_iter()
            .chain(own_fields())
            .chain(characteristics(CHARACTERISTICS, ONLY_2014))
            .chain(equipment())
            .chain(shared)
            .collect(),
    }
}

/// A child kind of the background with the parts every form shares.
fn child(
    id: &'static str,
    label: &'static str,
    plural: &'static str,
    description: &'static str,
    ty: &'static [&'static str],
) -> KindDef {
    KindDef {
        id,
        label,
        plural,
        description,
        group: KindGroup::Child,
        element_types: ty,
        parent_kinds: &[BACKGROUND_KIND],
        editions: BOTH,
        tabs: TABS,
        fields: head().into_iter().chain(tail()).collect(),
    }
}

/// A background variant: the shared parts and its own feature (the 2014 shape of a variant that replaces the
/// background's feature), made inside the variant.
fn variant_kind() -> KindDef {
    let mut k = child(
        VARIANT_KIND,
        "Background variant",
        "Background variants",
        "A version of a background a character may take instead, edited inside the background.",
        &["Background Variant"],
    );
    let feature = field(
        "feature",
        "Its own feature",
        GENERAL,
        Control::Children,
        Codec::Children {
            ty: "Background Feature",
            levelled: false,
        },
    )
    .picker(&["Background Feature"])
    .default("[]")
    .help("A feature the variant gives (instead of the background's, when it replaces it); make it here.");
    k.fields = head()
        .into_iter()
        .chain(std::iter::once(feature))
        .chain(tail())
        .collect();
    k
}

/// The background kinds: the background, its feature, its variants and a stored characteristics element.
pub(super) fn background_kinds() -> Vec<KindDef> {
    let mut characteristics_kind = child(
        CHARACTERISTICS_KIND,
        "Background characteristics",
        "Background characteristics",
        "Personality traits, ideals, bonds and flaws a background offers.",
        &["Background Characteristics"],
    );
    characteristics_kind.fields = head()
        .into_iter()
        .chain(characteristics(GENERAL, &[]))
        .chain(tail())
        .collect();
    vec![
        background_kind(),
        KindDef {
            parent_kinds: &[BACKGROUND_KIND, VARIANT_KIND],
            ..child(
                FEATURE_KIND,
                "Background feature",
                "Background features",
                "The feature a background (or one of its variants) gives, edited inside it.",
                &["Background Feature"],
            )
        },
        variant_kind(),
        characteristics_kind,
    ]
}

/// A new background of `edition` (a background is of one edition: its form shows that edition's fields).
#[must_use]
pub fn new_background_form(edition: Ruleset) -> FormState {
    let mut form = background_kind().new_form("e");
    form.fields
        .insert("edition".into(), json!(edition.as_str()));
    form
}
