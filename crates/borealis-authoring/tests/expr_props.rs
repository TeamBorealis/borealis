//! Property tests of the expression builders against the engine (spec 18.1-18.2, WB-02): for generated builder models,
//! the text a builder writes is read by the engine's own parser and evaluator exactly as the model says, in every
//! formula dialect by that dialect's engine reader. Deterministic generator (no external crate), as in
//! `borealis-content/src/formula_tests.rs`.

use borealis_authoring::expr_build::condition::{
    ConditionContext, ConditionLanguage, ConditionRow,
};
use borealis_authoring::expr_build::equipped::{EquippedLanguage, EquippedRow, Slot, SlotTest};
use borealis_authoring::expr_build::formula::{FormulaDialect, FormulaLanguage, FormulaNode, Op};
use borealis_authoring::expr_build::tag_filter::{TagFilter, TagFilterLanguage, TagRow};
use borealis_authoring::expr_build::{Ability, Built, ExprLanguage, GroupMode, Node};
use borealis_content::expr::{EquippedExpr, RequirementsExpr, SupportsExpr};
use borealis_content::formula::{self, FormulaContext};
use borealis_content::SelectRule;
use borealis_engine::eval::{
    equipped_holds, feature_name_key, requirement_holds, EquipState, HeldItem, IdCounts, ReqEnv,
};
use borealis_engine::snapshot::SpellcastingSnapshot;
use borealis_engine::universe::Cond;
use std::collections::{HashMap, HashSet};

const CASES: usize = 3000;
const ENVS: usize = 12;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.0 >> 33).unwrap_or(0)
    }

    fn below(&mut self, n: usize) -> usize {
        self.next() % n
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }

    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + i64::try_from(self.below(usize::try_from(hi - lo + 1).unwrap_or(1))).unwrap_or(0)
    }

    fn i32(&mut self, lo: i32, hi: i32) -> i32 {
        i32::try_from(self.int(i64::from(lo), i64::from(hi))).unwrap_or(0)
    }

    fn coin(&mut self) -> bool {
        self.below(2) == 0
    }
}

fn group<R>(r: &mut Rng, depth: usize, row: &dyn Fn(&mut Rng) -> R) -> Node<R> {
    if depth == 0 || r.below(3) == 0 {
        return Node::row(row(r));
    }
    let mode = *r.pick(&[GroupMode::All, GroupMode::Any, GroupMode::None]);
    let n = r.below(4);
    Node::group(mode, (0..n).map(|_| group(r, depth - 1, row)).collect())
}

/// A tree read the way the engine reads it: `None` for a group without rows (it writes nothing).
fn eval_tree<R>(node: &Node<R>, row: &dyn Fn(&R) -> bool) -> Option<bool> {
    match node {
        Node::Row { row: r } => Some(row(r)),
        Node::Group { mode, items } => {
            let vals: Vec<bool> = items.iter().filter_map(|i| eval_tree(i, row)).collect();
            if vals.is_empty() {
                return None;
            }
            Some(match mode {
                GroupMode::All => vals.iter().all(|v| *v),
                GroupMode::Any => vals.iter().any(|v| *v),
                GroupMode::None => !vals.iter().any(|v| *v),
            })
        }
    }
}

// ------------------------------------------------------------------ conditions

const IDS: [&str; 3] = ["ID_AA", "ID_SRD_FEAT_ALERT", "ID_X_Y_2"];
const CLASSES: [&str; 3] = ["warlock", "paladin", "blood hunter"];
const FEATURES: [&str; 4] = ["Fighting Style", "Fighting Style: Archery", "Rage", "7"];
const TYPES: [&str; 2] = ["Spell", "feat"];
const STATS: [&str; 2] = ["innate speed:fly", "foo:bar"];

fn condition_row(r: &mut Rng) -> ConditionRow {
    let ability = *r.pick(&Ability::ALL);
    match r.below(11) {
        0 => ConditionRow::Has {
            id: (*r.pick(&IDS)).into(),
        },
        1 => ConditionRow::Lacks {
            id: (*r.pick(&IDS)).into(),
        },
        2 => ConditionRow::Ability {
            ability,
            min: r.int(8, 18),
        },
        3 => ConditionRow::CharacterLevel { min: r.int(1, 20) },
        4 => ConditionRow::Level { min: r.int(1, 20) },
        5 => ConditionRow::ClassLevel {
            class: (*r.pick(&CLASSES)).into(),
            min: r.int(1, 20),
        },
        6 => ConditionRow::Feature {
            name: (*r.pick(&FEATURES)).into(),
        },
        7 => ConditionRow::Spellcasting { level: r.int(1, 9) },
        8 => ConditionRow::Type {
            value: (*r.pick(&TYPES)).into(),
        },
        9 => ConditionRow::Statistic {
            stat: (*r.pick(&STATS)).into(),
            min: r.int(1, 30),
        },
        _ => ConditionRow::Kept {
            text: "[ranger:archetype:hunter]".into(),
        },
    }
}

struct Env {
    ids: IdCounts,
    abilities: [i32; 6],
    total: i32,
    classes: HashMap<String, i32>,
    stats: HashMap<String, i32>,
    rule_class: Option<i32>,
    types: IdCounts,
    names: IdCounts,
}

fn counts(items: &[String]) -> IdCounts {
    let mut c = IdCounts::default();
    items.iter().for_each(|i| c.add(i));
    c
}

fn env(r: &mut Rng) -> Env {
    let some = |r: &mut Rng, xs: &[&str]| -> Vec<String> {
        xs.iter()
            .filter(|_| r.coin())
            .map(|s| (*s).to_string())
            .collect()
    };
    let mut stats: HashMap<String, i32> = some(r, &STATS)
        .into_iter()
        .map(|s| (s, r.i32(0, 30)))
        .collect();
    for n in 1..=9 {
        if r.below(4) == 0 {
            stats.insert(format!("warlock:spellcasting:slots:{n}"), r.i32(0, 2));
        }
    }
    Env {
        ids: counts(&some(r, &IDS)),
        abilities: std::array::from_fn(|_| r.i32(6, 20)),
        total: r.i32(1, 20),
        classes: some(r, &CLASSES)
            .into_iter()
            .map(|c| (c, r.i32(1, 20)))
            .collect(),
        stats,
        rule_class: r.coin().then(|| r.i32(1, 20)),
        types: counts(
            &some(r, &TYPES)
                .iter()
                .map(|t| t.to_lowercase())
                .collect::<Vec<_>>(),
        ),
        names: counts(
            &some(r, &FEATURES)
                .iter()
                .map(|f| feature_name_key(f))
                .collect::<Vec<_>>(),
        ),
    }
}

/// What each row says it means (the sentence of spec 6.3), independent of the engine's code.
fn row_holds(row: &ConditionRow, e: &Env) -> bool {
    match row {
        ConditionRow::Has { id } => e.ids.count(id) > 0,
        ConditionRow::Lacks { id } => e.ids.count(id) == 0,
        ConditionRow::Ability { ability, min } => i64::from(e.abilities[ability.index()]) >= *min,
        ConditionRow::CharacterLevel { min } => i64::from(e.total) >= *min,
        ConditionRow::Level { min } => i64::from(e.rule_class.unwrap_or(e.total)) >= *min,
        ConditionRow::ClassLevel { class, min } => {
            e.classes.get(class).is_some_and(|l| i64::from(*l) >= *min)
        }
        ConditionRow::Feature { name } => e.names.count(&feature_name_key(name)) > 0,
        ConditionRow::Spellcasting { level } => e.stats.iter().any(|(k, v)| {
            *v > 0
                && k.rsplit_once(':')
                    .is_some_and(|(_, n)| n.parse::<i64>().is_ok_and(|n| n >= *level))
                && k.contains(":spellcasting:slots:")
        }),
        ConditionRow::Type { value } => e.types.count(&value.to_lowercase()) > 0,
        ConditionRow::Statistic { stat, min } => {
            e.stats.get(stat).is_some_and(|v| i64::from(*v) >= *min)
        }
        ConditionRow::Kept { .. } => false,
    }
}

#[test]
fn conditions_mean_what_their_rows_say() {
    let cx = ConditionContext::new(CLASSES);
    let mut r = Rng(0xc0de_0001);
    for _ in 0..CASES {
        let m = group(&mut r, 3, &condition_row);
        assert!(ConditionLanguage::check(&m, &cx).is_empty(), "{m:?}");
        let text = ConditionLanguage::render(&m, &cx);
        let parsed =
            RequirementsExpr::parse(&text).expect("the engine parser reads the builder's text");
        assert_eq!(
            parsed.as_ref().map(|p| p.root().clone()),
            ConditionLanguage::to_bool(&m),
            "{text}"
        );
        assert!(ConditionLanguage::means_same(&m, &text, &cx), "{text}");
        let Built::Model { model: again } = ConditionLanguage::parse(&text, &cx) else {
            panic!("{text}")
        };
        assert_eq!(ConditionLanguage::render(&again, &cx), text, "fixed point");
        let cond = parsed.map_or(Cond::None, Cond::Expr);
        for _ in 0..ENVS {
            let e = env(&mut r);
            let req = ReqEnv {
                ids: &e.ids,
                exclude: None,
                abilities: e.abilities,
                total_level: e.total,
                class_levels: &e.classes,
                stats: &e.stats,
                rule_class_level: e.rule_class,
                stat_used: None,
                types: Some(&e.types),
                names: Some(&e.names),
                spell_blocks: false,
            };
            let want = eval_tree(&m, &|row| row_holds(row, &e)).unwrap_or(true);
            assert_eq!(requirement_holds(&cond, &req), want, "{text}");
        }
    }
}

// ------------------------------------------------------------------ tag filters

const TAGS: [&str; 6] = [
    "Wizard",
    "Evocation",
    "0",
    "1",
    "Spider's Cunning",
    "Senryōka: Path",
];

fn tag_row(r: &mut Rng) -> TagRow {
    match r.below(6) {
        0 | 1 => TagRow::Tag {
            tag: (*r.pick(&TAGS)).into(),
        },
        2 => TagRow::Element {
            id: (*r.pick(&IDS)).into(),
        },
        3 => TagRow::ClassList,
        4 => TagRow::CastableLevel,
        _ => TagRow::Choice {
            name: "Spell List".into(),
        },
    }
}

fn select(supports: &str) -> SelectRule {
    SelectRule {
        ty: "Spell".into(),
        supports: Some(supports.to_string()),
        ..SelectRule::default()
    }
}

#[test]
fn tag_filters_read_back_as_the_engine_reads_them() {
    let mut r = Rng(0xc0de_0002);
    for _ in 0..CASES {
        let m = TagFilter {
            specific: r.coin(),
            root: group(&mut r, 3, &tag_row),
        };
        assert!(TagFilterLanguage::check(&m, &()).is_empty(), "{m:?}");
        let text = TagFilterLanguage::render(&m, &());
        let parsed =
            SupportsExpr::parse(&text).expect("the engine parser reads the builder's text");
        assert_eq!(
            parsed
                .as_ref()
                .map(|p| (p.root().clone(), p.is_id_range_mode())),
            TagFilterLanguage::to_bool(&m),
            "{text}"
        );
        assert!(TagFilterLanguage::means_same(&m, &text, &()), "{text}");
        let Built::Model { model: again } = TagFilterLanguage::parse(&text, &()) else {
            panic!("{text}")
        };
        assert_eq!(TagFilterLanguage::render(&again, &()), text, "fixed point");
        // readers of the raw text answer the same for the `&&` spelling of the same tree
        if let Some(p) = &parsed {
            let other = p.to_string();
            assert_eq!(
                borealis_engine::cantrip_count::counts_toward_cantrips_known(&select(&text)),
                borealis_engine::cantrip_count::counts_toward_cantrips_known(&select(&other)),
                "{text} / {other}"
            );
        }
    }
}

// ------------------------------------------------------------------ equipped

const ITEMS: [&str; 3] = ["id_x_ring", "longsword", "id_x_plate"];

fn equipped_row(r: &mut Rng) -> EquippedRow {
    let item = || SlotTest::Item {
        item: String::new(),
    };
    let pick_test = |r: &mut Rng, slot: Slot| -> SlotTest {
        let tests: Vec<SlotTest> = match slot {
            Slot::Armor => vec![
                SlotTest::Any,
                SlotTest::None,
                SlotTest::Light,
                SlotTest::Medium,
                SlotTest::Heavy,
                item(),
            ],
            Slot::Shield => vec![SlotTest::Any, SlotTest::None, item()],
            Slot::MainHand => vec![
                SlotTest::Any,
                SlotTest::None,
                SlotTest::Weapon,
                SlotTest::Versatile,
                item(),
            ],
            Slot::OffHand => vec![SlotTest::Any, SlotTest::None, SlotTest::Weapon, item()],
        };
        match r.pick(&tests).clone() {
            SlotTest::Item { .. } => SlotTest::Item {
                item: (*r.pick(&ITEMS)).into(),
            },
            t => t,
        }
    };
    match r.below(6) {
        0..=3 => {
            let slot = *r.pick(&[Slot::Armor, Slot::Shield, Slot::MainHand, Slot::OffHand]);
            EquippedRow::Slot {
                slot,
                test: pick_test(r, slot),
            }
        }
        4 => EquippedRow::Attuned {
            item: (*r.pick(&ITEMS)).into(),
        },
        _ => EquippedRow::Wearing {
            item: (*r.pick(&ITEMS)).into(),
        },
    }
}

fn held(r: &mut Rng) -> Option<HeldItem> {
    r.coin().then(|| HeldItem {
        id: (*r.pick(&ITEMS)).into(),
        name: (*r.pick(&ITEMS)).into(),
        armor_kind: (*r.pick(&["light", "medium", "heavy", ""])).into(),
        weapon: r.coin(),
        versatile_damage: r.coin(),
        shield: r.coin(),
    })
}

fn equip_state(r: &mut Rng) -> EquipState {
    let set = |r: &mut Rng| -> HashSet<String> {
        ITEMS
            .iter()
            .filter(|_| r.coin())
            .map(|s| (*s).to_string())
            .collect()
    };
    EquipState {
        armor: held(r),
        primary: held(r),
        secondary: held(r),
        both_hands: r.coin(),
        attuned: set(r),
        equipped: set(r),
        counts_any: r.below(4) == 0,
        counts_kind: std::array::from_fn(|_| r.below(4) == 0),
    }
}

fn slot_holds(slot: Slot, test: &SlotTest, s: &EquipState) -> bool {
    let is = |i: &HeldItem, x: &str| i.id == x || i.name == x;
    let counts = s.counts_any || s.counts_kind.iter().any(|b| *b);
    let shield = s.secondary.as_ref().filter(|i| i.shield);
    let off = s.secondary.as_ref().filter(|i| !i.shield);
    match (slot, test) {
        (Slot::Armor, SlotTest::Any) => s.armor.is_some() || counts,
        (Slot::Armor, SlotTest::None) => s.armor.is_none() && !counts,
        (Slot::Armor, kind @ (SlotTest::Light | SlotTest::Medium | SlotTest::Heavy)) => {
            let (word, i) = match kind {
                SlotTest::Light => ("light", 0),
                SlotTest::Medium => ("medium", 1),
                _ => ("heavy", 2),
            };
            s.armor
                .as_ref()
                .map_or(s.counts_kind[i], |a| a.armor_kind == word)
        }
        (Slot::Armor, SlotTest::Item { item }) => s.armor.as_ref().is_some_and(|a| is(a, item)),
        (Slot::Shield, SlotTest::Any) => shield.is_some(),
        (Slot::Shield, SlotTest::None) => shield.is_none(),
        (Slot::Shield, SlotTest::Item { item }) => shield.is_some_and(|i| is(i, item)),
        (Slot::MainHand, SlotTest::Any) => s.primary.is_some(),
        (Slot::MainHand, SlotTest::None) => s.primary.is_none(),
        (Slot::MainHand, SlotTest::Weapon) => s.primary.as_ref().is_some_and(|i| i.weapon),
        (Slot::MainHand, SlotTest::Versatile) => {
            s.primary.as_ref().is_some_and(|i| i.versatile_damage) && s.both_hands
        }
        (Slot::MainHand, SlotTest::Item { item }) => {
            s.primary.as_ref().is_some_and(|i| is(i, item))
        }
        (Slot::OffHand, SlotTest::Any) => off.is_some(),
        (Slot::OffHand, SlotTest::None) => off.is_none(),
        (Slot::OffHand, SlotTest::Weapon) => off.is_some_and(|i| i.weapon),
        (Slot::OffHand, SlotTest::Item { item }) => off.is_some_and(|i| is(i, item)),
        _ => unreachable!("not generated"),
    }
}

#[test]
fn equipment_tests_mean_what_their_rows_say() {
    let mut r = Rng(0xc0de_0003);
    for _ in 0..CASES {
        let m = group(&mut r, 3, &equipped_row);
        assert!(EquippedLanguage::check(&m, &()).is_empty(), "{m:?}");
        let text = EquippedLanguage::render(&m, &());
        let parsed =
            EquippedExpr::parse(&text).expect("the engine parser reads the builder's text");
        assert_eq!(
            parsed.as_ref().map(|p| p.root().clone()),
            EquippedLanguage::to_bool(&m),
            "{text}"
        );
        assert!(EquippedLanguage::means_same(&m, &text, &()), "{text}");
        let cond = parsed.map_or(Cond::None, Cond::Expr);
        for _ in 0..ENVS {
            let s = equip_state(&mut r);
            let want = eval_tree(&m, &|row| match row {
                EquippedRow::Slot { slot, test } => slot_holds(*slot, test, &s),
                EquippedRow::Attuned { item } => s.attuned.contains(item),
                EquippedRow::Wearing { item } => s.equipped.contains(item),
                EquippedRow::Kept { .. } => false,
            })
            .unwrap_or(true);
            assert_eq!(equipped_holds(&cond, &s), want, "{text}");
        }
    }
}

// ------------------------------------------------------------------ formulas

struct Cx {
    prof: i32,
    mods: [i32; 6],
    classes: HashMap<String, i32>,
}

impl FormulaContext for Cx {
    fn proficiency(&self) -> i32 {
        self.prof
    }
    fn ability_modifier(&self, ability: &str) -> Option<i32> {
        Ability::from_key(ability).map(|a| self.mods[a.index()])
    }
    fn class_level(&self, class: &str) -> i32 {
        self.classes.get(class).copied().unwrap_or(0)
    }
}

const FORMULA_CLASSES: [&str; 3] = ["cleric", "druid", "blood hunter"];

fn some_levels(r: &mut Rng) -> HashMap<String, i32> {
    let mut out = HashMap::new();
    for c in FORMULA_CLASSES {
        if r.coin() {
            out.insert(c.to_string(), r.i32(1, 20));
        }
    }
    out
}

fn contexts(r: &mut Rng) -> Vec<Cx> {
    (0..ENVS)
        .map(|_| Cx {
            prof: r.i32(2, 6),
            mods: std::array::from_fn(|_| r.i32(-2, 5)),
            classes: some_levels(r),
        })
        .collect()
}

/// What a formula model says it is worth (spec 6.3), independent of the parser.
fn value(node: &FormulaNode, cx: &Cx) -> Option<i32> {
    let all = |args: &[FormulaNode]| {
        args.iter()
            .map(|a| value(a, cx))
            .collect::<Option<Vec<_>>>()
    };
    match node {
        FormulaNode::Number { value } => Some(*value),
        FormulaNode::Proficiency => Some(cx.prof),
        FormulaNode::HalfProficiency => Some(cx.prof / 2),
        FormulaNode::Ability { ability } => Some(cx.mods[ability.index()]),
        FormulaNode::ClassLevel { class } => Some(cx.class_level(class)),
        FormulaNode::ClassSaveDc { .. } => None,
        FormulaNode::Binary { op, left, right } => {
            let (a, b) = (value(left, cx)?, value(right, cx)?);
            match op {
                Op::Add => Some(a + b),
                Op::Sub => Some(a - b),
                Op::Mul => Some(a * b),
                Op::Div => a.checked_div(b),
            }
        }
        FormulaNode::Max { args } => all(args)?.into_iter().max(),
        FormulaNode::Min { args } => all(args)?.into_iter().min(),
        FormulaNode::Floor { arg } => value(arg, cx),
        FormulaNode::CeilDiv { num, den } => {
            let (a, b) = (value(num, cx)?, value(den, cx)?);
            (b > 0).then(|| (a + b - 1) / b)
        }
    }
}

fn leaf(r: &mut Rng) -> FormulaNode {
    match r.below(5) {
        0 => FormulaNode::number(r.i32(0, 12)),
        1 => FormulaNode::number(r.i32(-5, -1)),
        2 => FormulaNode::Proficiency,
        3 => FormulaNode::ability(*r.pick(&Ability::ALL)),
        _ => FormulaNode::class_level(r.pick(&FORMULA_CLASSES)),
    }
}

fn formula_node(r: &mut Rng, depth: usize) -> FormulaNode {
    if depth == 0 || r.below(4) == 0 {
        return leaf(r);
    }
    let sub = |r: &mut Rng| Box::new(formula_node(r, depth - 1));
    match r.below(7) {
        0..=2 => FormulaNode::Binary {
            op: *r.pick(&[Op::Add, Op::Sub, Op::Mul, Op::Div]),
            left: sub(r),
            right: sub(r),
        },
        3 => FormulaNode::Max {
            args: (0..=r.below(3)).map(|_| *sub(r)).collect(),
        },
        4 => FormulaNode::Min {
            args: (0..=r.below(3)).map(|_| *sub(r)).collect(),
        },
        5 => FormulaNode::Floor { arg: sub(r) },
        _ => FormulaNode::CeilDiv {
            num: sub(r),
            den: sub(r),
        },
    }
}

#[test]
fn formulas_read_back_by_the_shared_grammar() {
    let d = FormulaDialect::Formula;
    let mut r = Rng(0xc0de_0004);
    for _ in 0..CASES * 3 {
        let m = formula_node(&mut r, 4);
        assert!(FormulaLanguage::check(&m, &d).is_empty(), "{m:?}");
        let text = FormulaLanguage::render(&m, &d);
        let parsed = formula::parse(&text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(
            FormulaLanguage::parse(&text, &d),
            Built::Model { model: m.clone() },
            "{text}"
        );
        for cx in contexts(&mut r) {
            assert_eq!(parsed.eval(&cx), value(&m, &cx), "{text}");
        }
    }
}

/// Canonical text never changes what an existing formula is worth: generated texts (the WB-01 alphabet) that read
/// into a model are worth the same written canonically.
#[test]
fn canonical_text_keeps_the_value_of_any_readable_formula() {
    const PARTS: [&str; 20] = [
        "1",
        "2",
        "-3",
        "0",
        " ",
        "+",
        "-",
        "*",
        "/",
        "(",
        ")",
        ",",
        "max(",
        "min(",
        "floor(",
        "ceil(",
        "proficiency",
        "wis:modifier",
        "cleric level",
        "druid level",
    ];
    let d = FormulaDialect::Formula;
    let mut r = Rng(0xc0de_0005);
    let mut read = 0;
    for _ in 0..40_000 {
        let len = 1 + r.below(9);
        let text: String = (0..len).map(|_| *r.pick(&PARTS)).collect();
        let Built::Model { model } = FormulaLanguage::parse(&text, &d) else {
            continue;
        };
        read += 1;
        let canonical = FormulaLanguage::render(&model, &d);
        assert_eq!(
            FormulaLanguage::parse(&canonical, &d),
            Built::Model {
                model: model.clone()
            },
            "{text} -> {canonical}"
        );
        let (a, b) = (
            formula::parse(&text).unwrap(),
            formula::parse(&canonical).unwrap(),
        );
        for cx in contexts(&mut r) {
            assert_eq!(a.eval(&cx), b.eval(&cx), "{text} -> {canonical}");
        }
    }
    assert!(read > 2000, "{read}");
}

fn dc_sum(r: &mut Rng) -> FormulaNode {
    let terms = (0..=r.below(4))
        .map(|_| match r.below(3) {
            0 => FormulaNode::number(r.i32(0, 12)),
            1 => FormulaNode::Proficiency,
            _ => FormulaNode::ability(*r.pick(&Ability::ALL)),
        })
        .collect();
    FormulaNode::sum(terms).expect("one term or more")
}

fn caster(name: &str, dc: i32) -> SpellcastingSnapshot {
    serde_json::from_value(serde_json::json!({
        "name": name, "ability": "wisdom", "prepare": true, "allow_replace": false, "dc": dc, "attack": 5,
        "slots": [0, 0, 0, 0, 0, 0, 0, 0, 0], "pact": null, "prepare_limit": null, "prepared_count": 0,
        "spells": [], "prepare_from_list": false,
    }))
    .expect("a spellcasting snapshot")
}

#[test]
fn save_dcs_read_back_by_their_readers() {
    use borealis_engine::{
        breath::dc_formula_text, notes_d::eval_note_dc, spell_rules::eval_save_dc_formula,
    };
    let mut r = Rng(0xc0de_0006);
    for _ in 0..CASES {
        let m = dc_sum(&mut r);
        for d in [
            FormulaDialect::SaveDc,
            FormulaDialect::BreathDc,
            FormulaDialect::NoteDc,
        ] {
            assert!(FormulaLanguage::check(&m, &d).is_empty(), "{m:?}");
            let text = FormulaLanguage::render(&m, &d);
            assert_eq!(
                FormulaLanguage::parse(&text, &d),
                Built::Model { model: m.clone() },
                "{d:?} {text}"
            );
            for cx in contexts(&mut r) {
                let want = value(&m, &cx);
                let got = match d {
                    FormulaDialect::SaveDc => {
                        // the save-DC subset: both the save-DC reader and the shared grammar read it
                        let shared = formula::parse(&text).ok().and_then(|f| f.eval(&cx));
                        assert_eq!(shared, want, "shared grammar: {text}");
                        eval_save_dc_formula(&text, &cx.mods, cx.prof, &[])
                    }
                    FormulaDialect::BreathDc => formula::parse(&dc_formula_text(&text))
                        .ok()
                        .and_then(|f| f.eval(&cx)),
                    _ => eval_note_dc(&text, &cx.mods, cx.prof),
                };
                assert_eq!(got, want, "{d:?} {text}");
            }
        }
    }
    let class_dc = FormulaNode::ClassSaveDc {
        class: "Blood Hunter".into(),
    };
    let text = FormulaLanguage::render(&class_dc, &FormulaDialect::SaveDc);
    assert_eq!(text, "spell save DC (Blood Hunter)");
    let casters = [caster("Cleric", 13), caster("Blood Hunter", 15)];
    assert_eq!(eval_save_dc_formula(&text, &[0; 6], 2, &casters), Some(15));
}

#[test]
fn values_read_back_by_their_readers() {
    use borealis_engine::{combat::reduction_term, rule_view::eval_value};
    let mut r = Rng(0xc0de_0007);
    for _ in 0..CASES {
        let ability = FormulaNode::ability(*r.pick(&Ability::ALL));
        let number = FormulaNode::number(r.i32(-5, 20));
        let class = FormulaNode::class_level(r.pick(&FORMULA_CLASSES));
        let v = r
            .pick(&[
                number.clone(),
                FormulaNode::Proficiency,
                FormulaNode::HalfProficiency,
                ability.clone(),
            ])
            .clone();
        let d = FormulaDialect::Value;
        assert!(FormulaLanguage::check(&v, &d).is_empty(), "{v:?}");
        let text = FormulaLanguage::render(&v, &d);
        assert_eq!(
            FormulaLanguage::parse(&text, &d),
            Built::Model { model: v.clone() },
            "{text}"
        );
        let t = r.pick(&[number, ability, class]).clone();
        let d2 = FormulaDialect::ValueOrClassLevel;
        assert!(FormulaLanguage::check(&t, &d2).is_empty(), "{t:?}");
        let text2 = FormulaLanguage::render(&t, &d2);
        assert_eq!(
            FormulaLanguage::parse(&text2, &d2),
            Built::Model { model: t.clone() },
            "{text2}"
        );
        for cx in contexts(&mut r) {
            assert_eq!(
                eval_value(&text, &cx.mods, cx.prof),
                value(&v, &cx),
                "{text}"
            );
            assert_eq!(
                Some(reduction_term(&text2, &cx.mods, &cx.classes)),
                value(&t, &cx),
                "{text2}"
            );
        }
    }
}
