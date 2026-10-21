//! Engine behaviour on small synthetic content layered over the bundled core pack
//! (no external fixtures needed).

use borealis_content::{ContentFile, ContentStoreBuilder, PackOrigin, Ruleset};
use borealis_engine::commands::{self, slot_key};
use borealis_engine::snapshot::CharacterSnapshot;
use borealis_engine::state::{CharacterState, Choice, SlotKey};
use borealis_engine::Engine;
use serde_json::{json, Value};
use std::path::Path;

fn engine_with(extra: Vec<Value>) -> Engine {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": extra}),
    )
    .expect("synthetic content");
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .expect("core pack")
        .add_file("synthetic.json", PackOrigin::Core, file)
        .build();
    Engine::new(&store)
}

fn hero(e: &Engine) -> CharacterState {
    commands::new_character(e, Ruleset::Dnd2014)
}

fn pick(
    e: &Engine,
    s: &CharacterState,
    owner: &str,
    name: &str,
    ord: u32,
    id: &str,
) -> CharacterState {
    let snap = e.derive(s);
    let key = slot_key(&snap, owner, name, ord).unwrap_or_else(|| panic!("no slot {owner}/{name}"));
    commands::select(e, s, &key, Choice::Element(id.into()))
        .unwrap_or_else(|err| panic!("{err}"))
        .state
}

fn caster_class(
    id: &str,
    name: &str,
    slots_kind: &str,
    ability: &str,
    extra_rules: Vec<Value>,
) -> Vec<Value> {
    let mc = format!("{id}_MC");
    let feature = format!("{id}_CASTING");
    let mut rules = vec![
        json!({"kind":"grant","id": feature,"type":"Class Feature","level":1}),
        json!({"kind":"grant","id": format!("ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_{slots_kind}"),"type":"Grants","requirements":"ID_INTERNAL_GRANT_MULTICLASS"}),
    ];
    rules.extend(extra_rules);
    vec![
        json!({"id": id, "name": name, "type":"Class", "source":"Test", "setters":[{"name":"hd","value":"d8"}], "rules": rules,
               "multiclass":{"id": mc, "requirements": format!("[{}:13]", &ability[..3].to_lowercase()), "rules":[{"kind":"grant","id":"ID_INTERNAL_GRANT_MULTICLASS","type":"Grants"}]}}),
        json!({"id": feature, "name": format!("{name} Spellcasting"), "type":"Class Feature", "source":"Test",
               "spellcasting":{"name": name, "ability": ability, "prepare": true},
               "rules":[{"kind":"stat","name": format!("{}:spellcasting:prepare", name.to_lowercase()), "value": format!("{}:modifier", ability.to_lowercase())}]}),
    ]
}

fn start_second(e: &Engine, s: &CharacterState, mc: &str) -> CharacterState {
    let s = commands::level_up_new_multiclass(e, s, None)
        .expect("new multiclass")
        .state;
    let n = s.level();
    pick(
        e,
        &s,
        &format!("ID_LEVEL_{n}"),
        &format!("Multiclass (Level {n})"),
        1,
        mc,
    )
}

#[test]
fn third_casters_and_pooled_slots_e12() {
    let mut content = caster_class("ID_T_ARCHER", "Archer", "THIRD", "Wisdom", vec![]);
    content.extend(caster_class(
        "ID_T_MAGE",
        "Mage",
        "FULL",
        "Intelligence",
        vec![],
    ));
    let e = engine_with(content);
    let mut s = hero(&e);
    s.abilities.base = [10, 10, 10, 14, 14, 10];
    s = pick(&e, &s, "ID_LEVEL_1", "Class", 1, "ID_T_ARCHER");
    for _ in 0..5 {
        s = commands::level_up_main(&e, &s, None).unwrap().state;
    }
    s = start_second(&e, &s, "ID_T_MAGE_MC");
    for _ in 0..3 {
        s = commands::level_up_multiclass(&e, &s, 7, None)
            .unwrap()
            .state;
    }
    let snap = e.derive(&s);
    let pool = snap.multiclass_slots.clone().expect("two slot sources");
    // archer 6 -> 2 (third), mage 4 -> 4 : caster level 6 -> 4/3/3
    assert_eq!(pool.caster_level, 6);
    assert_eq!(&pool.slots[..4], &[4, 3, 3, 0]);
    assert_eq!(
        snap.stat("multiclass:spellcasting:level"),
        6,
        "rule based group agrees with the pure computation"
    );
}

#[test]
fn prepared_limit_is_at_least_one_e13() {
    let e = engine_with(caster_class(
        "ID_T_ARCHER",
        "Archer",
        "THIRD",
        "Wisdom",
        vec![],
    ));
    let mut s = hero(&e);
    s.abilities.base = [10, 10, 10, 10, 8, 10];
    s = pick(&e, &s, "ID_LEVEL_1", "Class", 1, "ID_T_ARCHER");
    let snap = e.derive(&s);
    let c = snap.caster("Archer").unwrap();
    assert_eq!(snap.stat("archer:spellcasting:prepare"), -1);
    assert_eq!(c.prepare_limit, Some(1));
    assert!(snap
        .diagnostics
        .iter()
        .any(|d| d.code == "legacy.prepare_min_one"));
}

#[test]
fn owner_exclusion_semantics_e7() {
    // A parent's own grant must not satisfy another of its grants' requirements, but a copied
    // class rule inside a Multiclass element does see the multiclass node's marker grant.
    let mut content = caster_class("ID_T_MAGE", "Mage", "FULL", "Intelligence", vec![]);
    content.extend(caster_class(
        "ID_T_ARCHER",
        "Archer",
        "THIRD",
        "Wisdom",
        vec![],
    ));
    content.push(json!({"id":"ID_T_SELF_BLOCK","name":"Self Block","type":"Class Feature","source":"Test","rules":[
        {"kind":"grant","id":"ID_T_MARK","type":"Grants"},
        {"kind":"grant","id":"ID_T_NEEDS_MARK","type":"Grants","requirements":"ID_T_MARK"}]}));
    content.push(json!({"id":"ID_T_MARK","name":"Mark","type":"Grants","source":"Test"}));
    content
        .push(json!({"id":"ID_T_NEEDS_MARK","name":"Needs Mark","type":"Grants","source":"Test"}));
    content.push(json!({"id":"ID_T_OPT","name":"Opt","type":"Option","source":"Test","rules":[{"kind":"grant","id":"ID_T_SELF_BLOCK","type":"Class Feature"}]}));
    let e = engine_with(content);
    let mut s = hero(&e);
    s.options.push("ID_T_OPT".into());
    let snap = e.derive(&s);
    assert!(snap.has_element("ID_T_MARK"));
    assert!(
        !snap.has_element("ID_T_NEEDS_MARK"),
        "owner children are hidden from the owner's own rule requirements"
    );
    // two casters: the Archer's copied FULL/THIRD grants depend on the multiclass marker
    s.abilities.base = [10, 10, 10, 14, 14, 10];
    let mut s2 = pick(&e, &s, "ID_LEVEL_1", "Class", 1, "ID_T_MAGE");
    s2 = commands::level_up_main(&e, &s2, None).unwrap().state;
    s2 = start_second(&e, &s2, "ID_T_ARCHER_MC");
    s2 = commands::level_up_multiclass(&e, &s2, 3, None)
        .unwrap()
        .state;
    let snap = e.derive(&s2);
    assert!(snap.has_element("ID_INTERNAL_GRANT_MULTICLASS"));
    assert!(snap.has_element("ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_THIRD"));
    assert!(snap.multiclass_slots.is_some());
}

#[test]
fn workaround_grants_are_stable_and_never_double_count() {
    let e = engine_with(vec![
        json!({"id":"ID_T_SKILL_FEATURE","name":"Skill Feature","type":"Class Feature","source":"Test","rules":[
        {"kind":"grant","id":"ID_INTERNAL_GRANTS_PROFICIENCY_SKILL_OR_DOUBLE_STEALTH","type":"Grants"}]}),
        json!({"id":"ID_T_OPT","name":"Opt","type":"Option","source":"Test","rules":[{"kind":"grant","id":"ID_T_SKILL_FEATURE","type":"Class Feature"}]}),
    ]);
    let mut s = hero(&e);
    s.options.push("ID_T_OPT".into());
    s.abilities.base = [10, 14, 10, 10, 10, 10];
    let snap = e.derive(&s);
    assert!(snap.skill("stealth").unwrap().proficient && !snap.skill("stealth").unwrap().expertise);
    assert_eq!(snap.skill("stealth").unwrap().total, 2 + 2);
    assert!(snap
        .diagnostics
        .iter()
        .all(|d| d.code != "eval.not_converged"));
}

#[test]
fn stat_cycles_are_reported() {
    let e = engine_with(vec![
        json!({"id":"ID_T_OPT","name":"Cycle","type":"Option","source":"Test","rules":[
        {"kind":"stat","name":"x","value":"y"},{"kind":"stat","name":"y","value":"x"},{"kind":"stat","name":"y","value":"1"}]}),
    ]);
    let mut s = hero(&e);
    s.options.push("ID_T_OPT".into());
    let snap = e.derive(&s);
    assert!(snap.diagnostics.iter().any(|d| d.code == "stat.cycle"));
}

#[test]
fn list_selects_defaults_and_optional_slots() {
    let e = engine_with(vec![
        json!({"id":"ID_T_BG","name":"Hermit","type":"Background","source":"Test","rules":[
            {"kind":"select","type":"List","name":"Personality Trait","items":[{"id":1,"text":"calm"},{"id":2,"text":"loud"}]},
            {"kind":"select","type":"Proficiency","name":"Pick Skill","supports":"Skill","default":"ID_PROFICIENCY_SKILL_NATURE"},
            {"kind":"select","type":"Language","name":"Optional Language","optional":true}]}),
    ]);
    let mut s = hero(&e);
    s = pick(&e, &s, "ID_LEVEL_1", "Background", 1, "ID_T_BG");
    let snap = e.derive(&s);
    let list = snap.slot("ID_T_BG", "Personality Trait").unwrap();
    assert!(list.requires_selection && list.list_items.len() == 2);
    let opt = snap.slot("ID_T_BG", "Optional Language").unwrap();
    assert!(!opt.requires_selection && opt.optional);
    let key = list.key.clone();
    s = commands::select(&e, &s, &key, Choice::ListItem(2))
        .unwrap()
        .state;
    assert_eq!(
        e.derive(&s)
            .slot("ID_T_BG", "Personality Trait")
            .unwrap()
            .list_choice,
        Some(2)
    );
    assert!(commands::select(&e, &s, &key, Choice::ListItem(9)).is_err());
    // defaults fill the skill slot
    let filled = commands::apply_defaults(&e, &s);
    let snap = e.derive(&filled);
    assert!(snap.skill("nature").unwrap().proficient);
    assert!(snap.slot("ID_T_BG", "Pick Skill").unwrap().chosen.is_some());
}

#[test]
fn command_errors_are_specific() {
    let e = engine_with(vec![]);
    let s = hero(&e);
    let bogus = SlotKey::new("ID_NOPE", 0, 1);
    assert!(matches!(
        commands::select(&e, &s, &bogus, Choice::Element("ID_X".into())),
        Err(commands::CommandError::UnknownSlot)
    ));
    let snap = e.derive(&s);
    let race = slot_key(&snap, "ID_LEVEL_1", "Race", 1).unwrap();
    assert!(matches!(
        commands::select(&e, &s, &race, Choice::Element("ID_NOT_THERE".into())),
        Err(commands::CommandError::UnknownElement(_))
    ));
    assert!(matches!(
        commands::select(&e, &s, &race, Choice::Element("ID_LEVEL_2".into())),
        Err(commands::CommandError::NotAllowed(_))
    ));
    assert!(commands::select(&e, &s, &race, Choice::ListItem(1)).is_err());
    assert!(commands::level_down(&e, &s).is_err());
    assert!(commands::set_option(&e, &s, "ID_NOT_AN_OPTION", true).is_err());
    assert!(commands::set_hp_roll(&s, &borealis_engine::state::ClassSlot::Main, 0, 5).is_err());
    assert!(commands::set_replace_level(&s, &race, 3).is_err());
    assert!(commands::remove_item(&e, &s, "nope").is_err());
}

#[test]
fn levels_experience_and_hp_rolls() {
    let e = engine_with(vec![
        json!({"id":"ID_T_CLASS","name":"Soldier","type":"Class","source":"Test","setters":[{"name":"hd","value":"d10"}]}),
    ]);
    let mut s = hero(&e);
    s.abilities.base = [10, 10, 14, 10, 10, 10];
    s = pick(&e, &s, "ID_LEVEL_1", "Class", 1, "ID_T_CLASS");
    s.options
        .retain(|o| o != "ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP");
    s = commands::level_up_main(&e, &s, Some(7)).unwrap().state;
    s = commands::level_up_main(&e, &s, Some(3)).unwrap().state;
    assert_eq!(s.experience, 900, "XP: level 3 threshold");
    let snap = e.derive(&s);
    assert_eq!(snap.hit_points, 10 + 7 + 3 + 3 * 2);
    assert!(snap.level_up_available == (900 >= snap.next_level_xp.unwrap()));
    assert_eq!(snap.next_level_xp, Some(2700));
    s = commands::set_hp_roll(&s, &borealis_engine::state::ClassSlot::Main, 2, 9)
        .unwrap()
        .state;
    assert_eq!(e.derive(&s).hit_points, 10 + 9 + 3 + 6);
    s = commands::set_experience(&s, 5000);
    assert!(e.derive(&s).level_up_available, "Prompt, not auto level");
    assert_eq!(s.level(), 3);
    let down = commands::level_down(&e, &s).unwrap().state;
    assert_eq!(
        (down.level(), down.experience),
        (2, 300),
        "XP becomes the new level threshold"
    );
    assert_eq!(down.hp_rolls[0].rolls.len(), 2);
    let mut max = hero(&e);
    max.levels = vec![borealis_engine::state::ClassSlot::Main; 20];
    assert!(commands::level_up_main(&e, &max, None).is_err());
}

#[test]
fn source_restriction_hides_options_but_keeps_choices() {
    let e = engine_with(vec![
        json!({"id":"ID_T_R1","name":"Alpha","type":"Race","source":"Book A"}),
        json!({"id":"ID_T_R2","name":"Beta","type":"Race","source":"Book B"}),
    ]);
    let mut s = hero(&e);
    s = pick(&e, &s, "ID_LEVEL_1", "Race", 1, "ID_T_R1");
    s.restricted_sources.push("Book A".into());
    s.restricted_sources.push("Book B".into());
    let snap = e.derive(&s);
    let slot = snap.slot("ID_LEVEL_1", "Race").unwrap();
    assert_eq!(
        slot.chosen.as_ref().unwrap().id,
        "ID_T_R1",
        "an existing choice stays"
    );
    assert!(
        slot.options.iter().all(|o| o.id != "ID_T_R2"),
        "restricted sources leave the option list"
    );
    let again = commands::select(&e, &s, &slot.key, Choice::Element("ID_T_R2".into()));
    assert!(again.is_err());
}

#[test]
fn ruleset_filter_and_proficiency_table_hook() {
    let e = engine_with(vec![
        json!({"id":"ID_T_OLD","name":"Old Race","type":"Race","source":"Test","ruleset":"dnd2014"}),
        json!({"id":"ID_T_NEW","name":"New Species","type":"Race","source":"Test","ruleset":"dnd2024"}),
    ]);
    let s14 = hero(&e);
    let names14: Vec<String> = e
        .derive(&s14)
        .slot("ID_LEVEL_1", "Race")
        .unwrap()
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect();
    assert!(
        names14.contains(&"ID_T_OLD".to_string()) && !names14.contains(&"ID_T_NEW".to_string())
    );
    let mut s24 = hero(&e);
    s24.ruleset = Ruleset::Dnd2024;
    // 2024 default is open to 2014 content (flagged, see the cross-edition tests); strict hides it
    s24.allow_cross_edition = Some(false);
    let snap = e.derive(&s24);
    let names24: Vec<String> = snap
        .slot("ID_LEVEL_1", "Race")
        .unwrap()
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect();
    assert!(
        names24.contains(&"ID_T_NEW".to_string()) && !names24.contains(&"ID_T_OLD".to_string())
    );
    // 2024 profile: the proficiency bonus comes from the profile table, not the Level elements
    assert_eq!(snap.proficiency_bonus, 2);
    s24.levels = vec![borealis_engine::state::ClassSlot::Main; 9];
    assert_eq!(e.derive(&s24).proficiency_bonus, 4);
    let mut s14b = s14.clone();
    s14b.levels = vec![borealis_engine::state::ClassSlot::Main; 9];
    assert_eq!(
        e.derive(&s14b).proficiency_bonus,
        4,
        "content rules and table agree"
    );
}

#[test]
fn items_with_select_rules_use_instance_keys() {
    let e = engine_with(vec![
        json!({"id":"ID_T_WAND","name":"Wand of Choices","type":"Magic Item","source":"Test","setters":[{"name":"slot","value":"onehand"}],
               "rules":[{"kind":"select","type":"Proficiency","name":"Wand Skill","supports":"Skill"}]}),
    ]);
    let mut s = hero(&e);
    s = commands::add_item(&e, &s, "ID_T_WAND", 1).unwrap().state;
    s = commands::add_item(&e, &s, "ID_T_WAND", 1).unwrap().state;
    let (a, b) = (
        s.inventory.items[0].uid.clone(),
        s.inventory.items[1].uid.clone(),
    );
    s = commands::equip(&e, &s, &a, None).unwrap().state;
    s = commands::equip(&e, &s, &b, None).unwrap().state;
    let snap = e.derive(&s);
    let slots: Vec<&SlotKey> = snap
        .slots
        .iter()
        .filter(|x| x.owner.id == "ID_T_WAND")
        .map(|x| &x.key)
        .collect();
    assert_eq!(slots.len(), 2);
    assert_ne!(slots[0].instance, slots[1].instance);
    let key = slots[0].clone();
    s = commands::select(
        &e,
        &s,
        &key,
        Choice::Element("ID_PROFICIENCY_SKILL_ARCANA".into()),
    )
    .unwrap()
    .state;
    assert!(e.derive(&s).skill("arcana").unwrap().proficient);
    s = commands::unequip(&e, &s, &a).unwrap().state;
    assert!(
        s.selections
            .iter()
            .all(|x| x.slot.instance.as_deref() != Some(a.as_str())),
        "selections of the removed instance are pruned"
    );
    assert!(!e.derive(&s).skill("arcana").unwrap().proficient);
}

fn snapshot_json_round_trip(snap: &CharacterSnapshot) {
    let text = serde_json::to_string(snap).unwrap();
    let back: CharacterSnapshot = serde_json::from_str(&text).unwrap();
    assert_eq!(&back, snap);
}

#[test]
fn snapshot_and_state_serialize() {
    let e = engine_with(vec![]);
    let s = hero(&e);
    snapshot_json_round_trip(&e.derive(&s));
    let text = serde_json::to_string(&s).unwrap();
    assert_eq!(serde_json::from_str::<CharacterState>(&text).unwrap(), s);
    let key = SlotKey::new("OWNER", 3, 2);
    assert_eq!(
        serde_json::from_str::<SlotKey>(&serde_json::to_string(&key).unwrap()).unwrap(),
        key
    );
}

#[test]
fn missing_level_and_option_content_is_diagnosed() {
    let e = engine_with(vec![]);
    let mut s = hero(&e);
    s.options.push("ID_NOT_INSTALLED".into());
    s.selections.push(borealis_engine::state::Selection {
        slot: SlotKey::new("ID_LEVEL_1", 0, 1),
        choice: Choice::Element("ID_GONE".into()),
        replace_level: None,
    });
    let snap = e.derive(&s);
    assert!(snap
        .diagnostics
        .iter()
        .any(|d| d.code == "selection.missing_content"));
}

#[test]
fn engine_is_shareable_between_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Engine>();
    assert_send_sync::<CharacterSnapshot>();
    assert_send_sync::<CharacterState>();
}

#[test]
fn profile_info_is_exposed() {
    let e = engine_with(vec![]);
    let mut s = hero(&e);
    let a = e.derive(&s);
    assert!(!a.profile.feat_at_level_1);
    assert_eq!(
        a.profile.half_caster_rounding,
        borealis_engine::CasterRounding::Down
    );
    s.ruleset = Ruleset::Dnd2024;
    let b = e.derive(&s);
    assert!(b.profile.feat_at_level_1);
    assert_eq!(
        b.profile.half_caster_rounding,
        borealis_engine::CasterRounding::Up
    );
    assert_eq!(b.profile.default_subclass_level, Some(3));
}

#[test]
fn legacy_stat_chain_note_for_long_reverse_chains() {
    let rules: Vec<Value> = [
        ("a", "b"),
        ("b", "c"),
        ("c", "d"),
        ("d", "e"),
        ("e", "strength:modifier"),
    ]
    .iter()
    .map(|(n, v)| json!({"kind":"stat","name": n, "value": v, "bonus": "chain"}))
    .collect();
    let e = engine_with(vec![
        json!({"id":"ID_T_OPT","name":"Chain","type":"Option","source":"Test","rules": rules}),
    ]);
    let mut s = hero(&e);
    s.options.push("ID_T_OPT".into());
    let snap = e.derive(&s);
    assert_eq!(snap.stat("a"), 0);
    assert!(
        snap.diagnostics
            .iter()
            .any(|d| d.code == "legacy.stat_chain_depth"),
        "{:?}",
        snap.diagnostics
    );
    // a short chain does not trigger the note
    let e = engine_with(vec![
        json!({"id":"ID_T_OPT","name":"Chain","type":"Option","source":"Test","rules":[
        {"kind":"stat","name":"a","value":"b","bonus":"x"},{"kind":"stat","name":"b","value":"strength:modifier","bonus":"x"}]}),
    ]);
    let mut s = hero(&e);
    s.options.push("ID_T_OPT".into());
    assert!(e
        .derive(&s)
        .diagnostics
        .iter()
        .all(|d| d.code != "legacy.stat_chain_depth"));
}

#[test]
fn unparsable_expressions_never_hold_and_are_reported() {
    let e = engine_with(vec![
        json!({"id":"ID_T_OPT","name":"Broken","type":"Option","source":"Test","rules":[
        {"kind":"grant","id":"ID_INTERNAL_GRANTS_DRAGONMARK","type":"Grants","requirements":"[bogus"},
        {"kind":"stat","name":"speed:misc","value":"5","equipped":"[armor:"}]}),
    ]);
    let mut s = hero(&e);
    s.options.push("ID_T_OPT".into());
    let snap = e.derive(&s);
    assert!(!snap.has_element("ID_INTERNAL_GRANTS_DRAGONMARK"));
    assert!(snap
        .diagnostics
        .iter()
        .any(|d| d.code == "content.bad_expression" && d.element.as_deref() == Some("ID_T_OPT")));
    assert_eq!(snap.speed.walk, 0);
}

#[test]
fn species_fill_the_race_slot() {
    let e = engine_with(vec![
        json!({"id":"ID_T_SPECIES","name":"Skyfolk","type":"Species","source":"Test","ruleset":"dnd2024"}),
    ]);
    let mut s = hero(&e);
    s.ruleset = Ruleset::Dnd2024;
    s = pick(&e, &s, "ID_LEVEL_1", "Race", 1, "ID_T_SPECIES");
    let snap = e.derive(&s);
    assert_eq!(snap.origin.get("Species").unwrap().id, "ID_T_SPECIES");
    assert_eq!(
        snap.slot("ID_LEVEL_1", "Race")
            .unwrap()
            .chosen
            .as_ref()
            .unwrap()
            .ty,
        "Species"
    );
}

#[test]
fn dnd2024_reports_no_unimplemented_areas_left() {
    let e = engine_with(vec![]);
    let s14 = hero(&e);
    assert!(!e
        .derive(&s14)
        .diagnostics
        .iter()
        .any(|d| d.code == "profile.unimplemented"));
    let mut s24 = hero(&e);
    s24.ruleset = Ruleset::Dnd2024;
    let snap = e.derive(&s24);
    let msgs: Vec<&str> = snap
        .diagnostics
        .iter()
        .filter(|d| d.code == "profile.unimplemented")
        .map(|d| d.message.as_str())
        .collect();
    assert!(msgs.is_empty(), "every named area is implemented: {msgs:?}");
    assert!(
        !msgs.iter().any(|m| m.contains("spell_preparation")),
        "spell preparation is implemented"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("feat_categories")),
        "feat categories are implemented"
    );
}

#[test]
fn point_buy_budget_override_beats_the_profile_default() {
    let e = engine_with(vec![]);
    let mut s = hero(&e);
    s = commands::set_ability_method(&s, borealis_engine::state::AbilityMethod::PointBuy);
    assert_eq!(e.derive(&s).point_buy.unwrap().budget, 27);
    s.house_rules.point_buy_budget = Some(30);
    assert_eq!(e.derive(&s).point_buy.unwrap().budget, 30);
}

fn race_ids(snap: &CharacterSnapshot) -> Vec<String> {
    snap.slot("ID_LEVEL_1", "Race")
        .unwrap()
        .options
        .iter()
        .map(|o| o.id.clone())
        .collect()
}

fn legacy_race_content() -> Vec<Value> {
    vec![
        json!({"id":"ID_T_ELF14","name":"Old Elf","type":"Race","source":"Test","ruleset":"dnd2014",
               "rules":[{"kind":"stat","name":"dexterity","value":"2"},
                        {"kind":"stat","name":"speed","value":"30"}]}),
        json!({"id":"ID_T_SPECIES24","name":"New Species","type":"Species","source":"Test","ruleset":"dnd2024"}),
        json!({"id":"ID_T_CLASS24","name":"New Class","type":"Class","source":"Test","ruleset":"dnd2024",
               "setters":[{"name":"hd","value":"d8"}]}),
    ]
}

fn class_ids(snap: &CharacterSnapshot) -> Vec<String> {
    snap.slots
        .iter()
        .filter(|s| s.ty == "Class")
        .flat_map(|s| s.options.iter().map(|o| o.id.clone()))
        .collect()
}

#[test]
fn legacy_race_in_a_2024_character_is_offered_flagged_and_converted() {
    let e = engine_with(legacy_race_content());
    let mut s = hero(&e);
    s.ruleset = Ruleset::Dnd2024;
    let snap = e.derive(&s);
    assert!(race_ids(&snap).contains(&"ID_T_ELF14".to_string()));
    let opt = snap
        .slot("ID_LEVEL_1", "Race")
        .unwrap()
        .options
        .iter()
        .find(|o| o.id == "ID_T_ELF14")
        .unwrap();
    let badge = opt.edition.as_ref().expect("flagged as foreign");
    assert!(badge.converted && badge.from == Ruleset::Dnd2014);
    assert!(badge.notes.iter().any(|n| n.contains("dexterity 2")));
    assert!(snap
        .slot("ID_LEVEL_1", "Race")
        .unwrap()
        .options
        .iter()
        .all(|o| o.id == "ID_T_ELF14" || o.edition.is_none()));

    s = pick(&e, &s, "ID_LEVEL_1", "Race", 1, "ID_T_ELF14");
    let snap = e.derive(&s);
    assert_eq!(snap.compatibility.entries.len(), 1);
    let entry = &snap.compatibility.entries[0];
    assert_eq!(entry.element.id, "ID_T_ELF14");
    assert!(entry.converted && !snap.compatibility.strict);
    // converted: the racial +2 is gone, the other rules stay
    let dex_sources = snap.abilities[1].score;
    let mut plain = hero(&e);
    plain.ruleset = Ruleset::Dnd2024;
    assert_eq!(dex_sources, e.derive(&plain).abilities[1].score);
    assert_eq!(snap.stat("speed"), 30);

    // the same race in its own edition keeps the increase
    let mut s14 = hero(&e);
    s14 = pick(&e, &s14, "ID_LEVEL_1", "Race", 1, "ID_T_ELF14");
    let snap14 = e.derive(&s14);
    assert_eq!(snap14.abilities[1].score, dex_sources + 2);
    assert!(snap14.compatibility.entries.is_empty());
}

#[test]
fn unchanged_foreign_content_is_reported_as_used_as_written() {
    let e = engine_with(vec![
        json!({"id":"ID_T_BG14","name":"Old Folk","type":"Background","source":"Test","ruleset":"dnd2014"}),
    ]);
    let mut s = hero(&e);
    s.ruleset = Ruleset::Dnd2024;
    s = pick(&e, &s, "ID_LEVEL_1", "Background", 1, "ID_T_BG14");
    let r = e.derive(&s).compatibility;
    assert_eq!(r.entries.len(), 1);
    assert!(!r.entries[0].converted);
    assert_eq!(r.entries[0].notes, vec!["used as written".to_string()]);
}

#[test]
fn a_2024_class_in_a_2014_character_is_never_offered() {
    let e = engine_with(legacy_race_content());
    for allow in [None, Some(true), Some(false)] {
        let mut s = hero(&e);
        s.allow_cross_edition = allow;
        let snap = e.derive(&s);
        assert!(
            !class_ids(&snap).contains(&"ID_T_CLASS24".to_string()),
            "{allow:?}"
        );
        // IM8 (amends R4): a 2024 species is offered only with the other edition allowed
        assert_eq!(
            race_ids(&snap).contains(&"ID_T_SPECIES24".to_string()),
            allow == Some(true),
            "{allow:?}"
        );
    }
    // and a 2014 class is never offered to a 2024 character
    let e = engine_with(vec![
        json!({"id":"ID_T_CLASS14","name":"Old Class","type":"Class","source":"Test","ruleset":"dnd2014","setters":[{"name":"hd","value":"d8"}]}),
    ]);
    let mut s = hero(&e);
    s.ruleset = Ruleset::Dnd2024;
    s.allow_cross_edition = Some(true);
    assert!(!class_ids(&e.derive(&s)).contains(&"ID_T_CLASS14".to_string()));
    assert!(!e.has_element(Ruleset::Dnd2024, "ID_T_CLASS14_MC"));
}

#[test]
fn strict_mode_hides_foreign_options_but_keeps_stored_choices() {
    let e = engine_with(legacy_race_content());
    let mut s = hero(&e);
    s.ruleset = Ruleset::Dnd2024;
    s = pick(&e, &s, "ID_LEVEL_1", "Race", 1, "ID_T_ELF14");
    s.allow_cross_edition = Some(false);
    let snap = e.derive(&s);
    assert!(snap.compatibility.strict);
    // the stored choice stays registered and selected (the current choice is always listed)
    assert_eq!(snap.origin.get("Race").unwrap().id, "ID_T_ELF14");
    assert!(snap.invalid_selections.is_empty());
    assert_eq!(snap.compatibility.entries.len(), 1);
    // a fresh character in strict mode is not offered the 2014 race
    let mut fresh = commands::new_character(&e, Ruleset::Dnd2024);
    fresh.allow_cross_edition = Some(false);
    assert!(!race_ids(&e.derive(&fresh)).contains(&"ID_T_ELF14".to_string()));
    fresh.allow_cross_edition = None;
    assert!(race_ids(&e.derive(&fresh)).contains(&"ID_T_ELF14".to_string()));
    // a 2014 character can opt in to foreign spells/items but still never to classes
    let mut s14 = hero(&e);
    s14.allow_cross_edition = Some(true);
    assert!(!class_ids(&e.derive(&s14)).contains(&"ID_T_CLASS24".to_string()));
}

#[test]
fn rejected_content_stays_invalid_on_replay() {
    let e = engine_with(legacy_race_content());
    let mut s = hero(&e);
    s.allow_cross_edition = Some(true);
    let snap = e.derive(&s);
    let key = slot_key(&snap, "ID_LEVEL_1", "Class", 1).unwrap();
    // stored by an older file / hand edit: a 2024 class in a 2014 character (classes never
    // cross editions; 2024 species do since IM8)
    s.selections.push(borealis_engine::state::Selection {
        slot: key,
        choice: Choice::Element("ID_T_CLASS24".into()),
        replace_level: None,
    });
    let snap = e.derive(&s);
    assert_eq!(snap.invalid_selections.len(), 1);
}

#[test]
fn passthrough_adapter_uses_foreign_races_as_written() {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/core");
    let file: ContentFile = serde_json::from_value(
        json!({"format": "borealis-content/1", "ruleset": "dnd2014", "elements": legacy_race_content()}),
    )
    .unwrap();
    let store = ContentStoreBuilder::new()
        .add_pack_dir(&core)
        .unwrap()
        .add_file("synthetic.json", PackOrigin::Core, file)
        .build();
    let e = Engine::with_adapter(
        &store,
        borealis_engine::EngineSettings::default(),
        &borealis_engine::PassThroughAdapter,
    );
    let mut s = commands::new_character(&e, Ruleset::Dnd2024);
    s = pick(&e, &s, "ID_LEVEL_1", "Race", 1, "ID_T_ELF14");
    let snap = e.derive(&s);
    let entry = &snap.compatibility.entries[0];
    assert!(!entry.converted);
    let plain = commands::new_character(&e, Ruleset::Dnd2024);
    assert_eq!(
        snap.abilities[1].score,
        e.derive(&plain).abilities[1].score + 2
    );
}

#[test]
fn compatibility_report_round_trips_through_serde() {
    let e = engine_with(legacy_race_content());
    let mut s = hero(&e);
    s.ruleset = Ruleset::Dnd2024;
    s = pick(&e, &s, "ID_LEVEL_1", "Race", 1, "ID_T_ELF14");
    let snap = e.derive(&s);
    assert!(!snap.compatibility.entries.is_empty());
    let text = serde_json::to_string(&snap.compatibility).unwrap();
    let back: borealis_engine::CompatibilityReport = serde_json::from_str(&text).unwrap();
    assert_eq!(back, snap.compatibility);
    snapshot_json_round_trip(&snap);
    // the state field is optional on disk
    let st = serde_json::to_value(&s).unwrap();
    assert!(st.get("allow_cross_edition").is_none());
    s.allow_cross_edition = Some(false);
    let back: CharacterState = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert_eq!(back.allow_cross_edition, Some(false));
}

/// engine-optional-sole-option.md: a background in the shape of the PHB Noble (an optional
/// one-option variant without a default, a variant feature gated on `!<variant>`) plus a
/// defaulted sole option and a defaulted two-option slot.
fn noble_engine() -> Engine {
    engine_with(vec![
        json!({"id":"ID_T_NOBLE","name":"Noble","type":"Background","source":"Test","rules":[
            {"kind":"select","type":"Background Variant","name":"Variant Noble","supports":"Variant Noble","optional":true},
            {"kind":"select","type":"Background Feature","name":"Variant Feature","supports":"Noble Feature","optional":true,"requirements":"!ID_T_KNIGHT"},
            {"kind":"grant","type":"Background Feature","id":"ID_T_PRIVILEGE","requirements":"!ID_T_KNIGHT"},
            {"kind":"select","type":"Background Feature","name":"Fixed","supports":"Sole Feature","default":"ID_T_ONLY"},
            {"kind":"select","type":"Background Feature","name":"Pair","supports":"Pair Feature","default":"ID_T_PAIR_A"}]}),
        json!({"id":"ID_T_KNIGHT","name":"Knight","type":"Background Variant","source":"Test","supports":["Variant Noble"],"rules":[
            {"kind":"grant","type":"Background Feature","id":"ID_T_RETAINERS"}]}),
        json!({"id":"ID_T_PRIVILEGE","name":"Position of Privilege","type":"Background Feature","source":"Test"}),
        json!({"id":"ID_T_RETAINERS","name":"Retainers","type":"Background Feature","source":"Test"}),
        json!({"id":"ID_T_FEAT_A","name":"Feature A","type":"Background Feature","source":"Test","supports":["Noble Feature"]}),
        json!({"id":"ID_T_FEAT_B","name":"Feature B","type":"Background Feature","source":"Test","supports":["Noble Feature"]}),
        json!({"id":"ID_T_ONLY","name":"Only","type":"Background Feature","source":"Test","supports":["Sole Feature"]}),
        json!({"id":"ID_T_PAIR_A","name":"Pair A","type":"Background Feature","source":"Test","supports":["Pair Feature"]}),
        json!({"id":"ID_T_PAIR_B","name":"Pair B","type":"Background Feature","source":"Test","supports":["Pair Feature"]}),
    ])
}

fn noble(e: &Engine) -> CharacterState {
    pick(e, &hero(e), "ID_LEVEL_1", "Background", 1, "ID_T_NOBLE")
}

#[test]
fn sole_option_without_default_stays_open_and_unlocked() {
    let e = noble_engine();
    let snap = e.derive(&noble(&e));
    // SO-1: one option, no default: empty, unlocked, nothing registered
    let variant = snap.slot("ID_T_NOBLE", "Variant Noble").unwrap();
    assert_eq!(variant.options.len(), 1);
    assert!(variant.chosen.is_none() && !variant.locked, "{variant:?}");
    assert!(!snap.has_element("ID_T_KNIGHT"));
    // SO-6: `!Knight` is true while Knight is only offered
    let feature = snap.slot("ID_T_NOBLE", "Variant Feature").unwrap();
    assert!(feature.rejected.is_none() && feature.options.len() == 2);
    assert!(snap.has_element("ID_T_PRIVILEGE"));
    // a defaulted slot is not locked before it holds a choice
    let fixed = snap.slot("ID_T_NOBLE", "Fixed").unwrap();
    assert!(fixed.chosen.is_none() && !fixed.locked);
}

#[test]
fn choosing_the_sole_variant_closes_the_gated_slot_and_clearing_reopens_it() {
    let e = noble_engine();
    let mut s = noble(&e);
    s = pick(&e, &s, "ID_T_NOBLE", "Variant Feature", 1, "ID_T_FEAT_A");
    assert!(e.derive(&s).has_element("ID_T_FEAT_A"));
    s = pick(&e, &s, "ID_T_NOBLE", "Variant Noble", 1, "ID_T_KNIGHT");
    let snap = e.derive(&s);
    assert!(snap.has_element("ID_T_KNIGHT") && snap.has_element("ID_T_RETAINERS"));
    assert!(
        !snap.has_element("ID_T_FEAT_A"),
        "the gated choice is unregistered"
    );
    assert!(!snap.has_element("ID_T_PRIVILEGE"));
    // a chosen sole option without a default stays unlocked: the user may clear it
    let variant = snap.slot("ID_T_NOBLE", "Variant Noble").unwrap();
    assert!(variant.chosen.is_some() && !variant.locked, "{variant:?}");
    let key = variant.key.clone();
    let s = commands::clear(&e, &s, &key).unwrap().state;
    let snap = e.derive(&s);
    assert!(!snap.has_element("ID_T_KNIGHT") && !snap.has_element("ID_T_RETAINERS"));
    let variant = snap.slot("ID_T_NOBLE", "Variant Noble").unwrap();
    assert!(variant.chosen.is_none() && !variant.locked);
    let feature = snap.slot("ID_T_NOBLE", "Variant Feature").unwrap();
    assert!(feature.chosen.is_none() && feature.options.len() == 2);
    assert!(snap.has_element("ID_T_PRIVILEGE"));
}

#[test]
fn only_a_defaulted_sole_option_that_is_chosen_is_locked() {
    let e = noble_engine();
    let snap = e.derive(&commands::apply_defaults(&e, &noble(&e)));
    // SO-3: default + one option + chosen
    let fixed = snap.slot("ID_T_NOBLE", "Fixed").unwrap();
    assert_eq!(
        fixed.chosen.as_ref().map(|c| c.id.as_str()),
        Some("ID_T_ONLY")
    );
    assert!(fixed.locked, "{fixed:?}");
    // a default among two options is chosen but stays unlocked
    let pair = snap.slot("ID_T_NOBLE", "Pair").unwrap();
    assert_eq!(
        pair.chosen.as_ref().map(|c| c.id.as_str()),
        Some("ID_T_PAIR_A")
    );
    assert!(!pair.locked);
    // the optional sole option without a default is still not picked
    let variant = snap.slot("ID_T_NOBLE", "Variant Noble").unwrap();
    assert!(variant.chosen.is_none() && !variant.locked);
}
