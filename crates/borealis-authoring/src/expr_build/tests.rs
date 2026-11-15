//! Unit tests of the expression builders: hand-written shapes per builder and dialect, the quirks the formula writer
//! avoids, problems, and the JSON shape the UI receives.

use super::condition::{ConditionContext, ConditionLanguage, ConditionRow};
use super::equipped::{EquippedLanguage, EquippedRow, Slot, SlotTest};
use super::formula::{dialect_of, presets, FormulaDialect, FormulaLanguage, FormulaNode, Op};
use super::tag_filter::{TagFilter, TagFilterLanguage, TagRow};
use super::{Ability, Built, ExprLanguage, ExprValue, GroupMode, Node};
use serde_json::json;

fn model<M: Clone>(b: Built<M>) -> M {
    match b {
        Built::Model { model } => model,
        Built::Kept { text, reason } => panic!("{text:?} kept: {reason}"),
    }
}

fn rows<R: Clone>(node: &Node<R>) -> Vec<R> {
    node.rows().into_iter().map(|(_, r)| r.clone()).collect()
}

fn cx() -> ConditionContext {
    ConditionContext::new(["Warlock", "Paladin"])
}

fn cond(text: &str) -> Node<ConditionRow> {
    model(ConditionLanguage::parse(text, &cx()))
}

#[test]
fn conditions_read_into_rows() {
    let m = cond(
        "ID_INTERNAL_OPTION_ALLOW_MULTICLASSING&&([str:13]||[dex:13])&&!ID_SRD_MULTICLASS_FIGHTER",
    );
    let Node::Group { mode, items } = &m else {
        panic!()
    };
    assert_eq!(*mode, GroupMode::All);
    assert_eq!(items.len(), 3);
    assert_eq!(
        rows(&m),
        vec![
            ConditionRow::Has {
                id: "ID_INTERNAL_OPTION_ALLOW_MULTICLASSING".into()
            },
            ConditionRow::Ability {
                ability: Ability::Strength,
                min: 13
            },
            ConditionRow::Ability {
                ability: Ability::Dexterity,
                min: 13
            },
            ConditionRow::Lacks {
                id: "ID_SRD_MULTICLASS_FIGHTER".into()
            },
        ]
    );
    assert_eq!(
        rows(&cond(
            "[warlock:2],[character:5],[level:3],[spellcasting:1],[type:Spell]"
        )),
        vec![
            ConditionRow::ClassLevel {
                class: "warlock".into(),
                min: 2
            },
            ConditionRow::CharacterLevel { min: 5 },
            ConditionRow::Level { min: 3 },
            ConditionRow::Spellcasting { level: 1 },
            ConditionRow::Type {
                value: "Spell".into()
            },
        ]
    );
    // without the class loaded, a class key reads as the statistic the engine falls back to
    let plain = model(ConditionLanguage::parse(
        "[warlock:2]",
        &ConditionContext::default(),
    ));
    assert_eq!(
        rows(&plain),
        vec![ConditionRow::Statistic {
            stat: "warlock".into(),
            min: 2
        }]
    );
    assert_eq!(
        rows(&cond(
            "[innate speed:fly:1]||[feature:Fighting Style: Archery]||[feature:7]"
        )),
        vec![
            ConditionRow::Statistic {
                stat: "innate speed:fly".into(),
                min: 1
            },
            ConditionRow::Feature {
                name: "Fighting Style:Archery".into()
            },
            ConditionRow::Feature { name: "7".into() },
        ]
    );
}

#[test]
fn conditions_keep_what_they_cannot_represent() {
    let m = cond("[ranger:archetype:hunter]&&ID_AA_BB");
    assert_eq!(
        rows(&m)[0],
        ConditionRow::Kept {
            text: "[ranger:archetype:hunter]".into()
        }
    );
    assert_eq!(
        ConditionLanguage::render(&m, &cx()),
        "[ranger:archetype:hunter]&&ID_AA_BB"
    );
    match ConditionLanguage::parse("Strength 13 or higher", &cx()) {
        Built::Kept { text, reason } => {
            assert_eq!(text, "Strength 13 or higher");
            assert!(
                reason.starts_with("Borealis can't read this condition"),
                "{reason}"
            );
        }
        Built::Model { .. } => panic!("prose is not a condition"),
    }
}

#[test]
fn condition_groups_render_minimal_text() {
    let none = Node::group(
        GroupMode::None,
        vec![
            Node::row(ConditionRow::Has {
                id: "ID_AA_BB".into(),
            }),
            Node::row(ConditionRow::Has {
                id: "ID_CC_DD".into(),
            }),
        ],
    );
    let m = Node::group(
        GroupMode::Any,
        vec![
            none,
            Node::row(ConditionRow::Ability {
                ability: Ability::Wisdom,
                min: 13,
            }),
            Node::empty(),
        ],
    );
    let text = ConditionLanguage::render(&m, &cx());
    assert_eq!(text, "!(ID_AA_BB||ID_CC_DD)||[wis:13]");
    assert!(ConditionLanguage::means_same(&m, &text, &cx()));
    assert_eq!(ConditionLanguage::render(&Node::empty(), &cx()), "");
    assert_eq!(
        ConditionLanguage::render(
            &ConditionLanguage::first_class_only("ID_SRD_MULTICLASS_PALADIN"),
            &cx()
        ),
        "!ID_SRD_MULTICLASS_PALADIN"
    );
    assert_eq!(model(ConditionLanguage::parse("  ", &cx())), Node::empty());
}

#[test]
fn condition_problems_are_plain_sentences() {
    let bad = Node::group(
        GroupMode::All,
        vec![
            Node::row(ConditionRow::Has {
                id: "Shield Master".into(),
            }),
            Node::row(ConditionRow::Feature { name: " ".into() }),
            Node::row(ConditionRow::Feature {
                name: "Rage, Reckless".into(),
            }),
            Node::row(ConditionRow::Feature {
                name: "Arcane: 3".into(),
            }),
            Node::row(ConditionRow::ClassLevel {
                class: "level".into(),
                min: 3,
            }),
            Node::row(ConditionRow::Statistic {
                stat: "str".into(),
                min: 3,
            }),
            Node::row(ConditionRow::Spellcasting { level: 10 }),
            Node::row(ConditionRow::Type {
                value: "a:b".into(),
            }),
        ],
    );
    let problems = ConditionLanguage::check(&bad, &cx());
    let paths: Vec<&str> = problems.iter().map(|p| p.path.as_str()).collect();
    assert_eq!(
        paths,
        ["items.0", "items.1", "items.2", "items.3", "items.4", "items.5", "items.6", "items.7"]
    );
    assert_eq!(problems[0].message, "Pick the element again.");
    assert!(problems.iter().all(|p| p.message.ends_with('.')));
    let fine = cond("[str:13]&&[feature:Fighting Style: Archery]&&[warlock:2]");
    assert!(ConditionLanguage::check(&fine, &cx()).is_empty());
}

#[test]
fn values_keep_their_spelling_until_edited() {
    let text = "[str:13],[cha:13]";
    let v = ExprValue::read::<ConditionLanguage>(text, &cx());
    assert_eq!(
        v.text::<ConditionLanguage>(&cx()),
        text,
        "unedited: byte for byte"
    );
    let Built::Model { model } = &v.value else {
        panic!()
    };
    let Node::Group { items, .. } = model else {
        panic!()
    };
    let mut edited = items.clone();
    edited[1] = Node::row(ConditionRow::Ability {
        ability: Ability::Charisma,
        min: 15,
    });
    let v2 = ExprValue {
        value: Built::Model {
            model: Node::group(GroupMode::All, edited),
        },
        original: v.original.clone(),
    };
    assert_eq!(
        v2.text::<ConditionLanguage>(&cx()),
        "[str:13]&&[cha:15]",
        "edited: canonical"
    );
    let kept = ExprValue::read::<ConditionLanguage>("((", &cx());
    assert_eq!(kept.text::<ConditionLanguage>(&cx()), "((");
    assert!(kept.check::<ConditionLanguage>(&cx()).is_empty());
    let new = ExprValue::new(Node::group(
        GroupMode::All,
        vec![Node::row(ConditionRow::CharacterLevel { min: 4 })],
    ));
    assert_eq!(new.text::<ConditionLanguage>(&cx()), "[character:4]");
}

/// The JSON the UI receives inside `HomebrewForm.fields` (WB-10 types it from this).
#[test]
fn builder_states_serialize_as_tagged_json() {
    let v = ExprValue::read::<ConditionLanguage>("[str:13]", &cx());
    assert_eq!(
        serde_json::to_value(&v).unwrap(),
        json!({"value": {"shape": "model", "model": {"node": "group", "mode": "all", "items": [
            {"node": "row", "row": {"row": "ability", "ability": "strength", "min": 13}}]}},
            "original": "[str:13]"})
    );
    let f = ExprValue::read::<FormulaLanguage>("max(1, wis:modifier)", &FormulaDialect::Formula);
    assert_eq!(
        serde_json::to_value(&f).unwrap(),
        json!({"value": {"shape": "model", "model": {"term": "max", "args": [
            {"term": "number", "value": 1}, {"term": "ability", "ability": "wisdom"}]}},
            "original": "max(1, wis:modifier)"})
    );
    let t = ExprValue::new(TagFilter::elements(&["ID_A_B", "ID_C_D"]));
    let j = serde_json::to_value(&t).unwrap();
    assert_eq!(j["value"]["model"]["specific"], json!(true));
    assert_eq!(
        j["value"]["model"]["root"]["items"][0],
        json!({"node": "row", "row": {"row": "element", "id": "ID_A_B"}})
    );
    let back: ExprValue<TagFilter> = serde_json::from_value(j).unwrap();
    assert_eq!(back, t);
    let kept = ExprValue::read::<EquippedLanguage>("[armor", &());
    assert_eq!(
        serde_json::to_value(&kept).unwrap()["value"]["shape"],
        json!("kept")
    );
}

fn tags(text: &str) -> TagFilter {
    model(TagFilterLanguage::parse(text, &()))
}

#[test]
fn tag_filters_read_into_rows() {
    let m = tags("$(spellcasting:list),(1||2||3)");
    assert!(!m.specific);
    assert_eq!(
        rows(&m.root),
        vec![
            TagRow::ClassList,
            TagRow::Tag { tag: "1".into() },
            TagRow::Tag { tag: "2".into() },
            TagRow::Tag { tag: "3".into() },
        ]
    );
    assert_eq!(
        TagFilterLanguage::render(&m, &()),
        "$(spellcasting:list),(1||2||3)"
    );
    let ids = tags("ID_INTERNAL_ARMOR_GROUP_LIGHT|ID_INTERNAL_ARMOR_GROUP_MEDIUM");
    assert!(ids.specific);
    assert_eq!(
        TagFilterLanguage::render(&ids, &()),
        "ID_INTERNAL_ARMOR_GROUP_LIGHT|ID_INTERNAL_ARMOR_GROUP_MEDIUM"
    );
    assert_eq!(
        rows(&tags("$(param:Spell List), 0, $(spellcasting:slots)||$(custom)").root),
        vec![
            TagRow::Choice {
                name: "Spell List".into()
            },
            TagRow::Tag { tag: "0".into() },
            TagRow::CastableLevel,
            TagRow::Kept {
                text: "$(custom)".into()
            },
        ]
    );
    let mixed = tags("Artisan tools||ID_SRD_TOOL_NAVIGATORS_TOOLS");
    assert_eq!(
        rows(&mixed.root)[1],
        TagRow::Element {
            id: "ID_SRD_TOOL_NAVIGATORS_TOOLS".into()
        }
    );
}

#[test]
fn specific_mode_needs_an_any_group() {
    // one element has no bar to write: the text reads as Tagged, which matches that element too
    let one = TagFilter::elements(&["ID_A_B"]);
    let text = TagFilterLanguage::render(&one, &());
    assert_eq!(text, "ID_A_B");
    assert!(TagFilterLanguage::means_same(&one, &text, &()));
    assert!(!tags(&text).specific);
    let two = TagFilter::elements(&["ID_A_B", "ID_C_D"]);
    assert_eq!(TagFilterLanguage::render(&two, &()), "ID_A_B|ID_C_D");
}

#[test]
fn tag_problems() {
    let m = TagFilter {
        specific: false,
        root: Node::group(
            GroupMode::Any,
            vec![
                Node::row(TagRow::Tag {
                    tag: "Fire & Ice".into(),
                }),
                Node::row(TagRow::Tag {
                    tag: "ID_X_Y".into(),
                }),
                Node::row(TagRow::Tag {
                    tag: " Padded".into(),
                }),
                Node::row(TagRow::Element {
                    id: "Shield".into(),
                }),
                Node::row(TagRow::Choice {
                    name: "List (a)".into(),
                }),
                Node::row(TagRow::Tag {
                    tag: "Spider’s Cunning: Path".into(),
                }),
            ],
        ),
    };
    let p = TagFilterLanguage::check(&m, &());
    let paths: Vec<&str> = p.iter().map(|x| x.path.as_str()).collect();
    assert_eq!(
        paths,
        ["items.0", "items.1", "items.2", "items.3", "items.4"]
    );
    assert_eq!(
        p[1].message,
        "A tag can't start with ID_; pick the element instead."
    );
}

fn equipped(text: &str) -> Vec<EquippedRow> {
    rows(&model(EquippedLanguage::parse(text, &())))
}

#[test]
fn equipped_reads_into_rows() {
    let slot = |slot, test| EquippedRow::Slot { slot, test };
    assert_eq!(
        equipped("[main:weapon],([offhand:none]||[shield:any])"),
        vec![
            slot(Slot::MainHand, SlotTest::Weapon),
            slot(Slot::OffHand, SlotTest::None),
            slot(Slot::Shield, SlotTest::Any),
        ]
    );
    assert_eq!(
        equipped("![armor:heavy]&&[primary:versatile]&&[secondary:versatile]&&[armor]"),
        vec![
            slot(Slot::Armor, SlotTest::Heavy),
            slot(Slot::MainHand, SlotTest::Versatile),
            slot(
                Slot::OffHand,
                SlotTest::Item {
                    item: "versatile".into()
                }
            ),
            slot(Slot::Armor, SlotTest::Any),
        ]
    );
    assert_eq!(
        equipped("[attuned:ID_X_RING],[item:ID_X_CLOAK],[pet:any]"),
        vec![
            EquippedRow::Attuned {
                item: "id_x_ring".into()
            },
            EquippedRow::Wearing {
                item: "id_x_cloak".into()
            },
            EquippedRow::Kept {
                text: "[pet:any]".into()
            },
        ]
    );
    let m = model(EquippedLanguage::parse("armor:none", &()));
    assert_eq!(EquippedLanguage::render(&m, &()), "[armor:none]");
    assert!(EquippedLanguage::means_same(&m, "armor:none", &()));
}

#[test]
fn equipped_problems() {
    let m = Node::group(
        GroupMode::All,
        vec![
            Node::row(EquippedRow::Slot {
                slot: Slot::Shield,
                test: SlotTest::Heavy,
            }),
            Node::row(EquippedRow::Slot {
                slot: Slot::OffHand,
                test: SlotTest::Versatile,
            }),
            Node::row(EquippedRow::Slot {
                slot: Slot::Armor,
                test: SlotTest::Item {
                    item: "None".into(),
                },
            }),
            Node::row(EquippedRow::Attuned {
                item: String::new(),
            }),
            Node::row(EquippedRow::Slot {
                slot: Slot::Armor,
                test: SlotTest::Item {
                    item: "weapon".into(),
                },
            }),
        ],
    );
    let p = EquippedLanguage::check(&m, &());
    let paths: Vec<&str> = p.iter().map(|x| x.path.as_str()).collect();
    assert_eq!(paths, ["items.0", "items.1", "items.2", "items.3"], "{p:?}");
}

fn formula(text: &str, d: FormulaDialect) -> FormulaNode {
    model(FormulaLanguage::parse(text, &d))
}

fn render(node: &FormulaNode, d: FormulaDialect) -> String {
    FormulaLanguage::render(node, &d)
}

#[test]
fn presets_write_the_content_shapes() {
    let f = FormulaDialect::Formula;
    assert_eq!(
        render(&presets::times_class_level(5, "Paladin"), f),
        "5 * paladin level"
    );
    assert_eq!(
        render(&presets::ability_min_one(Ability::Charisma), f),
        "max(1, charisma:modifier)"
    );
    assert_eq!(
        render(&presets::save_dc(Ability::Wisdom), f),
        "8 + proficiency + wisdom:modifier"
    );
    assert_eq!(
        render(&presets::half_class_level_up("Druid"), f),
        "ceil(druid level / 2)"
    );
    assert_eq!(
        render(&presets::half_class_level_down("sorcerer"), f),
        "sorcerer level / 2"
    );
    assert_eq!(
        render(&presets::save_dc(Ability::Wisdom), FormulaDialect::NoteDc),
        "8 + proficiency + wisdom"
    );
    for d in FormulaDialect::ALL.into_iter().filter(|d| d.is_dc()) {
        assert!(
            FormulaLanguage::check(&presets::save_dc(Ability::Strength), &d).is_empty(),
            "{d:?}"
        );
    }
}

#[test]
fn formula_writer_avoids_the_grammar_quirks() {
    let f = FormulaDialect::Formula;
    let ceil = |c: &str| presets::half_class_level_up(c);
    // `ceil(a / 2) + ceil(b / 2)` reads as one ceil and has no value
    let two = FormulaNode::binary(Op::Add, ceil("wizard"), ceil("cleric"));
    assert_eq!(
        render(&two, f),
        "(ceil(wizard level / 2)) + ceil(cleric level / 2)"
    );
    assert_eq!(formula(&render(&two, f), f), two);
    // a sign right after an operator splits there
    let neg = FormulaNode::binary(
        Op::Add,
        FormulaNode::Proficiency,
        FormulaNode::binary(
            Op::Mul,
            FormulaNode::number(-2),
            FormulaNode::class_level("monk"),
        ),
    );
    assert_eq!(render(&neg, f), "proficiency + (-2) * monk level");
    assert_eq!(formula(&render(&neg, f), f), neg);
    // right-most split: same-precedence right operands need parentheses
    let sub = FormulaNode::binary(
        Op::Sub,
        FormulaNode::number(10),
        FormulaNode::binary(Op::Sub, FormulaNode::number(3), FormulaNode::number(2)),
    );
    assert_eq!(render(&sub, f), "10 - (3 - 2)");
    // a ceil denominator with a top-level `/` is parenthesized
    let den = FormulaNode::CeilDiv {
        num: Box::new(FormulaNode::class_level("bard")),
        den: Box::new(FormulaNode::binary(
            Op::Add,
            FormulaNode::number(1),
            presets::half_class_level_down("bard"),
        )),
    };
    assert_eq!(render(&den, f), "ceil(bard level / (1 + bard level / 2))");
    assert_eq!(formula(&render(&den, f), f), den);
    // inside an argument too
    let arg = FormulaNode::Max {
        args: vec![two.clone(), FormulaNode::number(1)],
    };
    assert_eq!(formula(&render(&arg, f), f), arg);
}

#[test]
fn formulas_read_per_dialect() {
    use FormulaDialect::{BreathDc, Formula, NoteDc, SaveDc, Value, ValueOrClassLevel};
    let dc = presets::save_dc(Ability::Strength);
    let strength_first = FormulaNode::sum(vec![
        FormulaNode::number(8),
        FormulaNode::ability(Ability::Strength),
        FormulaNode::Proficiency,
    ])
    .unwrap();
    assert_eq!(formula("8+strength+proficiency", SaveDc), strength_first);
    assert_eq!(
        render(&strength_first, SaveDc),
        "8 + strength:modifier + proficiency"
    );
    assert_eq!(
        formula("8 + proficiency bonus + Strength:modifier", SaveDc),
        dc
    );
    assert_eq!(
        formula("spell save DC (Cleric)", SaveDc),
        FormulaNode::ClassSaveDc {
            class: "Cleric".into()
        }
    );
    assert_eq!(
        render(
            &FormulaNode::ClassSaveDc {
                class: "Cleric".into()
            },
            SaveDc
        ),
        "spell save DC (Cleric)"
    );
    assert_eq!(
        formula("8+dexterity+proficiency", NoteDc),
        formula("8+dex+proficiency", NoteDc)
    );
    assert_eq!(
        formula("8+constitution+proficiency", BreathDc),
        formula("8 + con:modifier + proficiency", BreathDc)
    );
    assert_eq!(
        formula("proficiency:half", Value),
        FormulaNode::HalfProficiency
    );
    assert_eq!(
        formula("monk level", ValueOrClassLevel),
        FormulaNode::class_level("monk")
    );
    assert_eq!(
        formula("5*cleric level", Formula),
        presets::times_class_level(5, "cleric")
    );
    for (text, d) in [
        ("8 + -1", SaveDc),           // outside the subset both readers read
        ("8 + con", SaveDc),          // short names are not read there
        ("8 + wis:modifier", NoteDc), // nor `:modifier` there
        ("spell save DC (Cleric)", BreathDc),
        ("proficiency", ValueOrClassLevel), // read with a proficiency bonus of 0
        ("max(1, luck:modifier)", Formula),
        ("floor(7, 9)", Formula),
        ("ceil(a level / 2) + ceil(b level / 2)", Formula),
        ("2 * proficiency", Value),
        ("", Formula),
    ] {
        assert!(
            matches!(FormulaLanguage::parse(text, &d), Built::Kept { .. }),
            "{text:?} in {d:?} is kept as is"
        );
    }
}

#[test]
fn formula_problems_per_dialect() {
    use FormulaDialect::{Formula, NoteDc, SaveDc, Value};
    let class_dc = FormulaNode::ClassSaveDc {
        class: "Cleric".into(),
    };
    assert!(FormulaLanguage::check(&class_dc, &SaveDc).is_empty());
    assert_eq!(FormulaLanguage::check(&class_dc, &NoteDc).len(), 1);
    assert_eq!(FormulaLanguage::check(&class_dc, &Formula).len(), 1);
    let neg = FormulaNode::sum(vec![FormulaNode::number(8), FormulaNode::number(-1)]).unwrap();
    let p = FormulaLanguage::check(&neg, &SaveDc);
    assert_eq!((p.len(), p[0].path.as_str()), (1, "terms.1"));
    assert!(FormulaLanguage::check(&neg, &Formula).is_empty());
    assert_eq!(
        FormulaLanguage::check(&FormulaNode::Max { args: vec![] }, &Formula).len(),
        1
    );
    assert_eq!(
        FormulaLanguage::check(&FormulaNode::class_level("half-orc"), &Formula).len(),
        1
    );
    assert_eq!(
        FormulaLanguage::check(&FormulaNode::number(5000), &Formula).len(),
        1
    );
    assert_eq!(
        FormulaLanguage::check(&presets::save_dc(Ability::Wisdom), &Value).len(),
        1
    );
    assert_eq!(
        FormulaLanguage::check(&FormulaNode::HalfProficiency, &Formula).len(),
        1
    );
    assert_eq!(dialect_of("save_dc", "formula"), Some(SaveDc));
    assert_eq!(dialect_of("resource", "max.expr"), Some(Formula));
    assert_eq!(dialect_of("resource", "name"), None);
}
