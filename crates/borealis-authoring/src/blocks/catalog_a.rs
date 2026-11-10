//! The core blocks (spec 6.1) and the custom blocks of the groups "Abilities & feats" and "Spells" (spec 6.2).

use super::choices::{
    ABILITIES, ANY_WORD, CHANGE_ON, FEAT_CATEGORIES, PREPARE_COUNT, PREPARE_WHEN, RECHARGE, RESTS,
    RITUAL_SOURCES, SIZES, SPELL_MODES, STAT_BONUSES, WEAPON_CATEGORIES, WEAPON_PROPERTIES,
};
use super::{BlockField as F, BlockGroup as G, BlockSpec, Control as C, Scope, Variant};
use crate::expr_build::formula::FormulaDialect as D;

const fn f(key: &'static str, label: &'static str, control: C) -> F {
    F::new(key, label, control)
}

/// A block whose reader honours `level` and `requirements` (`KindSpec::gated`).
pub(super) const fn gated(
    kind: &'static str,
    label: &'static str,
    group: G,
    fields: &'static [F],
) -> BlockSpec {
    BlockSpec {
        kind,
        label,
        group,
        common: false,
        level_gated: true,
        only_if: true,
        fields,
    }
}

/// A block without "From level" and "Only if"; [`with_level`] / [`with_only_if`] add them where the reader reads them.
pub(super) const fn plain(
    kind: &'static str,
    label: &'static str,
    group: G,
    fields: &'static [F],
) -> BlockSpec {
    BlockSpec {
        kind,
        label,
        group,
        common: false,
        level_gated: false,
        only_if: false,
        fields,
    }
}

/// Listed first in the Add menu.
pub(super) const fn common(b: BlockSpec) -> BlockSpec {
    BlockSpec { common: true, ..b }
}

/// Shows "From level".
pub(super) const fn with_level(b: BlockSpec) -> BlockSpec {
    BlockSpec {
        level_gated: true,
        ..b
    }
}

/// Shows "Only if".
pub(super) const fn with_only_if(b: BlockSpec) -> BlockSpec {
    BlockSpec { only_if: true, ..b }
}

const NUM: C = C::Number { min: 0, max: 999 };
const COUNT: C = C::Number { min: 1, max: 99 };
const SPELL_LEVEL: C = C::Number { min: 1, max: 9 };
const CLASS_LEVEL: C = C::Number { min: 1, max: 20 };
const CLASS: C = C::Name {
    scope: Scope::Class,
    multiple: false,
};
const RECHARGE_ONE: C = C::Combo {
    choices: RECHARGE,
    custom: false,
};

/// `free_cast`: slotless casts per rest.
pub(super) const FREE_CAST: C = C::Group(&[
    f("uses", "Uses", C::Uses).req(),
    f("recharge", "Regained on", RECHARGE_ONE).req(),
]);

/// The core blocks: `grant`, `select`, `list` (a `select` of type `List`), `stat`.
pub(super) const CORE: &[BlockSpec] = &[
    common(gated(
        "grant",
        "Gives ...",
        G::Basic,
        &[
            f("id", "What it gives", C::Reference { types: &[] }).req(),
            f("type", "Element type", C::Auto),
            f("prepared", "Always prepared", C::Check).help("For a spell."),
            f("spellcasting", "Counts as a spell of", CLASS).help("For a spell: the class whose spell it is."),
        ],
    )),
    common(gated(
        "select",
        "Lets the player choose ...",
        G::Basic,
        &[
            f(
                "type",
                "What to choose",
                C::Combo {
                    choices: &[],
                    custom: true,
                },
            )
            .req()
            .help("An element type (Language, Proficiency, Spell...)."),
            f("name", "Label", C::Text)
                .req()
                .help("Characters that already made this choice keep it only while the label stays the same."),
            f("number", "How many", COUNT),
            f("supports", "From", C::TagFilter),
            f("optional", "May stay empty", C::Check),
            f("default", "Suggested pick", C::Reference { types: &[] }),
            f("spellcasting", "Spells of", CLASS).help("For spells: the owning class."),
            f("prepared", "Always prepared", C::Check).help("For spells."),
            f("allow_replace", "Can be replaced later", C::Check).expert(),
        ],
    )),
    common(gated(
        "list",
        "Lets the player pick from a written list",
        G::Basic,
        &[
            f("type", "List", C::Auto),
            f("name", "Label", C::Text).req(),
            f("number", "How many", COUNT),
            f(
                "items",
                "Rows",
                C::Rows(&[f("id", "Number", C::Auto), f("text", "Text", C::Multiline).req()]),
            )
                .req()
                .help("Each row gets its number automatically."),
        ],
    )),
    common(gated(
        "stat",
        "Changes a number",
        G::Basic,
        &[
            f("name", "Statistic", C::Statistic).req(),
            f("value", "By", C::StatValue)
                .req()
                .help("A number, or another statistic (optionally subtracted)."),
            f(
                "bonus",
                "Stacking",
                C::Combo {
                    choices: STAT_BONUSES,
                    custom: true,
                },
            )
            .help("Bonuses with the same stacking name don't add up; the highest counts."),
            f("equipped", "Only while wearing or holding", C::Equipped),
            f("maximum", "At most", C::StatValue),
            f("alt", "Shown in the breakdown as", C::Text),
            f("inline", "Text value", C::Check).expert(),
        ],
    )),
];

const ABILITY_OPTIONS: C = C::Rows(&[
    f("abilities", "Abilities", C::AbilityChoice).req(),
    f("pick", "How many of them", COUNT),
    f("amount", "Increase by", C::Number { min: 1, max: 10 }).req(),
]);

const WEAPON_MASTERY_COUNT: C = C::Variant(&[
    Variant {
        value: "fixed",
        label: "A fixed number",
        fields: &[f("value", "Number", COUNT).req()],
    },
    Variant {
        value: "table",
        label: "From the class table",
        fields: &[f("key", "Column", C::ClassCounter).req()],
    },
]);

/// Abilities & feats and Spells.
pub(super) const CUSTOM: &[BlockSpec] = &[
    gated(
        "ability_distribution",
        "Ability score points from a list",
        G::Abilities,
        &[
            f("name", "Label", C::Text),
            f(
                "pool",
                "Abilities",
                C::Chips {
                    choices: ABILITIES,
                    custom: false,
                },
            )
            .req(),
            f("options", "Ways to spread the points", C::Splits).req(),
            f("cap", "No score above", C::Number { min: 1, max: 30 }),
        ],
    ),
    gated(
        "ability_increase",
        "Ability score increase (choose)",
        G::Abilities,
        &[
            f("name", "Label", C::Text),
            f("options", "Options", ABILITY_OPTIONS).req(),
            f("cap", "No score above", C::Number { min: 1, max: 30 }),
        ],
    ),
    with_only_if(with_level(plain(
        "grant_feat",
        "Gives a feat",
        G::Abilities,
        &[
            f("feat", "Feat", C::Reference { types: &["Feat"] }).req(),
            f(
                "params",
                "Its choices",
                C::Map(&[
                    f(
                        "name",
                        "Choice",
                        C::Name {
                            scope: Scope::Choice,
                            multiple: false,
                        },
                    ),
                    f("value", "Answer", C::Text),
                ]),
            )
            .help("Answers to the feat's own named choices (for example its spell list)."),
        ],
    ))),
    with_only_if(with_level(plain(
        "select_feat",
        "Lets the player choose a feat",
        G::Abilities,
        &[
            f("name", "Label", C::Text).req(),
            f(
                "category",
                "Category",
                C::Combo {
                    choices: FEAT_CATEGORIES,
                    custom: true,
                },
            ),
            f(
                "default",
                "Suggested feat",
                C::Reference { types: &["Feat"] },
            ),
            f(
                "alternatives",
                "Also offered",
                C::References {
                    types: &["Feat", "Option"],
                },
            ),
            f("distinct", "A different feat each time", C::Check),
        ],
    ))),
    with_only_if(with_level(plain(
        "select_param",
        "Named choice (a word from a list)",
        G::Abilities,
        &[
            f("name", "Label", C::Text).req(),
            f(
                "options",
                "Options",
                C::Chips {
                    choices: &[],
                    custom: true,
                },
            )
            .req(),
            f(
                "distinct_per_instance",
                "A different answer each time",
                C::Check,
            ),
        ],
    ))),
    with_level(plain(
        "select_size",
        "Choose a size",
        G::Abilities,
        &[
            f("name", "Label", C::Text),
            f(
                "options",
                "Sizes",
                C::Chips {
                    choices: SIZES,
                    custom: false,
                },
            )
            .req(),
        ],
    )),
    with_level(plain(
        "expertise",
        "Expertise in skills",
        G::Abilities,
        &[f("count", "How many skills", COUNT).req()],
    )),
    with_level(plain(
        "weapon_mastery",
        "Weapon mastery picks",
        G::Abilities,
        &[
            f("name", "Label", C::Text),
            f("count", "How many", WEAPON_MASTERY_COUNT).req(),
            f(
                "filter",
                "Which weapons",
                C::Group(&[
                    f(
                        "weapon_kind",
                        "Weapons",
                        C::Combo {
                            choices: super::choices::MASTERY_KINDS,
                            custom: false,
                        },
                    ),
                    f(
                        "categories",
                        "Categories",
                        C::Chips {
                            choices: WEAPON_CATEGORIES,
                            custom: false,
                        },
                    ),
                    f(
                        "proficient",
                        "Only weapons the character is proficient with",
                        C::Check,
                    ),
                ]),
            ),
        ],
    )),
    gated(
        "proficiency_filter",
        "Proficiency with a described group of weapons",
        G::Abilities,
        &[
            f("name", "Shown as", C::Text).note(),
            f(
                "weapon_category",
                "Weapon category",
                C::Combo {
                    choices: WEAPON_CATEGORIES,
                    custom: true,
                },
            ),
            f(
                "properties_any",
                "With any of these properties",
                C::Chips {
                    choices: WEAPON_PROPERTIES,
                    custom: true,
                },
            ),
        ],
    ),
    gated(
        "equipment_package",
        "Starting equipment choice",
        G::Abilities,
        &[
            f(
                "ref",
                "Package",
                C::Reference {
                    types: &["Equipment Package"],
                },
            )
            .req(),
            f("name", "Label", C::Text),
        ],
    ),
    // ---- Spells ----
    common(with_only_if(with_level(plain(
        "grant_spell",
        "Gives a spell",
        G::Spells,
        &[
            f("spell", "Spell", C::Reference { types: &["Spell"] }).req(),
            f(
                "mode",
                "How",
                C::Combo {
                    choices: SPELL_MODES,
                    custom: false,
                },
            ),
            f("spell_list", "Counts as a spell of", CLASS),
            f("free_cast", "Free casts", FREE_CAST),
            f(
                "also_with_slots",
                "Can also be cast with spell slots",
                C::Check,
            ),
            f(
                "requirements_param",
                "Only when a named choice is",
                C::Map(&[
                    f(
                        "name",
                        "Choice",
                        C::Name {
                            scope: Scope::Choice,
                            multiple: false,
                        },
                    ),
                    f("value", "Answer", C::Text),
                ]),
            ),
            f("cast_level", "Cast at spell level", SPELL_LEVEL),
            f(
                "resource_cost",
                "Costs resource points",
                C::Group(&[
                    f(
                        "resource",
                        "Pool",
                        C::Name {
                            scope: Scope::Resource,
                            multiple: false,
                        },
                    )
                    .req(),
                    f("cost", "Points", COUNT).req(),
                ]),
            ),
        ],
    )))),
    plain(
        "spell_list_extend",
        "Adds other classes' lists to a spell list",
        G::Spells,
        &[
            f("spell_class", "Spell list of", CLASS).req(),
            f(
                "lists",
                "Also choose from",
                C::Name {
                    scope: Scope::SpellList,
                    multiple: true,
                },
            )
            .req(),
        ],
    ),
    plain(
        "spell_free_cast",
        "Free casting of a chosen spell",
        G::Spells,
        &[
            f(
                "select",
                "The choice it applies to",
                C::Name {
                    scope: Scope::Select,
                    multiple: false,
                },
            )
            .req(),
            f("uses", "Uses", C::Uses),
            f("recharge", "Regained on", RECHARGE_ONE),
            f(
                "also_with_slots",
                "Can also be cast with spell slots",
                C::Check,
            ),
            f("ability", "Spellcasting ability", C::AbilitySource),
        ],
    ),
    gated(
        "free_cast_choice",
        "Free casting of spells from a feature",
        G::Spells,
        &[
            f(
                "spell_level",
                "Spell level",
                C::NumberOr {
                    min: 1,
                    max: 9,
                    words: ANY_WORD,
                    custom: false,
                },
            ),
            f("count", "How many spells", COUNT),
            f("uses_each", "Uses of each", COUNT),
            f("uses", "Uses", COUNT),
            f(
                "recharge",
                "Regained on",
                C::Chips {
                    choices: RESTS,
                    custom: false,
                },
            ),
            f(
                "from_feature",
                "Spells of the feature",
                C::Name {
                    scope: Scope::Feature,
                    multiple: false,
                },
            )
            .note(),
        ],
    ),
    gated(
        "at_will_spells",
        "Spells at will",
        G::Spells,
        &[
            f("levels", "Spell levels", C::Numbers { min: 0, max: 9 }),
            f(
                "casting_time",
                "Casting time",
                C::Combo {
                    choices: super::choices::ACTIONS,
                    custom: true,
                },
            ),
            f(
                "swap_on",
                "Can be swapped",
                C::Combo {
                    choices: CHANGE_ON,
                    custom: false,
                },
            ),
        ],
    ),
    plain(
        "ritual_casting",
        "Ritual casting",
        G::Spells,
        &[f(
            "source",
            "Rituals from",
            C::Combo {
                choices: RITUAL_SOURCES,
                custom: false,
            },
        )],
    ),
    plain(
        "spellbook",
        "Spellbook",
        G::Spells,
        &[
            f("initial_spells", "Spells at first", NUM),
            f("per_level", "Spells added per level", NUM),
            f(
                "copy_cost_gp_per_level",
                "Copy cost per spell level (gp)",
                NUM,
            ),
            f(
                "copy_hours_per_level",
                "Copy time per spell level (hours)",
                NUM,
            ),
        ],
    ),
    plain(
        "prepared_spell_change",
        "Change prepared spells",
        G::Spells,
        &[
            f(
                "when",
                "When",
                C::Combo {
                    choices: PREPARE_WHEN,
                    custom: false,
                },
            ),
            f(
                "count",
                "How many",
                C::Combo {
                    choices: PREPARE_COUNT,
                    custom: false,
                },
            ),
        ],
    ),
    plain("pact_magic", "Pact magic slots", G::Spells, &[]),
    plain(
        "mystic_arcanum",
        "Mystic arcanum",
        G::Spells,
        &[
            f(
                "slots",
                "Spells",
                C::Rows(&[
                    f("spell_level", "Spell level", SPELL_LEVEL).req(),
                    f("level", "At class level", CLASS_LEVEL).req(),
                ]),
            )
            .req(),
            f("free_cast", "Free casts", FREE_CAST),
        ],
    ),
    gated(
        "cantrip_damage_bonus",
        "Cantrip damage bonus",
        G::Spells,
        &[
            f("spell_class", "Cantrips of", CLASS),
            f("value", "Bonus", C::Formula(D::Formula)).req(),
            f("choose_cantrip", "The player chooses one cantrip", C::Check),
            f("cantrip_filter", "Which cantrips", C::Text).note(),
        ],
    ),
    gated(
        "cantrip_range_bonus",
        "Cantrip range bonus",
        G::Spells,
        &[
            f("spell_class", "Cantrips of", CLASS),
            f("feet_per_warlock_level", "Feet per class level", NUM).req(),
            f("cantrip_filter", "Which cantrips", C::Text).note(),
        ],
    ),
    gated(
        "cantrip_rider",
        "Cantrip extra effect",
        G::Spells,
        &[
            f("cantrip_filter", "Which cantrips", C::Text).note(),
            f("effect", "Effect", C::Text).req().note(),
            f("spell_class", "Cantrips of", CLASS),
        ],
    ),
    gated(
        "cantrip_swap",
        "Swap a cantrip",
        G::Spells,
        &[
            f(
                "spell_list",
                "From the spell list",
                C::Name {
                    scope: Scope::SpellList,
                    multiple: false,
                },
            ),
            f(
                "swap_on",
                "Can be swapped",
                C::Combo {
                    choices: CHANGE_ON,
                    custom: false,
                },
            ),
            f(
                "replaces",
                "Replaces",
                C::Name {
                    scope: Scope::Spell,
                    multiple: false,
                },
            )
            .note(),
        ],
    ),
    gated(
        "spell_cast_alt_cost",
        "Cast a spell another way",
        G::Spells,
        &[
            f("spell", "Spell", C::Reference { types: &["Spell"] }).req(),
            f(
                "cost",
                "Paid with",
                C::Chips {
                    choices: &[],
                    custom: true,
                },
            )
            .note(),
            f("no_material", "Without material components", C::Check),
        ],
    ),
    gated(
        "spell_replace_on_level",
        "Replace spells on level up",
        G::Spells,
        &[
            f("scope", "Which spells", C::Text).note(),
            f("same_level", "With a spell of the same level", C::Check),
        ],
    ),
    plain(
        "save_dc",
        "A feature's save DC",
        G::Spells,
        &[
            f("id", "Name", C::Text)
                .req()
                .help("The feature this save DC is for (\"channel divinity\")."),
            f("formula", "DC", C::Formula(D::SaveDc)).req(),
        ],
    ),
    gated(
        "slot_recovery",
        "Recover spell slots",
        G::Spells,
        &[
            f("pact_slots", "Pact Magic slots", C::Check),
            f("amount", "How many", C::Text).note(),
            f("cast_time", "Takes", C::Text).note(),
            f(
                "on",
                "During",
                C::Combo {
                    choices: RESTS,
                    custom: false,
                },
            ),
            f(
                "max_combined_level",
                "Combined slot levels at most",
                C::Formula(D::Formula),
            ),
            f("max_slot_level", "Slot level at most", SPELL_LEVEL),
            f("uses", "Uses", COUNT),
            f("recharge", "Regained on", RECHARGE_ONE),
        ],
    ),
    gated(
        "damage_bonus_spell",
        "Extra damage on spells",
        G::Spells,
        &[
            f("value", "Bonus", C::Formula(D::Formula)).req(),
            f(
                "once_per",
                "Once per",
                C::Combo {
                    choices: super::choices::ONCE_PER,
                    custom: false,
                },
            ),
            f(
                "damage_type_stat",
                "Damage type from the statistic",
                C::Statistic,
            ),
            f(
                "damage_type_param",
                "Damage type from the named choice",
                C::Name {
                    scope: Scope::Choice,
                    multiple: false,
                },
            ),
        ],
    ),
];
