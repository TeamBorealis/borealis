//! Element ids, type names and stat names the engine relies on (all defined by the core pack,
//! see `docs/core-pack-ids.md`).

pub const LEVEL_PREFIX: &str = "ID_LEVEL_";
pub const OPT_FEATS: &str = "ID_INTERNAL_OPTION_ALLOW_FEATS";
pub const OPT_MULTICLASSING: &str = "ID_INTERNAL_OPTION_ALLOW_MULTICLASSING";
pub const OPT_AVERAGE_HP: &str = "ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP";
pub const MULTICLASS_PREREQ: &str = "ID_INTERNAL_GRANTS_MULTICLASSING_PREREQUISITE";
pub const MULTICLASS_LEVEL_PREFIX: &str = "ID_INTERNAL_MULTICLASS_LEVEL_";
pub const GRANT_MULTICLASS: &str = "ID_INTERNAL_GRANT_MULTICLASS";
pub const MULTICLASS_SPELLCASTING: &str = "ID_INTERNAL_GRANTS_MULTICLASS_SPELLCASTING";
pub const SLOTS_PREFIX: &str = "ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_";
pub const TEMPLATE_ASI: &str = "ID_INTERNAL_TEMPLATE_CLASS_FEATURE_ABILITY_4";
pub const TEMPLATE_FEAT: &str = "ID_INTERNAL_TEMPLATE_CLASS_FEATURE_FEAT_4";
pub const COUNTS_AS_ARMOR: &str = "ID_INTERNAL_GRANTS_COUNTS_AS_EQUIPPED_ARMOR";
pub const COUNTS_AS_LIGHT: &str = "ID_INTERNAL_GRANTS_COUNTS_AS_EQUIPPED_LIGHT_ARMOR";
pub const COUNTS_AS_MEDIUM: &str = "ID_INTERNAL_GRANTS_COUNTS_AS_EQUIPPED_MEDIUM_ARMOR";
pub const COUNTS_AS_HEAVY: &str = "ID_INTERNAL_GRANTS_COUNTS_AS_EQUIPPED_HEAVY_ARMOR";
pub const CAPACITY_HALVED: &str = "ID_INTERNAL_GRANTS_WEIGHT_CAPACITY_HALVED";
pub const CAPACITY_DOUBLED: &str = "ID_INTERNAL_GRANTS_WEIGHT_CAPACITY_DOUBLED";
pub const CAPACITY_LARGER: &str = "ID_INTERNAL_GRANTS_WEIGHT_CAPACITY_COUNTS_AS_LARGER";
pub const STEALTH_DISADVANTAGE: &str = "ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE";
pub const INITIATIVE_ADVANTAGE: &str = "ID_INTERNAL_GRANTS_INITIATIVE_ADVANTAGE";
pub const SUPPORT_PREFIX: &str = "ID_INTERNAL_SUPPORT";
pub const SHIELD_GROUP: &str = "ID_INTERNAL_ARMOR_GROUP_SHIELD";

/// Ability names in canonical order.
pub const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];
/// Three-letter ability abbreviations in canonical order.
pub const ABILITY_ABBR: [&str; 6] = ["str", "dex", "con", "int", "wis", "cha"];

/// The 18 skills with their governing ability index (into [`ABILITIES`]).
pub const SKILLS: [(&str, usize); 18] = [
    ("acrobatics", 1),
    ("animal handling", 4),
    ("arcana", 3),
    ("athletics", 0),
    ("deception", 5),
    ("history", 3),
    ("insight", 4),
    ("intimidation", 5),
    ("investigation", 3),
    ("medicine", 4),
    ("nature", 3),
    ("perception", 4),
    ("performance", 5),
    ("persuasion", 5),
    ("religion", 3),
    ("sleight of hand", 1),
    ("stealth", 1),
    ("survival", 4),
];

/// Index of an ability from its full name, abbreviation (case-insensitive).
#[must_use]
pub fn ability_index(key: &str) -> Option<usize> {
    let k = key.trim().to_lowercase();
    ABILITIES
        .iter()
        .position(|a| *a == k)
        .or_else(|| ABILITY_ABBR.iter().position(|a| *a == k))
}

/// `ID_LEVEL_n`.
#[must_use]
pub fn level_id(n: u32) -> String {
    format!("{LEVEL_PREFIX}{n}")
}

/// Engine-generated element carrying the origin languages step (Common plus Standard picks).
pub const CREATION_LANGUAGES: &str = "ID_INTERNAL_CREATION_LANGUAGES";

/// Prefix of the improvement features the engine generates per class (B-06):
/// `ID_INTERNAL_CLASS_FEATURE_ASI_<n>_<CLASS>`, `ID_INTERNAL_CLASS_FEATURE_FEAT_<n>_<CLASS>`.
pub const GENERATED_IMPROVEMENT_PREFIXES: [&str; 2] = [
    "ID_INTERNAL_CLASS_FEATURE_ASI_",
    "ID_INTERNAL_CLASS_FEATURE_FEAT_",
];

/// True for an id of an element the engine generates from a class (B-06). When one is not
/// installed its class is not either: the class is the content to report, and the feature
/// returns with it.
#[must_use]
pub fn is_generated_with_class(id: &str) -> bool {
    GENERATED_IMPROVEMENT_PREFIXES
        .iter()
        .any(|p| id.starts_with(p))
}
