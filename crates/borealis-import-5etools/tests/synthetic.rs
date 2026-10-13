//! Pipeline behaviour on a small invented data folder (no real 5etools content).

mod support;

use borealis_import_5etools::{
    import_folder, import_folder_with_progress, ImportOptions, Phase, ProgressEvent,
};
use serde_json::{json, Value};
use std::path::Path;
use support::find;

fn write(dir: &Path, rel: &str, v: &Value) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, v.to_string()).unwrap();
}

/// Invented content of the made-up source `ZZZ`; nothing here is flagged SRD.
fn invented() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    write(
        d,
        "races.json",
        &json!({
            "race": [
                {"name": "Gloamling", "source": "ZZZ", "size": ["S"], "speed": 25,
                 "languageProficiencies": [{"common": true}],
                 "entries": [{"type": "entries", "name": "Dusk Sight", "entries": ["You see in dim light."]}]},
                {"name": "Dusk Gloamling", "source": "ZZZ", "_copy": {"name": "Gloamling", "source": "ZZZ"}, "speed": 30},
                {"name": "Orphan", "source": "ZZZ", "_copy": {"name": "Nowhere", "source": "ZZZ"}},
                {"name": "Broken", "source": "ZZZ", "size": "huge"},
                {"name": "Sourceless"}
            ],
            "subrace": [
                {"name": "Deep", "source": "ZZZ", "raceName": "Gloamling", "raceSource": "ZZZ", "entries": ["Deeper."]}
            ]
        }),
    );
    write(
        d,
        "fluff-races.json",
        &json!({"raceFluff": [{"name": "Gloamling", "source": "ZZZ", "entries": ["Pale folk of the twilight."]}]}),
    );
    write(
        d,
        "spells/spells-zzz.json",
        &json!({"spell": [{"name": "Zap", "source": "ZZZ", "level": 1, "school": "V",
            "time": [{"number": 1, "unit": "action"}], "range": {"type": "point", "distance": {"type": "feet", "amount": 30}},
            "components": {"v": true}, "duration": [{"type": "instant"}], "entries": ["A harmless spark."]}]}),
    );
    write(
        d,
        "items-base.json",
        &json!({"baseitem": [{"name": "Blade", "source": "ZZZ", "type": "M", "weaponCategory": "martial",
            "dmg1": "1d8", "dmgType": "S", "weight": 3, "value": 1500}]}),
    );
    write(d, "class/class-broken.json", &json!("not an object"));
    std::fs::write(d.join("languages.json"), "{ not json").unwrap();
    dir
}

#[test]
fn converts_invented_content_and_reports_every_problem() {
    let dir = invented();
    let r = import_folder(dir.path(), &ImportOptions::default()).unwrap();
    assert_eq!(r.packs.len(), 1);
    let pack = &r.packs[0];
    assert_eq!(pack.manifest.id, "5etools-zzz");
    assert!(find(&r, "ID_5ET_ZZZ_RACE_GLOAMLING").is_some());
    let copy = serde_json::to_value(find(&r, "ID_5ET_ZZZ_RACE_DUSK_GLOAMLING").unwrap()).unwrap();
    assert!(
        copy.to_string().contains("\"30\""),
        "_copy override applied: {copy}"
    );
    assert!(find(&r, "ID_5ET_ZZZ_SUBRACE_GLOAMLING_DEEP").is_some());
    assert!(find(&r, "ID_5ET_ZZZ_SPELL_ZAP").is_some());
    assert!(find(&r, "ID_5ET_ZZZ_ITEM_BLADE").is_some());
    // a subrace exists, so the race offers the choice
    let race = serde_json::to_string(find(&r, "ID_5ET_ZZZ_RACE_GLOAMLING").unwrap()).unwrap();
    assert!(race.contains("\"Sub Race\""));

    let skipped: Vec<(&str, &str)> = r
        .report
        .skipped
        .iter()
        .map(|s| (s.element.as_str(), s.reason.as_str()))
        .collect();
    assert!(skipped
        .iter()
        .any(|(e, why)| e.starts_with("Orphan") && why.contains("_copy")));
    assert!(skipped
        .iter()
        .any(|(e, why)| e.starts_with("Broken") && why.contains("invalid shape")));
    assert!(skipped
        .iter()
        .any(|(e, why)| e.starts_with("Sourceless") && why.contains("no source")));
    assert!(r
        .report
        .warnings
        .iter()
        .any(|w| w.contains("languages.json")));
    assert!(r
        .report
        .warnings
        .iter()
        .any(|w| w.contains("class/class-broken.json")));

    // counts are consistent: every entry is converted or skipped, exactly once
    for f in &r.report.files {
        for (kind, c) in &f.kinds {
            assert_eq!(c.converted + c.skipped, c.total, "{} {kind}", f.path);
        }
    }
}

#[test]
fn fluff_is_opt_in() {
    let dir = invented();
    let plain = import_folder(dir.path(), &ImportOptions::default()).unwrap();
    let with = import_folder(
        dir.path(),
        &ImportOptions {
            include_fluff: true,
            ..ImportOptions::default()
        },
    )
    .unwrap();
    let text = |r: &borealis_import_5etools::ImportResult| {
        find(r, "ID_5ET_ZZZ_RACE_GLOAMLING")
            .unwrap()
            .description
            .clone()
    };
    assert!(!text(&plain).contains("Pale folk"));
    assert!(text(&with).contains("Pale folk of the twilight."));
}

#[test]
fn filters_apply_before_conversion() {
    let dir = invented();
    let srd_only = import_folder(
        dir.path(),
        &ImportOptions {
            only_srd: true,
            ..ImportOptions::default()
        },
    )
    .unwrap();
    assert!(srd_only.packs.is_empty(), "nothing invented is SRD");
    assert!(srd_only
        .report
        .skipped
        .iter()
        .all(|s| !s.element.starts_with("Gloamling")));

    let other = import_folder(dir.path(), &ImportOptions::default().with_sources(["PHB"])).unwrap();
    assert!(other.packs.is_empty());

    let wrong_edition = import_folder(
        dir.path(),
        &ImportOptions {
            ruleset: Some(borealis_content::Ruleset::Dnd2024),
            ..ImportOptions::default()
        },
    )
    .unwrap();
    assert!(wrong_edition.packs.is_empty());
}

#[test]
fn progress_is_ordered_and_ends_with_done() {
    let dir = invented();
    let mut events: Vec<ProgressEvent> = Vec::new();
    import_folder_with_progress(dir.path(), &ImportOptions::default(), &mut |e| {
        events.push(e.clone());
    })
    .unwrap();
    let phases: Vec<Phase> = events.iter().map(|e| e.phase).collect();
    let mut deduped = phases.clone();
    deduped.dedup();
    assert_eq!(
        deduped,
        vec![
            Phase::Scanning,
            Phase::Reading,
            Phase::Resolving,
            Phase::Converting,
            Phase::Done
        ]
    );
    let converting: Vec<&ProgressEvent> = events
        .iter()
        .filter(|e| e.phase == Phase::Converting)
        .collect();
    assert!(converting.windows(2).all(|w| w[0].current <= w[1].current));
    assert!(converting.iter().all(|e| e.current <= e.total));
    let last = events.last().unwrap();
    assert_eq!(
        (last.current, last.total),
        (
            converting.last().unwrap().total,
            converting.last().unwrap().total
        )
    );
}

#[test]
fn rejects_folders_that_are_not_5etools_data() {
    let empty = tempfile::tempdir().unwrap();
    let err = import_folder(empty.path(), &ImportOptions::default()).unwrap_err();
    assert!(err
        .to_string()
        .contains("does not look like a 5etools data folder"));
    let err = import_folder(&empty.path().join("missing"), &ImportOptions::default()).unwrap_err();
    assert!(err.to_string().contains("not a directory"));
}
