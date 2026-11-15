//! The creature form (spec 4.11): a stat block over `MonsterData`, in the stat block's order. GENERAL holds what
//! heads it (size, type and alignment; armor class and hit points; speed and initiative; challenge), STATISTICS the
//! numbers under it (ability scores, saving throws and skills; damage and conditions; senses and languages; gear),
//! ACTIONS its traits, actions, bonus actions and reactions (each with its attack, saving throw or spellcasting),
//! LEGENDARY its legendary actions, lair actions and regional effects. A creature is written as `Monster` with the
//! stat block in `data`; the content validator checks it (`monster_validate.rs`) and its problems point here.

use super::super::codec::{Codec, CreatureField};
use super::super::FormState;
use super::{
    field, head, tail, Control, FieldDef, KindDef, KindGroup, BOTH, DESCRIPTION, GENERAL,
    ONLY_2024, RULES,
};
use borealis_content::Cr;
use borealis_content::Ruleset;
use serde_json::json;
use std::sync::OnceLock;

/// The creature form's id.
pub const CREATURE_KIND: &str = "creature";

const STATISTICS: &str = "statistics";
const ACTIONS: &str = "actions";
const LEGENDARY: &str = "legendary";
const TABS_OF_CREATURE: &[(&str, &str)] = &[
    (GENERAL, "GENERAL"),
    (STATISTICS, "STATISTICS"),
    (ACTIONS, "ACTIONS"),
    (LEGENDARY, "LEGENDARY"),
    (RULES, "RULES"),
    (DESCRIPTION, "DESCRIPTION"),
];

/// Shared fields a creature has no use for (nobody takes a creature, so no prerequisite, repeat or sheet text).
const NOT_FOR_CREATURES: &[&str] = &[
    "allowDuplicate",
    "requirements",
    "prerequisiteText",
    "sheet",
];

const SIZES: &[(&str, &str)] = &[
    ("tiny", "Tiny"),
    ("small", "Small"),
    ("medium", "Medium"),
    ("large", "Large"),
    ("huge", "Huge"),
    ("gargantuan", "Gargantuan"),
];
const SWARM_SIZES: &[(&str, &str)] = &[
    ("", "Not a swarm"),
    ("tiny", "Tiny"),
    ("small", "Small"),
    ("medium", "Medium"),
    ("large", "Large"),
];
const TYPES: &[(&str, &str)] = &[
    ("aberration", "Aberration"),
    ("beast", "Beast"),
    ("celestial", "Celestial"),
    ("construct", "Construct"),
    ("dragon", "Dragon"),
    ("elemental", "Elemental"),
    ("fey", "Fey"),
    ("fiend", "Fiend"),
    ("giant", "Giant"),
    ("humanoid", "Humanoid"),
    ("monstrosity", "Monstrosity"),
    ("ooze", "Ooze"),
    ("plant", "Plant"),
    ("undead", "Undead"),
];
const ALIGNMENTS: &[(&str, &str)] = &[
    ("unaligned", "unaligned"),
    ("lawful good", "lawful good"),
    ("neutral good", "neutral good"),
    ("chaotic good", "chaotic good"),
    ("lawful neutral", "lawful neutral"),
    ("neutral", "neutral"),
    ("chaotic neutral", "chaotic neutral"),
    ("lawful evil", "lawful evil"),
    ("neutral evil", "neutral evil"),
    ("chaotic evil", "chaotic evil"),
    ("any alignment", "any alignment"),
];
const ABILITIES: &[(&str, &str)] = &[
    ("str", "Strength"),
    ("dex", "Dexterity"),
    ("con", "Constitution"),
    ("int", "Intelligence"),
    ("wis", "Wisdom"),
    ("cha", "Charisma"),
];
const SKILLS: &[(&str, &str)] = &[
    ("acrobatics", "Acrobatics"),
    ("animal handling", "Animal Handling"),
    ("arcana", "Arcana"),
    ("athletics", "Athletics"),
    ("deception", "Deception"),
    ("history", "History"),
    ("insight", "Insight"),
    ("intimidation", "Intimidation"),
    ("investigation", "Investigation"),
    ("medicine", "Medicine"),
    ("nature", "Nature"),
    ("perception", "Perception"),
    ("performance", "Performance"),
    ("persuasion", "Persuasion"),
    ("religion", "Religion"),
    ("sleight of hand", "Sleight of Hand"),
    ("stealth", "Stealth"),
    ("survival", "Survival"),
];
pub(super) const DAMAGE_TYPES: &[(&str, &str)] = &[
    ("acid", "Acid"),
    ("bludgeoning", "Bludgeoning"),
    ("cold", "Cold"),
    ("fire", "Fire"),
    ("force", "Force"),
    ("lightning", "Lightning"),
    ("necrotic", "Necrotic"),
    ("piercing", "Piercing"),
    ("poison", "Poison"),
    ("psychic", "Psychic"),
    ("radiant", "Radiant"),
    ("slashing", "Slashing"),
    ("thunder", "Thunder"),
];
const CONDITIONS: &[(&str, &str)] = &[
    ("blinded", "blinded"),
    ("charmed", "charmed"),
    ("deafened", "deafened"),
    ("exhaustion", "exhaustion"),
    ("frightened", "frightened"),
    ("grappled", "grappled"),
    ("incapacitated", "incapacitated"),
    ("invisible", "invisible"),
    ("paralyzed", "paralyzed"),
    ("petrified", "petrified"),
    ("poisoned", "poisoned"),
    ("prone", "prone"),
    ("restrained", "restrained"),
    ("stunned", "stunned"),
    ("unconscious", "unconscious"),
];
const EDITIONS: &[(&str, &str)] = &[("dnd2014", "2014"), ("dnd2024", "2024")];

/// The challenge ratings, `0` to `30` (`Cr::XP_TABLE`), as combo choices.
fn challenge_ratings() -> &'static [(&'static str, &'static str)] {
    static CRS: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    CRS.get_or_init(|| Cr::XP_TABLE.iter().map(|(t, _)| (*t, *t)).collect())
}

fn cf(f: CreatureField) -> Codec {
    Codec::Creature(f)
}

fn identity() -> Vec<FieldDef> {
    vec![
        field(
            "statBlock",
            "Stat block edition",
            GENERAL,
            Control::Combo,
            cf(CreatureField::StatBlock),
        )
        .choices(EDITIONS, false)
        .read_only()
        .expert()
        .help("Follows the edition: the 2024 stat block adds initiative and gear.")
        .default("\"\""),
        field(
            "size",
            "Size",
            GENERAL,
            Control::Chips,
            cf(CreatureField::Size),
        )
        .choices(SIZES, false)
        .required()
        .section("Size, type and alignment")
        .default("[\"medium\"]")
        .help("Pick two for a creature of either size (Medium or Small)."),
        field(
            "creatureType",
            "Creature type",
            GENERAL,
            Control::Combo,
            cf(CreatureField::Type),
        )
        .choices(TYPES, false)
        .required()
        .section("Size, type and alignment")
        .default("\"\""),
        field(
            "swarmOf",
            "Swarm of",
            GENERAL,
            Control::Combo,
            cf(CreatureField::SwarmOf),
        )
        .choices(SWARM_SIZES, false)
        .section("Size, type and alignment")
        .default("\"\"")
        .help("For a swarm: the size of each creature in it."),
        field(
            "creatureTags",
            "Type tags",
            GENERAL,
            Control::Chips,
            cf(CreatureField::Tags),
        )
        .section("Size, type and alignment")
        .default("[]")
        .help("Shown after the type, like goblinoid or shapechanger."),
        field(
            "alignment",
            "Alignment",
            GENERAL,
            Control::Combo,
            cf(CreatureField::Alignment),
        )
        .choices(ALIGNMENTS, true)
        .required()
        .section("Size, type and alignment")
        .default("\"unaligned\""),
        field(
            "armorClass",
            "Armor class",
            GENERAL,
            Control::ArmorClass,
            cf(CreatureField::ArmorClass),
        )
        .section("Armor class and hit points")
        .default("{\"entries\":[],\"note\":\"\"}")
        .help("A number, with what gives it; or, when it is worked out, the words for it."),
        field(
            "hitPoints",
            "Hit points",
            GENERAL,
            Control::HitPoints,
            cf(CreatureField::HitPoints),
        )
        .section("Armor class and hit points")
        .default("{\"average\":null,\"formula\":\"\",\"note\":\"\"}")
        .help("The hit dice give the average; or, when they are worked out, the words for them."),
        field(
            "speed",
            "Speed",
            GENERAL,
            Control::Speeds,
            cf(CreatureField::Speed),
        )
        .section("Speed and initiative")
        .default("{\"walk\":30}"),
        field(
            "initiative",
            "Initiative",
            GENERAL,
            Control::Initiative,
            cf(CreatureField::Initiative),
        )
        .editions(ONLY_2024)
        .section("Speed and initiative")
        .default("null"),
        field(
            "challenge",
            "Challenge",
            GENERAL,
            Control::Challenge,
            cf(CreatureField::Challenge),
        )
        .choices(challenge_ratings(), false)
        .section("Challenge")
        .default(
            "{\"cr\":\"\",\"note\":\"\",\"xp\":null,\"xpLair\":null,\"proficiencyBonus\":null}",
        )
        .help("The XP and proficiency bonus follow the challenge rating unless you change them."),
    ]
}

fn statistics() -> Vec<FieldDef> {
    let damage = |key, label, f| {
        field(key, label, STATISTICS, Control::Damage, cf(f))
            .choices(DAMAGE_TYPES, false)
            .section("Damage and conditions")
            .default("[]")
    };
    vec![
        field(
            "abilities",
            "Ability scores",
            STATISTICS,
            Control::Abilities,
            cf(CreatureField::Abilities),
        )
        .section("Ability scores")
        .default("{\"str\":10,\"dex\":10,\"con\":10,\"int\":10,\"wis\":10,\"cha\":10}"),
        field(
            "saves",
            "Saving throws",
            STATISTICS,
            Control::Bonuses,
            cf(CreatureField::Saves),
        )
        .choices(ABILITIES, false)
        .section("Ability scores")
        .default("{}")
        .help("Only the saving throws it is proficient in, with their whole bonus."),
        field(
            "skills",
            "Skills",
            STATISTICS,
            Control::Bonuses,
            cf(CreatureField::Skills),
        )
        .help("Only the skills it is proficient in, with their whole bonus.")
        .choices(SKILLS, false)
        .section("Ability scores")
        .default("{}"),
        field(
            "skillNotes",
            "Skill notes",
            STATISTICS,
            Control::Chips,
            cf(CreatureField::SkillNotes),
        )
        .section("Ability scores")
        .default("[]")
        .expert(),
        damage(
            "damageVulnerabilities",
            "Damage vulnerabilities",
            CreatureField::Vulnerabilities,
        ),
        damage(
            "damageResistances",
            "Damage resistances",
            CreatureField::Resistances,
        ),
        damage(
            "damageImmunities",
            "Damage immunities",
            CreatureField::Immunities,
        ),
        field(
            "conditionImmunities",
            "Condition immunities",
            STATISTICS,
            Control::Chips,
            cf(CreatureField::ConditionImmunities),
        )
        .choices(CONDITIONS, true)
        .section("Damage and conditions")
        .default("[]"),
        field(
            "senses",
            "Senses",
            STATISTICS,
            Control::Senses,
            cf(CreatureField::Senses),
        )
        .help("Its special senses, such as darkvision, with their range in feet.")
        .section("Senses and languages")
        .default("{\"passive_perception\":null}"),
        field(
            "languages",
            "Languages",
            STATISTICS,
            Control::Chips,
            cf(CreatureField::Languages),
        )
        .section("Senses and languages")
        .default("[]")
        .help("As printed, one per chip (Common, understands Draconic but can't speak)."),
        field(
            "telepathy",
            "Telepathy (feet)",
            STATISTICS,
            Control::Number,
            cf(CreatureField::Telepathy),
        )
        .range(0, 1000)
        .section("Senses and languages")
        .default("null"),
        field(
            "gear",
            "Gear",
            STATISTICS,
            Control::Chips,
            cf(CreatureField::Gear),
        )
        .editions(ONLY_2024)
        .section("Gear")
        .default("[]"),
    ]
}

fn entries() -> Vec<FieldDef> {
    let list = |key, label, tab, f, help| {
        field(key, label, tab, Control::Entries, cf(f))
            .default("[]")
            .help(help)
    };
    vec![
        list(
            "traits",
            "Traits",
            ACTIONS,
            CreatureField::Traits,
            "What it always has or does (Keen Smell, Spellcasting).",
        ),
        list(
            "actions",
            "Actions",
            ACTIONS,
            CreatureField::Actions,
            "An attack has its to-hit, reach or range and damage.",
        ),
        list(
            "bonusActions",
            "Bonus actions",
            ACTIONS,
            CreatureField::BonusActions,
            "Taken as a bonus action.",
        ),
        list(
            "reactions",
            "Reactions",
            ACTIONS,
            CreatureField::Reactions,
            "Taken as a reaction, with what triggers it in the text.",
        ),
        field(
            "legendary",
            "Legendary actions",
            LEGENDARY,
            Control::Legendary,
            cf(CreatureField::Legendary),
        )
        .help("Actions it takes at the end of other creatures' turns, with how many it has each round.")
        .default("null"),
        field(
            "lairActions",
            "Lair actions",
            LEGENDARY,
            Control::Lair,
            cf(CreatureField::LairActions),
        )
        .help("What happens in its lair, and when.")
        .default("null"),
        field(
            "regionalEffects",
            "Regional effects",
            LEGENDARY,
            Control::Lair,
            cf(CreatureField::RegionalEffects),
        )
        .help("How the land around its lair changes, and how long the change lasts.")
        .default("null"),
        field(
            "flavour",
            "Flavour text",
            DESCRIPTION,
            Control::Multiline,
            cf(CreatureField::Flavour),
        )
        .default("\"\"")
        .help("Printed with the stat block."),
    ]
}

/// The creature form.
pub(super) fn creature_kind() -> KindDef {
    let shared = tail()
        .into_iter()
        .filter(|f| !NOT_FOR_CREATURES.contains(&f.key))
        .map(|f| match f.tab {
            GENERAL => f.section("Details"),
            RULES => FieldDef {
                label: "Other rules",
                help: Some("Rules of the creature that no field holds."),
                ..f
            },
            _ => f,
        });
    KindDef {
        id: CREATURE_KIND,
        label: "Creature",
        plural: "Creatures",
        description: "A stat block: a monster, a companion, a familiar or a Wild Shape form.",
        group: KindGroup::Main,
        element_types: &["Monster"],
        parent_kinds: &[],
        editions: BOTH,
        tabs: TABS_OF_CREATURE,
        fields: head()
            .into_iter()
            .chain(identity())
            .chain(statistics())
            .chain(entries())
            .chain(shared)
            .collect(),
    }
}

/// A new creature of `edition` (its stat block follows the edition: 2024 adds initiative and gear).
#[must_use]
pub fn new_creature_form(edition: Ruleset) -> FormState {
    let mut form = creature_kind().new_form("e");
    form.fields
        .insert("edition".into(), json!(edition.as_str()));
    form
}
