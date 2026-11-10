//! WB-08 (homebrew creator, spec 9 "Try it"): the trial commands through the real `AppState`, on a temporary data
//! folder with the bundled content: start with the defaults per kind, the level ladder, level and setup changes, the
//! trial character's choices and their options, the multiclass check, the idle expiry, `end`, and the guarantee that a
//! trial never changes the open character or any file.

mod contract_support;
use borealis_authoring::form::FormState;
use borealis_lib::content_dto::ContentProgress;
use borealis_lib::content_packs::EventSink;
use borealis_lib::dto::{
    AbilityMethodDto, CharacterSnapshot, NewCharacterRequest, OptionsPatch, Ruleset,
};
use borealis_lib::error::ErrorCode;
use borealis_lib::homebrew_dto::{
    HomebrewDraftView, HomebrewPackRequest, HomebrewTrialChange, HomebrewTrialSetup,
    HomebrewTrialView,
};
use borealis_lib::homebrew_trial::IDLE_LIMIT;
use borealis_lib::state::{AppPaths, AppState};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

struct NoEvents;
impl EventSink for NoEvents {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
}

struct App {
    data: tempfile::TempDir,
    state: AppState,
}

impl App {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::new(AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: data.path().to_path_buf(),
        })
        .unwrap();
        Self { data, state }
    }

    fn pack(&self, edition: Ruleset) -> String {
        self.state
            .homebrew_create_pack(
                &HomebrewPackRequest {
                    name: "Gloom Hollow".into(),
                    code: "GH".into(),
                    edition,
                    author: None,
                    description: None,
                    license: None,
                },
                &NoEvents,
            )
            .unwrap()
            .id
    }

    /// A draft copied from a loaded element (its rules kept, "<Name> (copy)").
    fn copy(&self, pack: &str, id: &str) -> HomebrewDraftView {
        self.state
            .homebrew_new_draft(pack, "other", Some(id), None, None)
            .unwrap()
    }

    fn named(&self, pack: &str, kind: &str, name: &str) -> HomebrewDraftView {
        let v = self
            .state
            .homebrew_new_draft(pack, kind, None, None, None)
            .unwrap();
        let mut form: FormState = v.form.clone();
        form.fields.insert("name".into(), json!(name));
        self.state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap()
    }

    fn start(&self, draft: &HomebrewDraftView, setup: HomebrewTrialSetup) -> HomebrewTrialView {
        self.state
            .homebrew_trial_start(&draft.draft_id, &setup)
            .unwrap()
    }

    fn update(&self, trial: &str, change: HomebrewTrialChange) -> HomebrewTrialView {
        self.state.homebrew_trial_update(trial, &change).unwrap()
    }

    /// Every file of the data folder with its bytes (the content cache written in the background is waited for).
    fn files(&self) -> BTreeMap<String, Vec<u8>> {
        self.state.hub().unwrap().wait_for_cache_write();
        assert!(self
            .state
            .search
            .wait_idle(std::time::Duration::from_secs(30)));
        let mut out = BTreeMap::new();
        walk(self.data.path(), self.data.path(), &mut out);
        out
    }
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.filter_map(Result::ok) {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            out.insert(rel, std::fs::read(&p).unwrap());
        }
    }
}

fn level(view: &HomebrewTrialView) -> u32 {
    view.snapshot.level.total
}

fn feature_names(s: &CharacterSnapshot) -> Vec<String> {
    let v = serde_json::to_value(&s.play.features).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["name"].as_str().map(str::to_string))
        .collect()
}

fn class_levels(s: &CharacterSnapshot) -> Vec<(String, u32)> {
    let v = serde_json::to_value(&s.level).unwrap();
    v["classes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap_or_default().to_string(),
                u32::try_from(c["level"].as_u64().unwrap()).unwrap(),
            )
        })
        .collect()
}

fn messages(view: &HomebrewTrialView) -> Vec<String> {
    view.problems.iter().map(|p| p.message.clone()).collect()
}

fn scores(view: &HomebrewTrialView) -> [i32; 6] {
    let b = &view.snapshot.abilities.base;
    [b.str, b.dex, b.con, b.int, b.wis, b.cha]
}

#[test]
fn a_feat_waits_for_the_first_feat_slot_with_the_srd_picks_around_it() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let draft = app.copy(&pack, "ID_2024_FEAT_GRAPPLER");
    let started = Instant::now();
    let view = app.start(&draft, HomebrewTrialSetup::default());
    eprintln!("start: {:?}", started.elapsed());
    // the real view has the shape the UI declares (`HomebrewTrialView`, with the app's `CharacterSnapshot`)
    let errs = contract_support::Contract::load()
        .def("HomebrewTrialView", &serde_json::to_value(&view).unwrap());
    assert!(errs.is_empty(), "{}", errs.join("\n"));
    // an empty setup: the pack's edition, Fighter / Human / Soldier, and the level the feat is taken at
    assert_eq!(view.snapshot.ruleset, Ruleset::Dnd2024);
    assert_eq!(
        class_levels(&view.snapshot),
        vec![("Fighter".to_string(), 4)]
    );
    assert_eq!(level(&view), 4, "{:?}", messages(&view));
    let names = feature_names(&view.snapshot);
    assert!(names.iter().any(|n| n == "Grappler (copy)"), "{names:?}");
    assert!(
        view.changed
            .iter()
            .any(|c| c == "play.features:Grappler (copy)"),
        "{:?}",
        view.changed
    );
    // the standard array by the Fighter's main abilities
    assert_eq!(scores(&view).iter().copied().max(), Some(15));
    let mut sorted = scores(&view);
    sorted.sort_unstable();
    assert_eq!(sorted, [8, 10, 12, 13, 14, 15]);
    // the ladder: 20 rows, the feat on the row of level 4 only
    assert_eq!(view.ladder.len(), 20);
    assert_eq!(
        view.ladder.iter().map(|r| r.level).collect::<Vec<_>>(),
        (1..=20).collect::<Vec<_>>()
    );
    let with_feat: Vec<u32> = view
        .ladder
        .iter()
        .filter(|r| r.features.iter().any(|f| f == "Grappler (copy)"))
        .map(|r| r.level)
        .collect();
    assert_eq!(with_feat, vec![4]);
    assert!(
        !view.ladder[0].features.is_empty(),
        "level 1 brings the Fighter's features"
    );
    assert!(
        view.ladder.iter().all(|r| r.slots.iter().all(|n| *n == 0)),
        "a Fighter casts nothing"
    );
    assert!(
        view.ladder[2]
            .choices
            .iter()
            .any(|c| c.contains("Subclass") || c.contains("Archetype")),
        "level 3 brings the subclass choice: {:?}",
        view.ladder[2].choices
    );
    // below its level the feat is not on the character
    let t = &view.trial_id;
    let started = Instant::now();
    let low = app.update(
        t,
        HomebrewTrialChange {
            level: Some(3),
            ..Default::default()
        },
    );
    eprintln!("level update: {:?}", started.elapsed());
    assert_eq!(level(&low), 3);
    assert!(!feature_names(&low.snapshot)
        .iter()
        .any(|n| n == "Grappler (copy)"));
    assert!(low.changed.is_empty(), "{:?}", low.changed);
    assert_eq!(low.ladder, view.ladder, "a level step keeps the ladder");
    // out-of-range levels are clamped, as the slider shows them
    let top = app.update(
        t,
        HomebrewTrialChange {
            level: Some(99),
            ..Default::default()
        },
    );
    assert_eq!(level(&top), 20);
    assert!(app.state.homebrew_trial_end(t).is_ok());
}

#[test]
fn an_origin_feat_is_taken_at_level_1_and_its_numbers_are_marked() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let draft = app.copy(&pack, "ID_2024_FEAT_ALERT");
    let view = app.start(&draft, HomebrewTrialSetup::default());
    assert_eq!(level(&view), 1, "{:?}", messages(&view));
    assert!(feature_names(&view.snapshot)
        .iter()
        .any(|n| n == "Alert (copy)"));
    // Alert adds the proficiency bonus to initiative
    assert!(
        view.changed.iter().any(|c| c == "stats.initiative"),
        "{:?}",
        view.changed
    );
    assert!(view
        .changed
        .iter()
        .any(|c| c == "play.features:Alert (copy)"));
    // nothing offers it at level 1 (the 2024 Human has no origin feat choice here): it is a test grant
    assert!(!view
        .snapshot
        .slots
        .iter()
        .any(|s| s.chosen.as_ref().is_some_and(|c| c.name == "Alert (copy)")));
}

#[test]
fn a_fighting_style_sits_in_its_choice_and_that_choice_is_fixed() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let draft = app.copy(&pack, "ID_2024_FEAT_DEFENSE");
    let view = app.start(&draft, HomebrewTrialSetup::default());
    assert_eq!(level(&view), 1, "{:?}", messages(&view));
    let slot = view
        .snapshot
        .slots
        .iter()
        .find(|s| {
            s.chosen
                .as_ref()
                .is_some_and(|c| c.name == "Defense (copy)")
        })
        .expect("the Fighting Style choice holds the draft");
    assert!(slot.locked, "the draft's own choice comes back locked");
    for element_id in [None, Some("ID_2024_FEAT_ARCHERY".to_string())] {
        let e = app
            .state
            .homebrew_trial_update(
                &view.trial_id,
                &HomebrewTrialChange {
                    slot_id: Some(slot.id.clone()),
                    element_id: Some(element_id),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidArgument);
    }
}

#[test]
fn choices_list_their_options_and_change_the_trial_character() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let draft = app.named(&pack, "language", "Gloomspeak");
    let view = app.start(&draft, HomebrewTrialSetup::default());
    // a language nothing offers is granted from level 1
    assert_eq!(level(&view), 1);
    let languages = &view.snapshot.stats.languages;
    assert!(languages.iter().any(|l| l == "Gloomspeak"), "{languages:?}");
    assert!(
        view.changed
            .iter()
            .any(|c| c == "stats.languages:Gloomspeak"),
        "{:?}",
        view.changed
    );
    // an open choice: its options are the rows get_slot_options answers
    let open = view
        .snapshot
        .slots
        .iter()
        .find(|s| s.chosen.is_none() && !s.locked && s.element_type != "List")
        .expect("an open choice");
    let page = app
        .state
        .homebrew_trial_options(&view.trial_id, &open.id, None, Some(5), None)
        .unwrap();
    assert!(page.total > 0 && !page.items.is_empty() && page.items.len() <= 5);
    let pick = page.items[0].base.summary.id.clone();
    let started = Instant::now();
    let picked = app.update(
        &view.trial_id,
        HomebrewTrialChange {
            slot_id: Some(open.id.clone()),
            element_id: Some(Some(pick.clone())),
            ..Default::default()
        },
    );
    eprintln!("choice update: {:?}", started.elapsed());
    let slot = picked
        .snapshot
        .slots
        .iter()
        .find(|s| s.id == open.id)
        .unwrap();
    assert_eq!(
        slot.chosen.as_ref().map(|c| c.id.as_str()),
        Some(pick.as_str())
    );
    // the choice stays when the level moves up and comes back down
    let up = app.update(
        &view.trial_id,
        HomebrewTrialChange {
            level: Some(6),
            ..Default::default()
        },
    );
    assert_eq!(level(&up), 6);
    let back = app.update(
        &view.trial_id,
        HomebrewTrialChange {
            level: Some(1),
            ..Default::default()
        },
    );
    let slot = back
        .snapshot
        .slots
        .iter()
        .find(|s| s.id == open.id)
        .unwrap();
    assert_eq!(
        slot.chosen.as_ref().map(|c| c.id.as_str()),
        Some(pick.as_str())
    );
    // null clears it
    let cleared = app.update(
        &view.trial_id,
        HomebrewTrialChange {
            slot_id: Some(open.id.clone()),
            element_id: Some(None),
            ..Default::default()
        },
    );
    let slot = cleared
        .snapshot
        .slots
        .iter()
        .find(|s| s.id == open.id)
        .unwrap();
    assert!(slot.chosen.is_none());
    // an option a choice doesn't offer, and a slot the character doesn't have, are refused
    let refused = app.state.homebrew_trial_update(
        &view.trial_id,
        &HomebrewTrialChange {
            slot_id: Some(open.id.clone()),
            element_id: Some(Some("ID_2024_CLASS_WIZARD".into())),
            ..Default::default()
        },
    );
    assert!(refused.is_err());
    let unknown = app
        .state
        .homebrew_trial_options(&view.trial_id, "ID_NOPE:0:1", None, None, None)
        .unwrap_err();
    assert_eq!(unknown.code, ErrorCode::NotFound);
    // new ability scores keep the choices; bad ones are refused
    let scored = app.update(
        &view.trial_id,
        HomebrewTrialChange {
            setup: Some(HomebrewTrialSetup {
                ability_scores: Some(vec![8, 15, 14, 10, 13, 12]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    assert_eq!(scores(&scored), [8, 15, 14, 10, 13, 12]);
    for bad in [
        vec![8, 15, 14],
        vec![0, 15, 14, 10, 13, 12],
        vec![31, 15, 14, 10, 13, 12],
    ] {
        let e = app
            .state
            .homebrew_trial_update(
                &view.trial_id,
                &HomebrewTrialChange {
                    setup: Some(HomebrewTrialSetup {
                        ability_scores: Some(bad),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidArgument);
    }
    // another edition starts from that edition's picks
    let old = app.update(
        &view.trial_id,
        HomebrewTrialChange {
            setup: Some(HomebrewTrialSetup {
                edition: Some(Ruleset::Dnd2014),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    assert_eq!(old.snapshot.ruleset, Ruleset::Dnd2014);
    assert_eq!(class_levels(&old.snapshot)[0].0, "Fighter");
}

#[test]
fn a_spell_is_learned_through_its_class_list_at_the_level_that_casts_it() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let draft = app.copy(&pack, "ID_2024_SPELL_FIREBALL");
    let view = app.start(&draft, HomebrewTrialSetup::default());
    // a third-level spell: a class of its list (SRD first) at the level it casts third-level spells
    let classes = class_levels(&view.snapshot);
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].1, 5, "{classes:?} {:?}", messages(&view));
    let spells = serde_json::to_value(&view.snapshot.spellcasting).unwrap();
    let names: Vec<&str> = spells["classes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["spells"].as_array().unwrap().iter())
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert!(names.contains(&"Fireball (copy)"), "{names:?}");
    assert!(
        view.changed
            .iter()
            .any(|c| c == "spellcasting.spells:Fireball (copy)"),
        "{:?}",
        view.changed
    );
    assert!(view.ladder[4]
        .features
        .iter()
        .any(|f| f == "Fireball (copy)"));
}

#[test]
fn a_subclass_is_taken_by_its_parent_class_at_the_subclass_level() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let draft = app.copy(&pack, "ID_2024_SUBCLASS_FIGHTER_CHAMPION");
    let view = app.start(&draft, HomebrewTrialSetup::default());
    assert_eq!(level(&view), 3, "{:?}", messages(&view));
    assert_eq!(
        class_levels(&view.snapshot),
        vec![("Fighter".to_string(), 3)]
    );
    let subclass = serde_json::to_value(&view.snapshot.level.classes[0].subclass).unwrap();
    assert_eq!(subclass["name"], "Champion (copy)");
    assert!(view.ladder[2]
        .features
        .iter()
        .any(|f| f == "Champion (copy)"));
    // the copy grants the SRD Champion's features: the sweep finds nothing wrong with it
    assert!(
        view.problems.iter().all(|p| p.severity != "error"),
        "{:?}",
        messages(&view)
    );
}

#[test]
fn a_class_draft_takes_the_class_slot_and_its_ladder_casts() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let draft = app.copy(&pack, "ID_2024_CLASS_WIZARD");
    let view = app.start(
        &draft,
        HomebrewTrialSetup {
            level: Some(3),
            ..Default::default()
        },
    );
    assert_eq!(
        class_levels(&view.snapshot),
        vec![("Wizard (copy)".to_string(), 3)]
    );
    let class_slot = view
        .snapshot
        .slots
        .iter()
        .find(|s| s.chosen.as_ref().is_some_and(|c| c.name == "Wizard (copy)"))
        .expect("the class slot holds the draft");
    assert!(class_slot.locked, "the draft's own slot comes back locked");
    // spell slots grow over the ladder: 2 first-level slots at 1, a 9th-level slot at 17
    assert_eq!(view.ladder[0].slots[0], 2);
    assert_eq!(view.ladder[16].slots[8], 1);
    assert_eq!(view.ladder[0].slots.len(), 9);
    assert!(
        view.changed
            .iter()
            .any(|c| c.starts_with("spellcasting.slots.")),
        "{:?}",
        view.changed
    );
}

#[test]
fn try_as_a_second_class_checks_the_multiclass_prerequisite() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let draft = app.copy(&pack, "ID_2024_CLASS_WIZARD");
    // the Fighter's standard array leaves Intelligence under 13: the prerequisite fails, it is taken anyway
    let setup = HomebrewTrialSetup {
        as_second_class: Some(true),
        level: Some(5),
        ability_scores: Some(vec![15, 14, 13, 8, 12, 10]),
        ..Default::default()
    };
    let view = app.start(&draft, setup);
    assert_eq!(
        class_levels(&view.snapshot),
        vec![("Fighter".to_string(), 1), ("Wizard (copy)".to_string(), 4)]
    );
    let prereq: Vec<_> = view
        .problems
        .iter()
        .filter(|p| p.message.contains("as a second class") && p.message.contains("prerequisite"))
        .collect();
    assert_eq!(prereq.len(), 1, "{:?}", messages(&view));
    assert!(view.ladder[1]
        .problems
        .iter()
        .any(|p| p.message.contains("prerequisite")));
    // with Intelligence 13 the prerequisite holds
    let ok = app.update(
        &view.trial_id,
        HomebrewTrialChange {
            setup: Some(HomebrewTrialSetup {
                ability_scores: Some(vec![15, 14, 12, 13, 10, 8]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    assert!(
        !ok.problems
            .iter()
            .any(|p| p.message.contains("prerequisite")),
        "{:?}",
        messages(&ok)
    );
    assert_eq!(
        class_levels(&ok.snapshot),
        vec![("Fighter".to_string(), 1), ("Wizard (copy)".to_string(), 4)]
    );
    // the multiclass slot holding the draft is fixed
    assert!(ok
        .snapshot
        .slots
        .iter()
        .filter(|s| s.chosen.as_ref().is_some_and(|c| c.name == "Wizard (copy)"))
        .all(|s| s.locked));
    // back to one class
    let single = app.update(
        &view.trial_id,
        HomebrewTrialChange {
            setup: Some(HomebrewTrialSetup {
                as_second_class: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    assert_eq!(
        class_levels(&single.snapshot),
        vec![("Wizard (copy)".to_string(), 5)]
    );
}

#[test]
fn an_idle_trial_expires_and_an_ended_one_is_gone() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let draft = app.named(&pack, "language", "Gloomspeak");
    let view = app.start(&draft, HomebrewTrialSetup::default());
    let t = view.trial_id.clone();
    // just under the limit it is still open
    let fresh = IDLE_LIMIT - std::time::Duration::from_secs(60);
    assert!(app.state.homebrew_trial_idle_for(&t, fresh).unwrap());
    assert!(app
        .state
        .homebrew_trial_update(&t, &HomebrewTrialChange::default())
        .is_ok());
    assert!(app.state.homebrew_trial_idle_for(&t, IDLE_LIMIT).unwrap());
    for e in [
        app.state
            .homebrew_trial_update(&t, &HomebrewTrialChange::default())
            .unwrap_err(),
        app.state
            .homebrew_trial_options(&t, "x:0:1", None, None, None)
            .unwrap_err(),
        app.state.homebrew_trial_end(&t).unwrap_err(),
    ] {
        assert_eq!(e.code, ErrorCode::NotFound, "{}", e.message);
    }
    // end, then not_found; the UI starts a new one with the same setup
    let again = app.start(
        &draft,
        HomebrewTrialSetup {
            level: Some(2),
            ..Default::default()
        },
    );
    assert_ne!(again.trial_id, t);
    assert_eq!(level(&again), 2);
    app.state.homebrew_trial_end(&again.trial_id).unwrap();
    assert_eq!(
        app.state
            .homebrew_trial_end(&again.trial_id)
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    // at most two trials are kept: a third start drops the least recently used
    let first = app.start(&draft, HomebrewTrialSetup::default());
    let second = app.start(&draft, HomebrewTrialSetup::default());
    app.update(&first.trial_id, HomebrewTrialChange::default());
    let third = app.start(&draft, HomebrewTrialSetup::default());
    assert_eq!(
        app.state
            .homebrew_trial_update(&second.trial_id, &HomebrewTrialChange::default())
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    for t in [&first.trial_id, &third.trial_id] {
        app.state.homebrew_trial_end(t).unwrap();
    }
    // an unknown draft
    assert_eq!(
        app.state
            .homebrew_trial_start("d_nope", &HomebrewTrialSetup::default())
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
}

#[test]
fn a_trial_never_touches_the_open_character_or_any_file() {
    let app = App::new();
    let pack = app.pack(Ruleset::Dnd2024);
    let opened = app
        .state
        .create_character(
            Ruleset::Dnd2024,
            &NewCharacterRequest {
                name: "Mira".into(),
                ruleset: Ruleset::Dnd2024,
                starting_level: 3,
                gender: String::new(),
                ability_method: AbilityMethodDto::StandardArray,
                options: OptionsPatch::default(),
                portrait: None,
                group: None,
            },
        )
        .unwrap();
    let draft = app.copy(&pack, "ID_2024_FEAT_GRAPPLER");
    let named = app.named(&pack, "language", "Gloomspeak");
    let files = app.files();
    let listed = app.state.list_characters().unwrap();
    let before = serde_json::to_value(app.state.get_snapshot(&opened.id).unwrap()).unwrap();
    assert_eq!(before["dirty"], false);

    let view = app.start(&draft, HomebrewTrialSetup::default());
    let open = view
        .snapshot
        .slots
        .iter()
        .find(|s| s.chosen.is_none() && !s.locked && s.element_type != "List")
        .expect("an open choice");
    let rows = app
        .state
        .homebrew_trial_options(&view.trial_id, &open.id, None, None, None)
        .unwrap();
    app.update(
        &view.trial_id,
        HomebrewTrialChange {
            slot_id: Some(open.id.clone()),
            element_id: Some(Some(rows.items[0].base.summary.id.clone())),
            level: Some(12),
            ..Default::default()
        },
    );
    app.update(
        &view.trial_id,
        HomebrewTrialChange {
            setup: Some(HomebrewTrialSetup {
                edition: Some(Ruleset::Dnd2014),
                as_second_class: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    let other = app.start(&named, HomebrewTrialSetup::default());
    app.state.homebrew_trial_end(&view.trial_id).unwrap();
    app.state.homebrew_trial_end(&other.trial_id).unwrap();

    // the draft never reached the shared content store
    let hub = app.state.hub().unwrap();
    assert!(!hub
        .store
        .iter()
        .any(|s| s.element.name == "Grappler (copy)"));
    assert!(!hub.store.iter().any(|s| s.element.name == "Gloomspeak"));
    // the open character, its dirty flag, the character list and every file are as they were
    let after = serde_json::to_value(app.state.get_snapshot(&opened.id).unwrap()).unwrap();
    assert_eq!(after, before);
    let listed_after = app.state.list_characters().unwrap();
    assert_eq!(
        serde_json::to_value(&listed_after).unwrap(),
        serde_json::to_value(&listed).unwrap()
    );
    let now = app.files();
    assert_eq!(
        now.keys().collect::<Vec<_>>(),
        files.keys().collect::<Vec<_>>(),
        "no file was added or removed"
    );
    for (path, bytes) in &files {
        assert!(now.get(path) == Some(bytes), "{path} changed");
    }
}
