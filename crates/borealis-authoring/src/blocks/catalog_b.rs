//! The custom blocks of the groups "Uses & resources", "Attacks & damage", "Defence & saves", "Checks & movement",
//! "Core rules only" (spec 6.2), and the effect-only toggle block `speed_multiply`.

use super::catalog_a::{common, gated, plain};
use super::choices::{
    ABILITIES, ACTIONS, ALL_WORD, ATTACK_WEAPONS, CHALLENGE_RATINGS, CHANGE_ON, CONDITIONS,
    CRIT_APPLIES, DAMAGE_TYPES, DIE_FLOOR_WEAPONS, DIE_SIZES, INSPIRATION_TRIGGERS, MOVEMENT_MODES,
    NOT_WEARING, ONCE_PER, RECHARGE, ROLLS, SAVE_SCOPES, SENSES, SKILLS, SKILLS_LOWER, SPEEDS,
    SPEED_WORD, TOGGLE_BLOCKS, TURN_TIMINGS, WEAPON_KINDS,
};
use super::{
    BlockField as F, BlockGroup as G, BlockSpec, Control as C, Scope, Variant, TOGGLE_EFFECT_KINDS,
};
use crate::expr_build::formula::FormulaDialect as D;

const fn f(key: &'static str, label: &'static str, control: C) -> F {
    F::new(key, label, control)
}

const NUM: C = C::Number { min: 0, max: 999 };
const COUNT: C = C::Number { min: 1, max: 99 };
const FEET: C = C::Number { min: 0, max: 9999 };
const CLASS_LEVEL: C = C::Number { min: 1, max: 20 };
const D20: C = C::Number { min: 1, max: 20 };
const ABILITY: C = C::Combo {
    choices: ABILITIES,
    custom: false,
};
const ACTION: C = C::Combo {
    choices: ACTIONS,
    custom: false,
};
const CLASS: C = C::Name {
    scope: Scope::Class,
    multiple: false,
};
const RESOURCE: C = C::Name {
    scope: Scope::Resource,
    multiple: false,
};
const CONDITION_ONE: C = C::Combo {
    choices: CONDITIONS,
    custom: false,
};
const CONDITION_LIST: C = C::Chips {
    choices: CONDITIONS,
    custom: false,
};
const DAMAGE_TYPE_LIST: C = C::Chips {
    choices: DAMAGE_TYPES,
    custom: false,
};
const RECHARGE_LIST: C = C::Chips {
    choices: RECHARGE,
    custom: false,
};
const FORMULA: C = C::Formula(D::Formula);

/// `resource.max`.
const RESOURCE_MAX: C = C::Variant(&[
    Variant {
        value: "fixed",
        label: "A fixed number",
        fields: &[f("value", "Number", NUM).req()],
    },
    Variant {
        value: "stat",
        label: "A statistic",
        fields: &[f("stat", "Statistic", C::Statistic).req()],
    },
    Variant {
        value: "table",
        label: "From the class table",
        fields: &[f("key", "Column", C::ClassCounter).req()],
    },
    Variant {
        value: "formula",
        label: "A formula",
        fields: &[f("expr", "Formula", FORMULA).req()],
    },
    Variant {
        value: "rolled",
        label: "Rolled dice (rolled once when gained)",
        fields: &[f("dice", "Dice", C::Dice).req()],
    },
]);

/// Uses & resources, Attacks & damage, Defence & saves, Checks & movement, Core rules only.
pub(super) const CUSTOM: &[BlockSpec] = &[
    common(gated(
        "resource",
        "Limited uses / pool",
        G::Uses,
        &[
            f("id", "Pool name", C::Text)
                .req()
                .help("Other blocks refer to the pool by this name; written from the label when left alone."),
            f("name", "Label", C::Text),
            f("max", "Maximum", RESOURCE_MAX).req(),
            f("recharge", "Regained on", RECHARGE_LIST),
            f(
                "regain",
                "Regained per rest",
                C::Map(&[
                    f(
                        "on",
                        "Rest",
                        C::Combo {
                            choices: RECHARGE,
                            custom: false,
                        },
                    ),
                    f(
                        "amount",
                        "Uses",
                        C::NumberOr {
                            min: 0,
                            max: 999,
                            words: ALL_WORD,
                            custom: false,
                        },
                    ),
                ]),
            ),
            f("alt_regain", "Also regained by", C::Text).note(),
            f("replaces", "Replaces the pool", RESOURCE),
        ],
    )),
    gated(
        "resource_cost",
        "Costs uses of a pool",
        G::Uses,
        &[f("resource", "Pool", RESOURCE).req(), f("cost", "Uses", COUNT)],
    ),
    gated(
        "resource_regain",
        "Regains uses of a pool",
        G::Uses,
        &[
            f("resource", "Pool", RESOURCE).req(),
            f("on", "On", RECHARGE_LIST),
            f("by", "By", C::Text).note(),
            f("up_to", "Up to", COUNT),
            f(
                "amount",
                "Uses",
                C::NumberOr {
                    min: 0,
                    max: 999,
                    words: ALL_WORD,
                    custom: true,
                },
            )
            .help("A number, all, or words shown as a note."),
        ],
    ),
    gated(
        "toggle",
        "On/off state (rage, a stance)",
        G::Uses,
        &[
            f("id", "State name", C::Text)
                .req()
                .help("Other blocks refer to the state by this name."),
            f("name", "Label", C::Text),
            f(
                "activation",
                "Turned on with",
                C::Combo {
                    choices: ACTIONS,
                    custom: true,
                },
            ),
            f("duration", "Lasts", C::Text).note(),
            f("effects", "While on", C::Blocks(&TOGGLE_EFFECT_KINDS)),
            f(
                "requires_not_wearing",
                "Not while wearing",
                C::Combo {
                    choices: NOT_WEARING,
                    custom: false,
                },
            ),
            f(
                "blocks",
                "Prevents",
                C::Chips {
                    choices: TOGGLE_BLOCKS,
                    custom: true,
                },
            ),
        ],
    ),
    gated(
        "convert",
        "Convert one resource to another",
        G::Uses,
        &[
            f("from", "From", C::Text).note(),
            f("to", "To", C::Text).note(),
            f(
                "table",
                "Costs",
                C::Rows(&[
                    f("slot", "Slot level", C::Number { min: 1, max: 9 }).req(),
                    f("cost", "Cost", NUM).req(),
                    f("min_level", "From class level", CLASS_LEVEL),
                ]),
            ),
            f("amount", "Amount", C::Text).note(),
            f("action", "Takes", ACTION),
        ],
    ),
    gated(
        "healing",
        "Healing ability",
        G::Uses,
        &[
            f("name", "Label", C::Text),
            f("dice", "Dice", C::Dice),
            f("plus", "Plus", FORMULA),
            f("action", "Takes", ACTION),
        ],
    ),
    gated(
        "heroic_inspiration",
        "Heroic Inspiration",
        G::Uses,
        &[f(
            "trigger",
            "Gained",
            C::Combo {
                choices: INSPIRATION_TRIGGERS,
                custom: false,
            },
        )
        .req()],
    ),
    gated(
        "channel_divinity_option",
        "Channel Divinity option",
        G::Uses,
        &[
            f("name", "Label", C::Text).req(),
            f("duration", "Lasts", C::Text).note(),
            f("attack_bonus", "Attack bonus", FORMULA),
            f("pool_hit_points", "Pool of hit points", FORMULA),
            f("max_fraction", "Healing limit", C::Text).note(),
        ],
    ),
    gated(
        "replace_on_level",
        "Replace a pick on level up",
        G::Uses,
        &[f("scope", "Which picks", C::Text).note(), f("count", "How many", COUNT)],
    ),
    // ---- Attacks & damage ----
    gated(
        "attack_bonus",
        "Attack bonus",
        G::Attacks,
        &[
            f(
                "weapon",
                "With",
                C::Combo {
                    choices: ATTACK_WEAPONS,
                    custom: false,
                },
            ),
            // `attack_rules` reads a whole number (`as_i64`), not a formula
            f("value", "Bonus", C::Number { min: -20, max: 20 }).req(),
        ],
    ),
    gated(
        "critical_range",
        "Critical hit range",
        G::Attacks,
        &[
            f("min", "Critical hit on a roll of at least", C::Number { min: 2, max: 20 }).req(),
            f(
                "applies_to",
                "On",
                C::Chips {
                    choices: CRIT_APPLIES,
                    custom: false,
                },
            ),
            f(
                "item",
                "Only with the weapon",
                C::Reference {
                    types: &["Weapon", "Magic Item"],
                },
            )
            .help("Leave empty for every weapon."),
        ],
    ),
    gated(
        "damage_die_floor",
        "Reroll low damage dice",
        G::Attacks,
        &[
            f("floor", "Treat rolls below as", C::Number { min: 2, max: 12 }).req(),
            f(
                "weapon",
                "With",
                C::Combo {
                    choices: DIE_FLOOR_WEAPONS,
                    custom: false,
                },
            ),
        ],
    ),
    common(gated(
        "extra_damage",
        "Extra damage",
        G::Attacks,
        &[
            f("name", "Label", C::Text),
            f("dice", "Dice", C::Dice),
            f("dice_table", "Dice from the class table", C::ClassCounter),
            f(
                "die",
                "Die",
                C::Combo {
                    choices: DIE_SIZES,
                    custom: false,
                },
            ),
            f("dice_count", "Number of dice", C::NumberOrStatistic),
            f("damage_type", "Damage type", C::Text),
            f(
                "once_per",
                "Once per",
                C::Combo {
                    choices: ONCE_PER,
                    custom: true,
                },
            ),
            f("trigger", "When", C::Text).note(),
            f("requires", "Requires", C::Text).note(),
            f("cost", "Costs", C::Text).note(),
            f(
                "replaces",
                "Replaces",
                C::Name {
                    scope: Scope::ExtraDamage,
                    multiple: false,
                },
            ),
            f("effects_choose", "Effects to choose", COUNT),
            f("per_slot_level", "Plus per slot level", C::Dice),
        ],
    )),
    gated(
        "extra_attack_damage_modifier",
        "Add modifier to the extra attack",
        G::Attacks,
        &[],
    ),
    gated(
        "damage_scaling",
        "Damage that grows with level",
        G::Attacks,
        &[
            f("name", "Label", C::Text),
            f(
                "levels",
                "Dice by level",
                C::Map(&[f("level", "From level", CLASS_LEVEL), f("dice", "Dice", C::Dice)]),
            )
            .req(),
            f("type_from", "Damage type", C::Text).note(),
        ],
    ),
    gated(
        "martial_arts",
        "Martial arts",
        G::Attacks,
        &[
            f("die_table", "Die from the class table", C::ClassCounter),
            f(
                "conditions",
                "Only",
                C::Group(&[
                    f("no_armor", "Without armor", C::Check),
                    f("no_shield", "Without a shield", C::Check),
                ]),
            ),
            f("bonus_unarmed_strike", "Bonus unarmed strike", C::Check),
        ],
    ),
    gated(
        "pact_weapon",
        "Pact weapon",
        G::Attacks,
        &[
            f("action", "Summoned with", ACTION),
            f(
                "weapon_kinds",
                "Weapon kinds",
                C::Chips {
                    choices: WEAPON_KINDS,
                    custom: false,
                },
            ),
            f("ability", "Attack ability", ABILITY),
            f("damage_types", "Damage types", DAMAGE_TYPE_LIST),
            f("is_spellcasting_focus", "Is a spellcasting focus", C::Check),
        ],
    ),
    gated(
        "cunning_strike",
        "Cunning strike",
        G::Attacks,
        &[
            f("save_dc", "Save DC", C::Formula(D::NoteDc)).req(),
            f("max_effects", "Effects per hit", COUNT),
        ],
    ),
    gated(
        "cunning_effect",
        "Cunning strike effect",
        G::Attacks,
        &[f("cost_dice", "Costs dice", COUNT).req(), f("save", "Save", ABILITY)],
    ),
    // ---- Defence & saves ----
    gated(
        "damage_reduction",
        "Reduce damage",
        G::Defence,
        &[
            f("dice", "Dice", C::Dice),
            f("plus", "Plus", C::Formulas(D::ValueOrClassLevel)),
            f("damage_types", "Damage types", DAMAGE_TYPE_LIST),
        ],
    ),
    gated(
        "damage_reduction_all_types",
        "Damage reduction applies to all types",
        G::Defence,
        &[],
    ),
    gated("evasion", "Evasion", G::Defence, &[]),
    common(gated(
        "condition_immunity",
        "Immune to conditions",
        G::Defence,
        &[f("conditions", "Conditions", CONDITION_LIST).req()],
    )),
    gated(
        "condition_immunity_while",
        "Immune to conditions while on",
        G::Defence,
        &[
            f(
                "toggle",
                "While",
                C::Name {
                    scope: Scope::Toggle,
                    multiple: false,
                },
            )
            .req(),
            f("conditions", "Conditions", CONDITION_LIST).req(),
        ],
    ),
    common(gated(
        "save_advantage",
        "Advantage on saves",
        G::Defence,
        &[
            f("ability", "Saving throws of", ABILITY),
            f("against_condition", "Against being", CONDITION_ONE),
            f(
                "against_ability",
                "Saving throws of these abilities",
                C::Chips {
                    choices: ABILITIES,
                    custom: false,
                },
            ),
            f(
                "scope",
                "Only",
                C::Combo {
                    choices: SAVE_SCOPES,
                    custom: false,
                },
            ),
            f("context", "To", C::Text).note(),
            f("unless_condition", "Unless", CONDITION_ONE),
        ],
    )),
    gated(
        "save_advantage_check",
        "Advantage on a named check",
        G::Defence,
        &[f("context", "To", C::Text).req().note()],
    ),
    gated(
        "death_save_rules",
        "Death save rules",
        G::Defence,
        &[
            f("advantage", "Advantage on death saves", C::Check),
            f("natural_20_threshold", "Counts as a 20 from", D20),
        ],
    ),
    gated(
        "regeneration",
        "Regain hit points each turn",
        G::Defence,
        &[
            f("condition", "While", C::Text).note(),
            f("amount", "Hit points", FORMULA).req(),
            f(
                "timing",
                "When",
                C::Combo {
                    choices: TURN_TIMINGS,
                    custom: false,
                },
            ),
        ],
    ),
    gated(
        "aura",
        "Aura",
        G::Defence,
        &[
            f("radius_feet", "Radius (feet)", FEET),
            f(
                "radius_by_level",
                "Radius by level",
                C::Map(&[f("level", "From level", CLASS_LEVEL), f("feet", "Feet", FEET)]),
            ),
            f(
                "bonus_saving_throws",
                "Bonus to saving throws",
                C::Group(&[f("formula", "Bonus", FORMULA).req()]),
            ),
        ],
    ),
    gated(
        "aura_immunity",
        "Aura grants immunity",
        G::Defence,
        &[
            f(
                "aura",
                "Aura",
                C::Name {
                    scope: Scope::Aura,
                    multiple: false,
                },
            )
            .req(),
            f("conditions", "Conditions", CONDITION_LIST).req(),
        ],
    ),
    // ---- Checks & movement ----
    common(gated(
        "roll_advantage",
        "Advantage on a roll",
        G::Checks,
        &[
            f(
                "roll",
                "Roll",
                C::Combo {
                    choices: ROLLS,
                    custom: false,
                },
            )
            .req(),
            f("ability", "Ability", ABILITY),
            f(
                "skill",
                "Skill",
                C::Combo {
                    choices: SKILLS_LOWER,
                    custom: false,
                },
            ),
            f("spell_class", "Spells of", CLASS),
        ],
    )),
    gated(
        "skill_bonus",
        "Bonus to skills",
        G::Checks,
        &[
            f(
                "skills",
                "Skills",
                C::Chips {
                    choices: SKILLS,
                    custom: false,
                },
            )
            .req(),
            f("value", "Bonus", C::Formula(D::Value)).req(),
            f("minimum", "At least", NUM),
        ],
    ),
    gated(
        "skill_bonus_unproficient",
        "Bonus to unproficient checks",
        G::Checks,
        &[f("value", "Bonus", C::Formula(D::Value)).req()],
    ),
    gated(
        "check_floor",
        "Minimum roll on checks",
        G::Checks,
        &[
            f(
                "applies_to",
                "On",
                C::Chips {
                    choices: &[],
                    custom: true,
                },
            )
            .req()
            .note(),
            f("floor", "Total at least the score of", ABILITY),
            f("d20_floor", "A d20 roll counts as at least", D20),
        ],
    ),
    gated("initiative_swap", "Swap initiative", G::Checks, &[]),
    common(gated(
        "sense",
        "Special sense",
        G::Checks,
        &[
            f(
                "sense",
                "Sense",
                C::Combo {
                    choices: SENSES,
                    custom: true,
                },
            )
            .req(),
            f("range", "Range (feet)", FEET),
            f("note", "Note", C::Text).note(),
        ],
    )),
    common(gated(
        "movement_mode",
        "Movement mode",
        G::Checks,
        &[
            f(
                "mode",
                "Mode",
                C::Combo {
                    choices: MOVEMENT_MODES,
                    custom: false,
                },
            )
            .req(),
            f(
                "speed",
                "Speed (feet)",
                C::NumberOr {
                    min: 0,
                    max: 9999,
                    words: SPEED_WORD,
                    custom: false,
                },
            ),
            f("duration", "Lasts", C::Text).note(),
            f("action", "Takes", ACTION),
        ],
    )),
    gated(
        "carry_capacity_size_bonus",
        "Count as larger for carrying",
        G::Checks,
        &[f("steps", "Sizes larger", C::Number { min: 1, max: 5 }).req()],
    ),
    gated(
        "beast_forms",
        "Shape-change forms",
        G::Checks,
        &[
            f(
                "known",
                "Forms known",
                C::Rows(&[
                    f("level", "From class level", CLASS_LEVEL).req(),
                    f("forms", "Forms", COUNT).req(),
                    f(
                        "max_cr",
                        "Challenge rating at most",
                        C::Combo {
                            choices: CHALLENGE_RATINGS,
                            custom: true,
                        },
                    )
                    .req(),
                    f("fly", "Flying forms", C::Check),
                ]),
            )
            .req(),
            f("duration_hours", "Lasts (hours)", FORMULA),
            f("temp_hp", "Temporary hit points", FORMULA),
            f(
                "change_on",
                "Forms can be changed",
                C::Combo {
                    choices: CHANGE_ON,
                    custom: false,
                },
            ),
        ],
    ),
    // ---- Core rules only ----
    plain(
        "exhaustion",
        "Exhaustion rules",
        G::CoreRules,
        &[
            f("max_levels", "Levels", COUNT),
            f("d20_penalty_per_level", "d20 penalty per level", NUM),
            f("speed_penalty_ft_per_level", "Speed penalty per level (feet)", FEET),
        ],
    ),
];

/// Blocks that exist only inside a toggle's effects.
pub(super) const EFFECTS: &[BlockSpec] = &[plain(
    "speed_multiply",
    "Multiply a speed",
    G::Effects,
    &[
        f(
            "speed",
            "Speed",
            C::Combo {
                choices: SPEEDS,
                custom: false,
            },
        )
        .req(),
        f("factor", "Times", C::Number { min: 2, max: 10 }).req(),
        f(
            "of",
            "Of the speed",
            C::Combo {
                choices: SPEEDS,
                custom: false,
            },
        )
        .help(
            "At least this many times another speed (a flying speed of twice the walking speed).",
        ),
    ],
)];
