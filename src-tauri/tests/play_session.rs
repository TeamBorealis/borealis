//! A full play session through the real command layer on every bundled pack, checking each
//! response (and the events the backend emits) against the TS contract.

mod contract_support;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use borealis_lib::dto::{AbilityMethodDto, NewCharacterRequest, OptionsPatch, Ruleset};
use borealis_lib::dto_ext::{ItemFilter, SheetOptionsDto};
use borealis_lib::state::{AppPaths, AppState};
use contract_support::Contract;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;

struct Session {
    contract: Contract,
    state: AppState,
    errors: Vec<String>,
    _dir: tempfile::TempDir,
}

impl Session {
    fn new() -> Self {
        borealis_engine::invariants::enable_for_thread();
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths {
            content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
            data: dir.path().to_path_buf(),
        };
        Self {
            contract: Contract::load(),
            state: AppState::new(paths).unwrap(),
            errors: Vec::new(),
            _dir: dir,
        }
    }

    /// Serialize a command result and check it against the TS response type.
    fn check<T: Serialize>(&mut self, command: &str, value: &T) -> Value {
        let v = serde_json::to_value(value).unwrap();
        self.errors.extend(self.contract.response(command, &v));
        v
    }

    fn new_character(&mut self, name: &str, ruleset: Ruleset) -> (String, Value) {
        let request = NewCharacterRequest {
            name: name.into(),
            ruleset,
            starting_level: 1,
            gender: String::new(),
            ability_method: AbilityMethodDto::StandardArray,
            options: OptionsPatch::default(),
            portrait: None,
            group: None,
        };
        let snap = self.state.create_character(ruleset, &request).unwrap();
        let v = self.check("create_character", &snap);
        (v["id"].as_str().unwrap().to_string(), v)
    }

    /// Pick `preferred` for slots of the given element type, else the first enabled option, until
    /// no required slot is left open.
    fn fill(&mut self, id: &str, preferred: &[(&str, &str)]) -> Value {
        let mut snap = self.check("get_snapshot", &self.state.get_snapshot(id).unwrap());
        for _ in 0..80 {
            let Some(slot) =
                snap["slots"].as_array().unwrap().iter().find(|s| {
                    s["required"] == true && s["locked"] == false && s["chosen"].is_null()
                })
            else {
                break;
            };
            let (slot_id, ty) = (
                slot["id"].as_str().unwrap().to_string(),
                slot["elementType"].as_str().unwrap().to_string(),
            );
            let page = self
                .state
                .get_slot_options(id, &slot_id, None, Some(200), None)
                .unwrap();
            let page = self.check("get_slot_options", &page);
            let items = page["items"].as_array().unwrap();
            let want = preferred.iter().find(|(t, _)| *t == ty).map(|(_, e)| *e);
            let pick = items
                .iter()
                .find(|o| {
                    Some(o["id"].as_str().unwrap()) == want && o.get("disabledReason").is_none()
                })
                .or_else(|| items.iter().find(|o| o.get("disabledReason").is_none()))
                .unwrap_or_else(|| panic!("no option for slot {slot_id} ({ty})"));
            let element = pick["id"].as_str().unwrap().to_string();
            let out = self.state.apply_selection(id, &slot_id, &element).unwrap();
            snap = self.check("apply_selection", &out);
        }
        snap
    }

    fn level_to(&mut self, id: &str, level: u64) -> Value {
        let mut snap = self.check("get_snapshot", &self.state.get_snapshot(id).unwrap());
        while snap["level"]["total"].as_u64().unwrap_or(0) < level {
            let slot = snap["level"]["classes"][0]["slotId"]
                .as_str()
                .unwrap()
                .to_string();
            let out = self.state.level_up(id, &slot).unwrap();
            self.check("level_up", &out);
            snap = self.fill(id, &[]);
        }
        snap
    }

    fn finish(self) {
        let mut unique = std::collections::BTreeSet::new();
        for e in &self.errors {
            let flat: String = e
                .split('[')
                .map(|p| p.split_once(']').map_or(p, |(_, rest)| rest))
                .collect::<Vec<_>>()
                .join("[]");
            let tail = flat
                .split_once('.')
                .map_or(flat.clone(), |(_, t)| t.to_string());
            unique.insert(tail);
        }
        assert!(
            unique.is_empty(),
            "contract violations:\n{}",
            unique.into_iter().collect::<Vec<_>>().join("\n")
        );
    }
}

#[test]
fn a_druid_play_session_matches_the_ts_contract() {
    let mut s = Session::new();
    let (id, created) = s.new_character("Moss", Ruleset::Dnd2024);
    assert_eq!(created["ruleset"], "dnd2024");
    let snap = s.fill(
        &id,
        &[
            ("class", "ID_2024_CLASS_DRUID"),
            ("race", "ID_2024_SPECIES_HUMAN"),
        ],
    );
    assert_eq!(snap["level"]["classes"][0]["name"], "Druid");
    let snap = s.level_to(&id, 4);
    assert_eq!(snap["level"]["total"], 4);
    // The snapshot's top-level keys are exactly the TS `CharacterSnapshot` keys.
    let mut rust_keys: Vec<_> = snap.as_object().unwrap().keys().cloned().collect();
    let mut ts_keys = s.contract.def_keys("CharacterSnapshot");
    rust_keys.sort();
    ts_keys.sort();
    assert_eq!(rust_keys, ts_keys);

    // Wild Shape: the real options come from the snapshot.
    let ws = snap["play"]["wildShape"].clone();
    assert!(!ws.is_null(), "a level 4 Druid has Wild Shape");
    let options = ws["options"].as_array().unwrap();
    assert!(!options.is_empty(), "no Wild Shape options");
    let forms: Vec<String> = options
        .iter()
        .take(ws["maxForms"].as_u64().unwrap() as usize)
        .map(|o| o["id"].as_str().unwrap().to_string())
        .collect();
    let out = s.state.set_wild_shape_forms(&id, &forms).unwrap();
    let snap = s.check("set_wild_shape_forms", &out);
    assert_eq!(
        snap["play"]["wildShape"]["known"].as_array().unwrap().len(),
        forms.len()
    );

    // Explain every stat key the Play panel uses.
    for stat in ["ac", "initiative", "speed", "hp", "proficiency"] {
        let trace = s.state.explain_stat(&id, stat).unwrap();
        let v = s.check("explain_stat", &trace);
        assert!(
            !v["steps"].as_array().unwrap().is_empty(),
            "explain `{stat}` has no steps: {v}"
        );
    }
    let ac = s.state.explain_stat(&id, "ac").unwrap();
    assert!(
        serde_json::to_value(&ac).unwrap()["value"]
            .as_i64()
            .unwrap()
            >= 10
    );

    // Catalog category used by the Weapon Mastery chooser.
    let weapons = s
        .state
        .search_items(&ItemFilter {
            category: Some("Weapons".into()),
            limit: Some(500),
            ..ItemFilter::default()
        })
        .unwrap();
    let v = s.check("search_items", &weapons);
    assert!(
        v["total"].as_u64().unwrap() > 5,
        "category `Weapons` is empty: {v}"
    );

    // Sheet export.
    let file = s
        .state
        .sheet_file(
            &id,
            &SheetOptionsDto {
                format: Some("pdf".into()),
            },
        )
        .unwrap();
    assert!(file.bytes.starts_with(b"%PDF"));
    let exported = file.into_base64();
    let v = s.check("export_sheet", &exported);
    assert!(STANDARD
        .decode(v["dataBase64"].as_str().unwrap())
        .unwrap()
        .starts_with(b"%PDF"));
    let preview = s
        .state
        .render_sheet(&id, &SheetOptionsDto::default())
        .unwrap();
    s.check("render_sheet", &preview);

    s.finish();
}

fn item_named<'a>(rows: &'a Value, name: &str) -> &'a Value {
    rows["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("no catalogue row named {name}"))
}

fn catalog(s: &mut Session, category: &str) -> Value {
    let page = s
        .state
        .search_items(&ItemFilter {
            category: Some(category.into()),
            limit: Some(500),
            ..ItemFilter::default()
        })
        .unwrap();
    s.check("search_items", &page)
}

#[test]
fn equipment_and_magic_columns_match_the_ts_contract() {
    let mut s = Session::new();
    let (id, _) = s.new_character("Columns", Ruleset::Dnd2024);
    let snap = s.fill(&id, &[("class", "ID_2024_CLASS_WIZARD")]);

    // Armor columns.
    let armor = catalog(&mut s, "Armor");
    let chain = item_named(&armor, "Chain Mail");
    assert_eq!(chain["armor"]["group"], "Heavy");
    assert_eq!(chain["armor"]["acBase"], 16);
    assert_eq!(chain["armor"]["strength"], 13);
    assert_eq!(chain["armor"]["stealthDisadvantage"], true);
    assert_eq!(chain["armor"]["addsDex"], false);
    assert_eq!(chain["weight"], 55.0);
    assert_eq!(chain["costCp"], 7500);
    let breast = item_named(&armor, "Breastplate");
    assert_eq!(breast["armor"]["dexCap"], 2);
    assert_eq!(breast["armor"]["addsDex"], true);
    assert!(chain.get("weapon").is_none() && chain.get("spell").is_none());
    assert_eq!(item_named(&armor, "Shield")["armor"]["group"], "Shield");

    // Weapon columns (both rulesets share the category).
    let weapons = catalog(&mut s, "Weapons");
    let bow = item_named(&weapons, "Longbow");
    assert_eq!(bow["weapon"]["range"], "150/600");
    assert_eq!(bow["weapon"]["kind"], "Ranged");
    assert_eq!(bow["weapon"]["damageType"], "piercing");
    let club = weapons["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == "ID_2024_WEAPON_CLUB")
        .expect("2024 club");
    assert_eq!(club["weapon"]["mastery"], "Slow");
    assert_eq!(club["weapon"]["damage"], "1d4");
    assert_eq!(club["weapon"]["properties"][0], "Light");
    assert_eq!(club["weapon"]["proficiency"], "Simple");

    // Spell columns on the compendium detail.
    let alarm = s.state.get_element("ID_2024_SPELL_ALARM").unwrap();
    let alarm = s.check("get_element", &alarm);
    assert_eq!(alarm["spell"]["ritual"], true);
    assert_eq!(alarm["spell"]["material"], true);
    assert_eq!(alarm["spell"]["materialText"], "a bell and silver wire");
    assert_eq!(alarm["spell"]["duration"], "8 hours");

    // Spellcasting rows carry the same facts.
    let spells = snap["spellcasting"]["classes"][0]["spells"]
        .as_array()
        .unwrap();
    assert!(!spells.is_empty(), "a level 1 Wizard knows spells");
    for sp in spells {
        let c = &sp["casting"];
        assert!(!c["castingTime"].as_str().unwrap().is_empty(), "{sp}");
        assert!(!c["range"].as_str().unwrap().is_empty(), "{sp}");
        assert!(!c["duration"].as_str().unwrap().is_empty(), "{sp}");
    }
    assert!(spells.iter().any(|sp| sp["casting"]["verbal"] == true));

    // Inventory rows: armor columns and weapon mastery/range.
    for (item, qty) in [
        ("ID_2024_ARMOR_CHAIN_MAIL", 1),
        ("ID_2024_WEAPON_LONGBOW", 1),
    ] {
        let out = s.state.add_item(&id, item, qty, false, None).unwrap();
        s.check("add_item", &out);
    }
    let snap = s.check("get_snapshot", &s.state.get_snapshot(&id).unwrap());
    let rows = snap["inventory"]["items"].as_array().unwrap();
    let mail = rows.iter().find(|r| r["name"] == "Chain Mail").unwrap();
    assert_eq!(mail["armor"]["strength"], 13);
    assert!(mail["weapon"].is_null());
    let bow = rows.iter().find(|r| r["name"] == "Longbow").unwrap();
    assert_eq!(bow["weapon"]["range"], "150/600");
    assert_eq!(bow["weapon"]["mastery"], "Slow");
    assert!(bow["armor"].is_null());

    // Treasure and quest items live in the play-time details.
    assert_eq!(snap["details"]["treasure"], "");
    let out = s
        .state
        .update_details(
            &id,
            &json!({"treasure": "A jade idol", "questItems": "Map"}),
        )
        .unwrap();
    let out = s.check("update_details", &out);
    assert_eq!(out["details"]["treasure"], "A jade idol");
    assert_eq!(out["details"]["questItems"], "Map");
    s.finish();
}

#[test]
fn a_2014_character_converts_to_2024_with_a_contract_shaped_preview() {
    let mut s = Session::new();
    let (id, _) = s.new_character("Old Hand", Ruleset::Dnd2014);
    s.fill(&id, &[("class", "ID_WOTC_PHB_CLASS_FIGHTER")]);
    let preview = s.state.preview_convert_to_2024(&id).unwrap();
    s.check("preview_convert_to_2024", &preview);
    let applied = s.state.apply_convert_to_2024(&id).unwrap();
    let v = s.check("apply_convert_to_2024", &applied);
    assert_eq!(v["snapshot"]["ruleset"], "dnd2024");
    s.finish();
}

#[test]
fn mastery_chooser_accepts_catalog_ids_and_reports_the_same_weapon_value() {
    let mut s = Session::new();
    let (id, _) = s.new_character("Blade", Ruleset::Dnd2024);
    let snap = s.fill(&id, &[("class", "ID_2024_CLASS_FIGHTER")]);
    let cap = snap["slots"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["elementType"] == "weapon" && x["title"].to_string().contains("Mastery"))
        .count();
    assert!(cap > 0, "a 2024 Fighter has Weapon Mastery slots");
    let weapons = s
        .state
        .search_items(&ItemFilter {
            category: Some("Weapons".into()),
            limit: Some(500),
            ..ItemFilter::default()
        })
        .unwrap();
    let ids: Vec<String> = serde_json::to_value(&weapons).unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_str().unwrap().to_string())
        .collect();
    let mut chosen = None;
    for w in &ids {
        if let Ok(out) = s
            .state
            .choose_weapon_masteries(&id, std::slice::from_ref(w))
        {
            chosen = Some((w.clone(), s.check("choose_weapon_masteries", &out)));
            break;
        }
    }
    let (weapon, snap) = chosen.expect("some catalog weapon qualifies for mastery");
    let masteries = snap["play"]["masteries"].as_array().unwrap();
    assert_eq!(masteries.len(), 1, "{masteries:?}");
    // Record what the backend returns for `weapon`: the UI must match on exactly this.
    assert_eq!(
        masteries[0]["weapon"], weapon,
        "play.masteries[].weapon is the catalog id"
    );
    s.finish();
}

/// The fake provider is a `#!/bin/sh` script, so these tests are unix-only.
#[cfg(unix)]
mod fake_provider {
    use super::*;
    use borealis_ai::DetectConfig;
    use borealis_lib::ai_dto::AiWireEvent;
    use borealis_lib::ai_service::AiService;
    use borealis_lib::import_dto::{ImportEvent, ImportSink};
    use borealis_lib::import_sources::ImportSourceDto;
    use std::sync::{Arc, Mutex};

    const FAKE: &str = "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'fake-claude 1.0'; exit 0; fi\nD=\"$(dirname \"$0\")\"\ncat > \"$D/stdin.tmp\"\nA=\"$*\"\nwhile [ $# -gt 0 ]; do if [ \"$1\" = \"--system-prompt-file\" ]; then A=\"$A $(cat \"$2\")\"; fi; shift; done\nfor t in detect generate repair revise outline target; do\n case \"$A\" in *\"TASK: import_$t\"*) cat \"$D/r_$t.jsonl\"; exit 0;; esac\ndone\nexit 1\n";

    #[derive(Default)]
    struct Rec(Mutex<Vec<ImportEvent>>);
    impl ImportSink for Rec {
        fn import_event(&self, e: &ImportEvent) {
            self.0.lock().unwrap().push(e.clone());
        }
    }

    fn structured(v: &Value) -> String {
        json!({"type":"result","subtype":"success","is_error":false,"result":v.to_string(),"structured_output":v}).to_string()
    }

    #[test]
    fn import_with_the_fake_provider_matches_the_ts_contract() {
        use std::os::unix::fs::PermissionsExt;
        let mut s = Session::new();
        let bin = tempfile::tempdir().unwrap();
        let p = bin.path().join("claude");
        std::fs::write(&p, FAKE).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        for (task, v) in [
            (
                "detect",
                json!({"kind":"feat","edition":"2014","confidence":0.9,"items":[{"kind":"feat","name":"Stonewall Stance"}],"transcript":""}),
            ),
            (
                "generate",
                json!({"element":{"id":"X","name":"Stonewall Stance","type":"Feat","description":"<p>You plant your feet.</p>"},"notes":[]}),
            ),
        ] {
            std::fs::write(
                bin.path().join(format!("r_{task}.jsonl")),
                structured(&v) + "\n",
            )
            .unwrap();
        }
        let svc = Arc::new(AiService::new(DetectConfig {
            path_env: Some(bin.path().into()),
            skip_known_locations: true,
            home: Some(bin.path().into()),
            ..DetectConfig::default()
        }));
        let sources = vec![ImportSourceDto::Text {
            text: "Stonewall Stance\nPrerequisite: Strength 13".into(),
        }];
        let job = tauri::async_runtime::block_on(s.state.import_start(&svc, sources, None, false))
            .unwrap();
        let draft_id = job.draft_id.clone();
        let rec = Arc::new(Rec::default());
        tauri::async_runtime::block_on(s.state.run_import_start(&svc, job, rec.clone()));
        let events = rec.0.lock().unwrap().clone();
        assert!(!events.is_empty());
        for e in &events {
            let v = serde_json::to_value(e).unwrap();
            s.errors.extend(s.contract.event("import:event", &v));
        }
        let draft = s.state.import_get_draft(&draft_id).unwrap();
        s.check("import_get_draft", &draft);

        for e in [
            AiWireEvent::Started {
                task_id: "t".into(),
                mode: borealis_lib::ai_dto::AiMode::Explain,
            },
            AiWireEvent::Delta {
                task_id: "t".into(),
                text: "x".into(),
            },
            AiWireEvent::Tool {
                task_id: "t".into(),
                name: "n".into(),
                summary: "s".into(),
            },
            AiWireEvent::Result {
                task_id: "t".into(),
                result: json!({"kind":"explain","data":{"text":"x"}}),
            },
            AiWireEvent::Error {
                task_id: "t".into(),
                message: "m".into(),
            },
            AiWireEvent::Done {
                task_id: "t".into(),
                cancelled: false,
            },
        ] {
            let v = serde_json::to_value(&e).unwrap();
            s.errors.extend(s.contract.event("ai:event", &v));
        }
        s.finish();
    }
}
