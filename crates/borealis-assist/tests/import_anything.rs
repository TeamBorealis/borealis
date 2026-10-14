//! The "import anything" pipeline with a scripted fake provider (no CLI).

mod import_fake;

use borealis_ai::{Attachment, AttachmentKind};
use borealis_assist::import::anything::{ElementStatus, ImportSource, Role};
use borealis_assist::{
    choose_parent, continue_draft, finalize_draft, import_anything, revise_draft, AnythingRequest,
    Edition, ImportDraft, ImportStep,
};
use borealis_content::Severity;
use import_fake::*;
use serde_json::{json, Value};
// ---- Runebinder (2024 class) -----------------------------------------------------------------

const RUNE_FEATURES: [&str; 20] = [
    "Rune Carving",
    "Bound Glyphs",
    "Runebinder Subclass",
    "Ability Score Improvement",
    "Rune Surge",
    "Glyph Lore",
    "Living Script",
    "Ability Score Improvement Two",
    "Ward Etching",
    "Quick Inscription",
    "Resonant Marks",
    "Ability Score Improvement Three",
    "Deep Script",
    "Ability Score Improvement Four",
    "Rune Echo",
    "Ability Score Improvement Five",
    "Master Carver",
    "Stone Memory",
    "Epic Boon Of Runes",
    "Final Inscription",
];

fn runebinder_source() -> String {
    let mut s = String::from("# The Runebinder\n\nA scholar who binds power into carved runes. Hit die d6, Intelligence casting.\n\n");
    for (i, n) in RUNE_FEATURES.iter().enumerate() {
        s.push_str(&format!(
            "Level {}: {n}\n{n}. You carve runes and gain power.\n\n",
            i + 1
        ));
    }
    s
}

/// A Runebinder class element cloned from the SRD 2024 Wizard (a known engine-valid shape).
fn runebinder_class_reply() -> String {
    let text = std::fs::read_to_string(root().join("srd-5.2.1/classes/wizard.json")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    let wiz = file["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "ID_2024_CLASS_WIZARD")
        .unwrap()
        .to_string()
        .replace("Wizard", "Runebinder")
        .replace("WIZARD", "RUNEBINDER");
    let mut el: Value = serde_json::from_str(&wiz).unwrap();
    el["data"]["spellcasting"]["list"] = json!("Wizard");
    el["rules"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["kind"] != "equipment_package");
    json!({"element": el, "notes": []}).to_string()
}

/// The SRD Wizard's Spellcasting feature, renamed (slot and prepared stats are engine-specific).
fn runebinder_spellcasting_reply() -> String {
    let text = std::fs::read_to_string(root().join("srd-5.2.1/classes/wizard.json")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    let f = file["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "ID_2024_CLASS_FEATURE_WIZARD_SPELLCASTING")
        .unwrap()
        .to_string()
        .replace("Wizard", "Runebinder")
        .replace("WIZARD", "RUNEBINDER")
        .replace("wizard", "runebinder");
    let mut el: Value = serde_json::from_str(&f).unwrap();
    el["spellcasting"]["list"]["supports"] = json!("Wizard"); // reuse the existing spell list
    json!({"element": el, "notes": []}).to_string()
}

/// `persistent_break`: the repair of "Glyph Lore" is still invalid, so the loop must give up.
fn runebinder_fake(persistent_break: bool) -> Fake {
    let class_outline = outline(
        "Runebinder",
        json!({"hit_die": 6, "subclass_level": 3,
            "spellcasting": {"ability": "intelligence", "progression": "full", "list": "Wizard"}}),
        &RUNE_FEATURES
            .iter()
            .enumerate()
            .map(|(i, n)| (i as u32 + 1, *n))
            .collect::<Vec<_>>(),
    );
    let broken_select = |n: &str| {
        feature(
            n,
            json!({"rules": [{"kind": "select", "type": "Proficiency", "name": "Lore Pick", "supports": "ID_PROFICIENCY_NOPE_NOTHING"}]}),
        )
    };
    let dangling = feature(
        "Rune Surge",
        json!({"rules": [{"kind": "grant", "id": "ID_NO_SUCH_THING", "type": "Proficiency"}]}),
    );
    // Plan order: Spellcasting first (level 1), then the named features; the gate is not generated.
    let names: Vec<&str> = std::iter::once("Runebinder Spellcasting")
        .chain(
            RUNE_FEATURES
                .iter()
                .copied()
                .filter(|n| *n != "Runebinder Subclass"),
        )
        .collect();
    let good = |n: &str| match n {
        "Runebinder Spellcasting" => {
            let mut v: Value = serde_json::from_str(&runebinder_spellcasting_reply()).unwrap();
            v["element"]["id"] = json!("ID_RUNE_CLASS_FEATURE_RUNEBINDER_SPELLCASTING");
            v.to_string()
        }
        "Glyph Lore" => broken_select(n),
        _ => feature(n, json!({})),
    };
    let batches: Vec<Vec<String>> = names
        .chunks(8)
        .map(|c| c.iter().map(|n| good(n)).collect())
        .collect();
    // The first batch first answers with a dangling reference (a generation-loop repair).
    let mut bad_first = batches[0].clone();
    let at = names.iter().position(|n| *n == "Rune Surge").unwrap();
    bad_first[at] = dangling;
    let queue = vec![
        batch_reply(&bad_first),
        batch_reply(&batches[0]),
        batch_reply(&batches[1]),
        batch_reply(&batches[2]),
    ];
    let fixed = if persistent_break {
        broken_select("Glyph Lore")
    } else {
        feature("Glyph Lore", json!({}))
    };
    Fake::default()
        .on(
            "import_detect",
            "Classify",
            &[detect("class", "Runebinder", None, "2024")],
        )
        .on("import_outline", "Runebinder", &[class_outline])
        .on("import_repair", "element \"Glyph Lore\"", &[fixed])
        .on("import_generate", BATCH_NEEDLE, &queue)
        .on(
            "import_generate",
            "(type Class,",
            &[runebinder_class_reply()],
        )
}

#[tokio::test]
async fn runebinder_class_plans_sweeps_and_repairs() {
    let fake = runebinder_fake(false);
    let req = request(
        ImportSource::Text(runebinder_source()),
        "rune",
        Edition::Y2024,
    );
    let step = import_anything(&fake, &base(), &req, &opts(), None)
        .await
        .unwrap();
    let ImportStep::Draft(draft) = step else {
        panic!("expected a draft")
    };

    assert!(
        draft.plan.elements.len() >= 20,
        "{} elements",
        draft.plan.elements.len()
    );
    let class = draft
        .elements
        .iter()
        .find(|e| e.role == Role::Class)
        .unwrap();
    let bad: Vec<_> = draft
        .elements
        .iter()
        .filter(|e| e.status == ElementStatus::Failed)
        .map(|e| (&e.name, &e.issues))
        .collect();
    assert!(!class.json.is_null(), "{bad:?}");
    assert_eq!(class.id, "ID_RUNE_CLASS_RUNEBINDER");
    // The class grants one feature per level.
    let grants = class.json["rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["type"] == "Class Feature")
        .count();
    assert_eq!(grants, 21); // 20 levels + the planned Spellcasting feature
    assert!(
        class.json["data"]["progression"]["proficiency_bonus"]
            .as_array()
            .unwrap()
            .len()
            == 20
    );
    // Sweep passed levels 1-20.
    assert!(draft.sweep_issues.is_empty(), "{:?}", draft.sweep_issues);
    let failed: Vec<_> = draft
        .elements
        .iter()
        .filter(|e| e.status == ElementStatus::Failed)
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
    // The select with no options was caught by the sweep and repaired.
    let lore = draft
        .elements
        .iter()
        .find(|e| e.name == "Glyph Lore")
        .unwrap();
    assert_eq!(lore.status, ElementStatus::Repaired);
    assert_eq!(lore.repair_rounds, 1);
    assert_eq!(fake.calls("import_repair"), 1);
    // The dangling reference was repaired inside the generation loop.
    let surge = draft
        .elements
        .iter()
        .find(|e| e.name == "Rune Surge")
        .unwrap();
    assert_eq!(surge.status, ElementStatus::Repaired);
    // Cost bound: detect + outline + class + 3 feature batches + 1 sweep repair + 1 batch retry.
    assert_eq!(fake.calls("import_detect"), 1);
    assert_eq!(fake.calls("import_outline"), 1);
    assert_eq!(fake.calls("import_generate"), 1 + 4);
    // Source excerpts map elements back to the text.
    let carving = draft
        .elements
        .iter()
        .find(|e| e.name == "Rune Carving")
        .unwrap();
    assert!(carving
        .excerpt
        .as_ref()
        .unwrap()
        .snippet
        .contains("Rune Carving"));
    // Prompts carry a real SRD few-shot and the core id catalogue.
    let p = &fake.prompts("import_generate")[0];
    assert!(p.contains("ID_2024_CLASS_FEATURE_") || p.contains("SRD 5.2.1"));
    assert!(p.contains("ID_PROFICIENCY_SKILL_ARCANA"));
    // The whole thing commits as a loadable user-import pack.
    let done = finalize_draft(&base(), &draft).unwrap();
    assert_eq!(done.pack.provenance.origin, "user-import");
    assert!(!done.pack.provenance.redistributable);
    assert!(done.renamed.is_empty());
}

// ---- Subclass of the SRD Fighter -------------------------------------------------------------

const BANNER_SOURCE: &str = "# Banner Knight (Fighter Subclass)\n\nA knight who carries a banner.\n\n\
Rally Banner.\nYou raise a banner as a bonus action.\n\nHold The Line.\nAllies near you cannot be pushed.\n\n\
Inspiring Charge.\nYou lead allies forward.\n\nBanner Mastery.\nYour banner grants more.\n\nLegendary Standard.\nYour banner is renowned.\n";

fn banner_fake(parent_hint: Option<&str>) -> Fake {
    let sub = |name: &str| {
        json!({"element": {"id": "X", "name": name, "type": "Archetype Feature", "description": format!("<p>{name}.</p>")},
            "notes": []})
        .to_string()
    };
    let root = json!({"element": {"id": "X", "name": "Banner Knight", "type": "Archetype",
        "description": "<p>A knight.</p>"}, "notes": []})
    .to_string();
    Fake::default()
        .on(
            "import_detect",
            "Classify",
            &[detect("subclass", "Banner Knight", parent_hint, "2024")],
        )
        .on(
            "import_outline",
            "Banner Knight",
            &[outline(
                "Banner Knight",
                json!({}),
                &[
                    (3, "Rally Banner"),
                    (7, "Hold The Line"),
                    (9, "Inspiring Charge"),
                    (10, "Banner Mastery"),
                    (18, "Legendary Standard"),
                ],
            )],
        )
        .on("import_generate", "(type Archetype,", &[root])
        .on(
            "import_generate",
            BATCH_NEEDLE,
            &[batch_reply(&[
                sub("Rally Banner"),
                sub("Hold The Line"),
                sub("Inspiring Charge"),
                sub("Banner Mastery"),
                sub("Legendary Standard"),
            ])],
        )
}

async fn banner_draft() -> (Fake, ImportDraft) {
    let fake = banner_fake(Some("Fighter"));
    let req = request(
        ImportSource::Text(BANNER_SOURCE.into()),
        "banner",
        Edition::Y2024,
    );
    let ImportStep::Draft(d) = import_anything(&fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    else {
        panic!("expected a draft")
    };
    (fake, *d)
}

#[tokio::test]
async fn subclass_resolves_parent_and_lands_on_fighter_levels() {
    let (_fake, draft) = banner_draft().await;
    let link = draft.plan.groups[0]
        .parent
        .as_ref()
        .expect("parent resolved");
    assert_eq!(link.class_id, "ID_2024_CLASS_FIGHTER");
    assert_eq!(link.supports, "Fighter Subclass");
    let mut levels: Vec<u32> = draft
        .elements
        .iter()
        .filter(|e| e.role == Role::SubclassFeature)
        .filter_map(|e| e.level)
        .collect();
    levels.sort_unstable();
    assert_eq!(levels, vec![3, 7, 10, 10, 18]);
    assert!(draft
        .plan
        .notes
        .iter()
        .any(|n| n.contains("Inspiring Charge") && n.contains("moved to 10")));
    let root = draft
        .elements
        .iter()
        .find(|e| e.role == Role::Subclass)
        .unwrap();
    assert_eq!(root.json["supports"], json!(["Fighter Subclass"]));
    assert!(draft.sweep_issues.is_empty(), "{:?}", draft.sweep_issues);
    assert!(draft
        .elements
        .iter()
        .all(|e| e.status != ElementStatus::Failed));
}

#[tokio::test]
async fn ambiguous_parent_returns_needs_choice_then_continues() {
    let fake = banner_fake(Some("Cleric or Druid"));
    let req = request(
        ImportSource::Text(BANNER_SOURCE.into()),
        "banner",
        Edition::Y2024,
    );
    let step = import_anything(&fake, &base(), &req, &opts(), None)
        .await
        .unwrap();
    let ImportStep::NeedsChoice(draft) = step else {
        panic!("expected a choice")
    };
    let choice = draft.plan.open_choices()[0].clone();
    let names: Vec<&str> = choice
        .candidates
        .iter()
        .map(|c| c.link.class_name.as_str())
        .collect();
    assert_eq!(names, vec!["Cleric", "Druid"]);
    assert!(draft.elements.is_empty());
    assert_eq!(fake.calls("import_generate"), 0);

    let picked = choose_parent(&base(), &draft, "Banner Knight", "ID_2024_CLASS_CLERIC").unwrap();
    let ImportStep::Draft(done) = continue_draft(&fake, &base(), picked, &opts(), None)
        .await
        .unwrap()
    else {
        panic!("expected a draft")
    };
    assert_eq!(
        done.plan.groups[0].parent.as_ref().unwrap().class_name,
        "Cleric"
    );
    assert!(done
        .elements
        .iter()
        .all(|e| e.status != ElementStatus::Pending));
}

#[tokio::test]
async fn revise_changes_one_element_and_revalidates() {
    let (_f, draft) = banner_draft().await;
    let target = draft
        .elements
        .iter()
        .find(|e| e.name == "Hold The Line")
        .unwrap()
        .clone();
    let revised = json!({"element": {"id": "X", "name": "Hold The Line", "type": "Archetype Feature",
        "description": "<p>You and allies within 10 feet have resistance to fire damage.</p>"}, "notes": []})
    .to_string();
    let revised2 = revised.clone();
    let fake = Fake::default().on("import_revise", "fire", &[revised]);
    let next = revise_draft(
        &fake,
        &base(),
        &draft,
        "the level 7 feature should grant resistance to fire",
        Some(&target.id),
        &opts(),
        None,
    )
    .await
    .unwrap();
    let after = next.element(&target.id).unwrap();
    assert!(after.json["description"].as_str().unwrap().contains("fire"));
    assert_eq!(after.revisions, 1);
    assert_eq!(after.status, ElementStatus::Ok);
    assert_eq!(after.level, Some(7));
    // Nothing else changed.
    for (a, b) in draft.elements.iter().zip(&next.elements) {
        if a.id != target.id {
            assert_eq!(a.json, b.json);
        }
    }
    assert!(next.sweep_issues.is_empty());
    assert_eq!(fake.calls("import_revise"), 1);
    assert!(fake.prompts("import_revise")[0].contains("resistance to fire"));
    // Without an element id the model picks the target first.
    let fake2 = Fake::default()
        .on(
            "import_target",
            "fire",
            &[json!({"element_id": target.id}).to_string()],
        )
        .on("import_revise", "fire", &[revised2]);
    let picked = revise_draft(&fake2, &base(), &draft, "fire", None, &opts(), None)
        .await
        .unwrap();
    assert_eq!(picked.element(&target.id).unwrap().revisions, 1);
    assert_eq!(fake2.calls("import_target"), 1);
}

#[tokio::test]
async fn finalize_builds_loadable_pack_and_renames_collisions() {
    let (_f, draft) = banner_draft().await;
    let done = finalize_draft(&base(), &draft).unwrap();
    assert!(done.renamed.is_empty());
    let ids: Vec<&str> = done
        .pack
        .content
        .elements
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len());
    assert!(ids.iter().all(|i| i.starts_with("ID_BANNER_")));
    let store = base()
        .add_file(
            "t",
            borealis_content::file::PackOrigin::Custom,
            done.pack.content.clone(),
        )
        .build();
    assert!(!store
        .validate()
        .iter()
        .any(|i| i.severity == Severity::Error
            && i.element
                .as_ref()
                .is_some_and(|e| ids.contains(&e.as_str()))));
    // Same text, rebuilt: deterministic.
    assert_eq!(
        finalize_draft(&base(), &draft).unwrap().pack.content,
        done.pack.content
    );

    // A colliding id is renamed and its references follow.
    let clash = draft.clone();
    let lib = "ID_2024_CLASS_FEATURE_FIGHTER_SECOND_WIND";
    let old = clash.elements[1].id.clone();
    let text = serde_json::to_string(&clash).unwrap().replace(&old, lib);
    let clash: ImportDraft = serde_json::from_str(&text).unwrap();
    let fixed = finalize_draft(&base(), &clash).unwrap();
    assert_eq!(fixed.renamed.len(), 1);
    assert!(fixed.pack.content.elements.iter().all(|e| e.id != lib));

    // Unready drafts are refused.
    let mut broken = draft.clone();
    broken.elements[0].status = ElementStatus::Failed;
    assert!(finalize_draft(&base(), &broken).is_err());
}

#[tokio::test]
async fn attachments_reach_detect_and_use_the_transcript() {
    let png = Attachment {
        kind: AttachmentKind::Image {
            mime: "image/png".into(),
        },
        bytes: vec![1, 2, 3],
        name: "page1.png".into(),
    };
    // The image is transcribed verbatim first; detection then reads that text.
    let transcript = json!({"text": BANNER_SOURCE, "headings": ["Banner Knight"]}).to_string();
    let fake = banner_fake(None).on("import_transcribe", "page1.png", &[transcript]);
    let req = request(
        ImportSource::Attachments(vec![png]),
        "banner",
        Edition::Y2024,
    );
    let step = import_anything(&fake, &base(), &req, &opts(), None)
        .await
        .unwrap();
    assert_eq!(
        fake.attachments_seen.lock().unwrap().first().copied(),
        Some(1)
    );
    assert_eq!(fake.calls("import_transcribe"), 1);
    assert!(fake.prompts("import_detect")[0].contains("Rally Banner"));
    let ImportStep::NeedsChoice(d) = step else {
        panic!("expected a choice")
    };
    assert_eq!(d.source_kind, "attachments");
    assert!(d.source_text.contains("Rally Banner"));
}

// ---- Cost bound and repair-loop behaviour ------------------------------------------------------

fn spellcasting_element() -> Value {
    let mut v: Value = serde_json::from_str(&runebinder_spellcasting_reply()).unwrap();
    v["element"]["id"] = json!("ID_RUNE_CLASS_FEATURE_RUNEBINDER_SPELLCASTING");
    v["element"].take()
}

fn tiny_class_fake(batch: Vec<Value>) -> Fake {
    let class_outline = outline(
        "Runebinder",
        json!({"hit_die": 6, "subclass_level": 3,
            "spellcasting": {"ability": "intelligence", "progression": "full", "list": "Wizard"}}),
        &[
            (1, "Rune Carving"),
            (3, "Runebinder Subclass"),
            (5, "Glyph Lore"),
        ],
    );
    let replies: Vec<String> = batch
        .into_iter()
        .map(|b| json!({ "elements": b }).to_string())
        .collect();
    Fake::default()
        .on(
            "import_detect",
            "Classify",
            &[detect("class", "Runebinder", None, "2024")],
        )
        .on("import_outline", "Runebinder", &[class_outline])
        .on("import_generate", BATCH_NEEDLE, &replies)
        .on(
            "import_generate",
            "(type Class,",
            &[runebinder_class_reply()],
        )
}

fn wrapped(el: Value) -> Value {
    json!({"element": el, "notes": []})
}

fn good_tiny_batch() -> Value {
    let plain = |n: &str| -> Value {
        wrapped(serde_json::from_str::<Value>(&feature(n, json!({}))).unwrap()["element"].clone())
    };
    json!([
        wrapped(spellcasting_element()),
        plain("Rune Carving"),
        plain("Glyph Lore")
    ])
}

#[tokio::test]
async fn small_class_import_costs_four_calls() {
    let fake = tiny_class_fake(vec![good_tiny_batch()]);
    let req = request(ImportSource::Text("# The Runebinder\n\nLevel 1: Rune Carving\nRune Carving.\n\nLevel 5: Glyph Lore\nGlyph Lore.\n".into()), "rune", Edition::Y2024);
    let ImportStep::Draft(d) = import_anything(&fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    else {
        panic!("expected a draft")
    };
    // No placeholder elements for levels without a feature: 3 features + gate + class.
    assert_eq!(d.plan.elements.len(), 5, "{:?}", d.plan.elements);
    // detect + outline + class + ONE batch for all features.
    let total = [
        "import_detect",
        "import_outline",
        "import_generate",
        "import_repair",
    ]
    .iter()
    .map(|t| fake.calls(t))
    .sum::<usize>();
    assert_eq!(total, 4);
    assert!(d.sweep_issues.is_empty(), "{:?}", d.sweep_issues);
    let failed: Vec<_> = d
        .elements
        .iter()
        .filter(|e| e.status == ElementStatus::Failed)
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
}

#[tokio::test]
async fn invalid_first_batch_is_repaired_once() {
    let mut bad = good_tiny_batch();
    bad[2]["element"]["rules"] =
        json!([{"kind": "grant", "id": "ID_NO_SUCH_THING", "type": "Proficiency"}]);
    let fake = tiny_class_fake(vec![bad, good_tiny_batch()]);
    let req = request(
        ImportSource::Text("# The Runebinder\n\nGlyph Lore.\n".into()),
        "rune",
        Edition::Y2024,
    );
    let ImportStep::Draft(d) = import_anything(&fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    else {
        panic!("expected a draft")
    };
    // One feature call failed validation, the repair attempt was valid: 2 batch calls + class.
    assert_eq!(fake.calls("import_generate"), 3);
    let lore = d.elements.iter().find(|e| e.name == "Glyph Lore").unwrap();
    assert_eq!(lore.status, ElementStatus::Repaired);
    assert!(fake.prompts("import_generate")[1].contains("ID_NO_SUCH_THING"));
    assert!(d.elements.iter().all(|e| e.status != ElementStatus::Failed));
    assert!(d.sweep_issues.is_empty());
}

#[tokio::test]
async fn generation_gives_up_after_max_repairs_with_a_clear_error() {
    let mut bad = good_tiny_batch();
    bad[2]["element"]["rules"] =
        json!([{"kind": "grant", "id": "ID_NO_SUCH_THING", "type": "Proficiency"}]);
    let fake = tiny_class_fake(vec![bad]); // the last reply sticks: always invalid
    let req = request(
        ImportSource::Text("# The Runebinder\n\nGlyph Lore.\n".into()),
        "rune",
        Edition::Y2024,
    );
    let ImportStep::Draft(d) = import_anything(&fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    else {
        panic!("expected a draft")
    };
    // 1 + max_repairs (2) attempts, then the batch's elements are Failed with the reason.
    assert_eq!(
        fake.prompts("import_generate")
            .iter()
            .filter(|p| p.contains(BATCH_NEEDLE))
            .count(),
        3
    );
    let lore = d.elements.iter().find(|e| e.name == "Glyph Lore").unwrap();
    assert_eq!(lore.status, ElementStatus::Failed);
    assert!(
        lore.issues[0].contains("generation failed") && lore.issues[0].contains("ID_NO_SUCH_THING"),
        "{:?}",
        lore.issues
    );
    assert!(finalize_draft(&base(), &d).is_err());
}

#[tokio::test]
async fn sweep_repair_gives_up_after_the_round_limit() {
    let fake = runebinder_fake(true); // "Glyph Lore" stays an empty select after every repair
    let req = request(
        ImportSource::Text(runebinder_source()),
        "rune",
        Edition::Y2024,
    );
    let ImportStep::Draft(d) = import_anything(&fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    else {
        panic!("expected a draft")
    };
    assert_eq!(
        fake.calls("import_repair"),
        borealis_assist::import::anything::MAX_REPAIR_ROUNDS as usize
    );
    let lore = d.elements.iter().find(|e| e.name == "Glyph Lore").unwrap();
    assert_eq!(lore.status, ElementStatus::Failed);
    assert_eq!(
        lore.repair_rounds,
        borealis_assist::import::anything::MAX_REPAIR_ROUNDS
    );
    assert!(
        lore.issues.iter().any(|i| i.contains("offers no options")),
        "{:?}",
        lore.issues
    );
    assert!(d
        .sweep_issues
        .iter()
        .any(|i| i.message.contains("offers no options")));
    assert!(finalize_draft(&base(), &d).is_err());
}

// ---- rule catalogue, normalisation, report, batching (import-pipeline-fix-1) --------------------

fn tiny_request() -> AnythingRequest {
    request(
        ImportSource::Text("# The Runebinder\n\nRune Carving.\n\nGlyph Lore.\n".into()),
        "rune",
        Edition::Y2024,
    )
}

/// The tiny class whose "Glyph Lore" carries `rules`; repairs of it answer `repairs` in turn.
fn tiny_with_lore_rules(rules: Value, repairs: &[Value]) -> Fake {
    let mut batch = good_tiny_batch();
    batch[2]["element"]["rules"] = rules;
    let replies: Vec<String> = repairs
        .iter()
        .map(|r| feature("Glyph Lore", json!({ "rules": r })))
        .collect();
    let fake = tiny_class_fake(vec![batch]);
    if replies.is_empty() {
        fake
    } else {
        fake.on("import_repair", "\"Glyph Lore\"", &replies)
    }
}

async fn draft_of(fake: &Fake) -> ImportDraft {
    let ImportStep::Draft(d) = import_anything(fake, &base(), &tiny_request(), &opts(), None)
        .await
        .unwrap()
    else {
        panic!("expected a draft")
    };
    *d
}

fn lore(d: &ImportDraft) -> &borealis_assist::import::anything::DraftElement {
    d.elements.iter().find(|e| e.name == "Glyph Lore").unwrap()
}

#[tokio::test]
async fn prompts_carry_the_rule_catalogue_from_the_registry() {
    let fake = tiny_with_lore_rules(json!([]), &[]);
    draft_of(&fake).await;
    for p in fake.prompts("import_generate") {
        assert!(p.contains("resource*: id, name, max"), "no catalogue");
        assert!(p.contains("Any other kind or field is rejected"));
    }
}

#[tokio::test]
async fn invented_rule_kind_is_repaired_with_the_exact_errors() {
    let invented = json!([{"kind": "strike_bonus", "amount": 2}]);
    let clean = json!([{"kind": "stat", "name": "speed", "value": "5"}]);
    let fake = tiny_with_lore_rules(invented, &[clean]);
    let d = draft_of(&fake).await;
    assert_eq!(fake.calls("import_repair"), 1);
    let p = &fake.prompts("import_repair")[0];
    assert!(
        p.contains("\"strike_bonus\"") && p.contains("rule catalogue"),
        "{p}"
    );
    let l = lore(&d);
    assert_eq!(l.status, ElementStatus::Repaired);
    assert!(l.rule_errors.is_empty());
    assert_eq!(l.json["rules"][0]["kind"], "stat");
}

#[tokio::test]
async fn rule_errors_that_survive_repairs_are_removed_and_reported() {
    let invented = json!([{"kind": "strike_bonus", "amount": 2},
        {"kind": "stat", "name": "speed", "value": "5"}]);
    let fake = tiny_with_lore_rules(invented.clone(), &[invented]);
    let d = draft_of(&fake).await;
    assert_eq!(
        fake.calls("import_repair"),
        borealis_assist::import::anything::MAX_REPAIR_ROUNDS as usize
    );
    let l = lore(&d);
    assert_eq!(l.status, ElementStatus::NeedsReview);
    assert_eq!(l.rule_errors.len(), 1, "{:?}", l.rule_errors);
    assert!(l.issues.iter().any(|i| i.contains("strike_bonus")));
    assert_eq!(l.json["rules"].as_array().unwrap().len(), 1);

    let done = finalize_draft(&base(), &d).unwrap();
    let report = done.pack.report.clone().expect("report");
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].element, l.id);
    let dir = std::env::temp_dir().join(format!("borealis-report-test-{}", std::process::id()));
    done.pack.write_to_dir(&dir).unwrap();
    let text = std::fs::read_to_string(dir.join("report.json")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["format"], "borealis-import-report/1");
    assert!(v["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("strike_bonus"));
}

#[tokio::test]
async fn resource_rules_are_normalised_without_a_model_call() {
    let rules = json!([
        {"kind": "resource", "name": "Carving Charges", "max": 2, "recharge": "short rest"},
        {"kind": "resource_cost", "resource": "Carving Charges", "cost": 1}
    ]);
    let fake = tiny_with_lore_rules(rules, &[]);
    let d = draft_of(&fake).await;
    assert_eq!(fake.calls("import_repair"), 0);
    let l = lore(&d);
    let r = &l.json["rules"];
    assert_eq!(r[0]["id"], "carving charges");
    assert_eq!(r[0]["max"], json!({"type": "fixed", "value": 2}));
    assert_eq!(r[0]["recharge"], json!(["short_rest", "long_rest"]));
    assert_eq!(r[1]["resource"], "carving charges");
    assert_eq!(l.normalised.len(), 4, "{:?}", l.normalised);
    let report = finalize_draft(&base(), &d).unwrap().pack.report.unwrap();
    assert_eq!(report.normalised.len(), 4);
    assert!(report.errors.is_empty());
}

/// An invented mixed source: 2 Fighter subclasses (3 features each), 3 feats, 2 magic items and
/// a weapon property no element kind covers.
fn mixed_fake() -> Fake {
    let detect = json!({"kind": "mixed", "edition": "2024", "confidence": 0.9, "items": [
        {"kind": "subclass", "name": "Banner Knight", "parent_hint": "Fighter"},
        {"kind": "subclass", "name": "Shield Warden", "parent_hint": "Fighter"},
        {"kind": "feat", "name": "Quick Draw"}, {"kind": "feat", "name": "Steady Hand"},
        {"kind": "feat", "name": "Iron Gut"},
        {"kind": "item", "name": "Lantern Of Echoes"}, {"kind": "item", "name": "Copper Key"}],
        "other": [{"name": "Thunderous", "what": "weapon property"}]});
    let banner = [
        (3, "Rally Banner"),
        (7, "Hold The Line"),
        (10, "Banner Mastery"),
    ];
    let warden = [(3, "Shield Wall"), (7, "Brace"), (10, "Bulwark")];
    let outlines = json!({"outlines": [outline_of("Banner Knight", &banner),
        outline_of("Shield Warden", &warden)]});
    let feats: Vec<String> = banner
        .iter()
        .chain(&warden)
        .map(|(_, n)| el_reply(n, "Archetype Feature"))
        .collect();
    let of = |names: &[&str], ty: &str| -> Vec<String> {
        vec![batch_reply(
            &names.iter().map(|n| el_reply(n, ty)).collect::<Vec<_>>(),
        )]
    };
    Fake::default()
        .on("import_detect", "Classify", &[detect.to_string()])
        .on("import_outline", "in ONE answer", &[outlines.to_string()])
        .on(
            "import_generate",
            "of type Archetype Feature (",
            &[batch_reply(&feats)],
        )
        .on(
            "import_generate",
            "of type Archetype (",
            &of(&["Banner Knight", "Shield Warden"], "Archetype"),
        )
        .on(
            "import_generate",
            "of type Feat (",
            &of(&["Quick Draw", "Steady Hand", "Iron Gut"], "Feat"),
        )
        .on(
            "import_generate",
            "of type Magic Item (",
            &of(&["Lantern Of Echoes", "Copper Key"], "Magic Item"),
        )
}

#[tokio::test]
async fn mixed_source_is_batched_by_type_and_keeps_what_has_no_kind_as_text() {
    let fake = mixed_fake();
    let source = "Banner Knight. Shield Warden. Quick Draw. Steady Hand. Iron Gut. \
Lantern Of Echoes. Copper Key. Thunderous.";
    let req = request(ImportSource::Text(source.into()), "mix", Edition::Y2024);
    let ImportStep::Draft(d) = import_anything(&fake, &base(), &req, &opts(), None)
        .await
        .unwrap()
    else {
        panic!("expected a draft")
    };
    let bad: Vec<_> = d
        .elements
        .iter()
        .filter(|e| matches!(e.status, ElementStatus::Failed | ElementStatus::Pending))
        .map(|e| (&e.name, &e.issues))
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
    // detect + ONE outline call + 4 batches (subclass features of both subclasses, both roots,
    // feats, items). One call per outline, root and standalone element would be 12.
    assert_eq!(fake.calls("import_detect"), 1);
    assert_eq!(fake.calls("import_outline"), 1);
    assert_eq!(fake.calls("import_generate"), 4);
    assert_eq!(fake.calls("import_repair"), 0);
    let report = finalize_draft(&base(), &d).unwrap().pack.report.unwrap();
    assert_eq!(report.entries.len(), 1, "{:?}", report.entries);
    assert_eq!(report.entries[0].source, "Thunderous");
    // No element kind mechanises it: its text is kept as a text-only element (no model call).
    assert_eq!(report.entries[0].outcome, "text_only");
    assert_eq!(report.entries[0].elements.len(), 1);
    let text = d.elements.iter().find(|e| e.name == "Thunderous").unwrap();
    assert_eq!(text.ty, "Rules Glossary Entry");
    assert!(text.json["description"]
        .as_str()
        .unwrap()
        .contains("Thunderous"));
}
