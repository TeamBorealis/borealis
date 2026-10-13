//! Fixture sweep: migrate all 72 Aurora files and check the expected numbers.

mod common;

use borealis_content::schema::{validate_document, DocumentKind};
use borealis_content::{
    ContentFile, ContentStoreBuilder, KnownType, PackManifest, PackOrigin, Severity,
};
use borealis_migrate::{migrate_path, MigrateOptions, MigrateOutput};
use serde_json::Value;

fn run(origin: PackOrigin) -> Option<MigrateOutput> {
    let dir = common::require(
        "Aurora XML fixtures",
        common::fixtures_dir(),
        "BOREALIS_AURORA_FIXTURES",
    )?;
    let opts = MigrateOptions {
        origin,
        ..MigrateOptions::default()
    };
    Some(migrate_path(&dir, &opts).expect("migration runs"))
}

#[test]
fn counts_match_the_spec() {
    let Some(out) = run(PackOrigin::Core) else {
        return;
    };
    let r = &out.report;
    let errors: Vec<_> = r
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "errors: {errors:#?}");
    assert_eq!(r.summary.files_read, 72);
    assert_eq!(r.summary.content_files, 72);
    assert_eq!(r.totals.elements, 1919);
    assert_eq!(r.totals.dropped_elements, 0);
    assert_eq!(r.totals.rules, 1904);
    assert_eq!(r.totals.rules_by_kind["stat"], 853);
    assert_eq!(r.totals.rules_by_kind["grant"], 767);
    assert_eq!(r.totals.rules_by_kind["select"], 284);
    assert_eq!(r.totals.appends, 0);

    // expected counts per type
    let by_type = &r.totals.elements_by_type;
    for (ty, n) in [
        ("Magic Item", 370),
        ("Spell", 361),
        ("Class Feature", 253),
        ("Item", 160),
        ("Proficiency", 140),
        ("Grants", 125),
        ("Archetype Feature", 89),
        ("Deity", 61),
        ("Racial Trait", 58),
        ("Weapon", 37),
        ("Condition", 39),
        ("Support", 29),
        ("Level", 20),
        ("Language", 18),
        ("Ability Score Improvement", 18),
        ("Weapon Property", 18),
        ("Damage Type", 14),
        ("Armor", 13),
        ("Class", 12),
        ("Archetype", 12),
        ("Alignment", 9),
        ("Weapon Group", 9),
        ("Race", 9),
        ("Size", 7),
        ("Property", 6),
        ("Weapon Category", 5),
        ("Sub Race", 4),
        ("Vision", 4),
        ("Armor Group", 4),
        ("Spellcasting Focus Group", 4),
        ("Source", 3),
        ("Option", 3),
        ("Race Variant", 1),
        ("Feat", 1),
        ("Background", 1),
        ("Background Feature", 1),
        ("Ignore", 1),
    ] {
        assert_eq!(by_type.get(ty), Some(&n), "type {ty}");
    }
    assert_eq!(by_type.values().sum::<usize>(), 1919);
    for ty in by_type.keys() {
        assert!(
            KnownType::parse(ty).is_some(),
            "type {ty} is not in the known-types table"
        );
    }
    // the spec's other raw counts
    assert_eq!(r.totals.unknown_children.get("element/set"), None);
}

#[test]
fn every_output_file_is_schema_valid_and_rewrites_identically() {
    let Some(out) = run(PackOrigin::Core) else {
        return;
    };
    let mut content_files = 0;
    for f in &out.files {
        if f.path == "pack.json" {
            let v: Value = serde_json::from_str(&f.contents).unwrap();
            assert!(validate_document(DocumentKind::Pack, &v)
                .unwrap()
                .is_empty());
            continue;
        }
        let value: Value = serde_json::from_str(&f.contents).unwrap();
        let violations = validate_document(DocumentKind::Content, &value).unwrap();
        assert!(violations.is_empty(), "{}: {violations:#?}", f.path);
        // JSON -> model -> JSON is byte-identical
        let parsed = ContentFile::from_json_str(&f.contents).unwrap();
        assert_eq!(parsed.to_json_pretty().unwrap(), f.contents, "{}", f.path);
        content_files += 1;
    }
    assert_eq!(content_files, 72);
}

#[test]
fn migration_is_deterministic() {
    let (Some(a), Some(b)) = (run(PackOrigin::Core), run(PackOrigin::Core)) else {
        return;
    };
    assert_eq!(a.files, b.files);
    assert_eq!(a.report, b.report);
}

#[test]
fn migrated_pack_loads_into_a_store_with_all_elements() {
    let Some(out) = run(PackOrigin::Core) else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    borealis_migrate::write_output(tmp.path(), &out).unwrap();
    let manifest = PackManifest::from_json_str(
        &std::fs::read_to_string(tmp.path().join("pack.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest.files.len(), 72);
    let store = ContentStoreBuilder::new()
        .add_pack_dir(tmp.path())
        .unwrap()
        .build();
    assert_eq!(store.len(), 1919);
    assert!(store.diagnostics().is_empty(), "{:#?}", store.diagnostics());
    assert_eq!(store.by_type("Class").count(), 12);
    let fighter = store.element("ID_WOTC_PHB_CLASS_FIGHTER").expect("fighter");
    assert_eq!(
        fighter.multiclass.as_ref().unwrap().id,
        "ID_WOTC_PHB_MULTICLASS_FIGHTER"
    );
    // second pass: store built from the files is the same as validating the in-memory output
    let issues = store.validate();
    assert!(
        issues.iter().all(|i| i.severity != Severity::Error),
        "{issues:#?}"
    );
}

#[test]
fn xml_and_json_agree_on_structure_per_element() {
    // Independent cross-check: count nodes straight from the XML DOM and compare with
    // the converted JSON element by element.
    let Some(dir) = common::require(
        "Aurora XML fixtures",
        common::fixtures_dir(),
        "BOREALIS_AURORA_FIXTURES",
    ) else {
        return;
    };
    for path in common::files_with_ext(&dir, "xml") {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        let dom = borealis_migrate::dom::parse_document(&text).unwrap();
        let conv = borealis_migrate::convert_xml(&text, &name);
        let Some(borealis_migrate::Document::Content(file)) = conv.document else {
            panic!("{name}")
        };
        let xml_elements: Vec<_> = dom.elements().filter(|e| e.name == "element").collect();
        assert_eq!(xml_elements.len(), file.elements.len(), "{name}");
        for (x, j) in xml_elements.iter().zip(&file.elements) {
            assert_eq!(x.attr("id").unwrap().trim(), j.id, "{name}");
            let count = |tag: &str| {
                x.child("rules")
                    .map_or(0, |r| r.elements().filter(|e| e.name == tag).count())
            };
            let json_count = |kind: borealis_content::RuleKind| {
                j.rules.iter().filter(|r| r.kind() == kind).count()
            };
            assert_eq!(
                count("grant"),
                json_count(borealis_content::RuleKind::Grant),
                "{name} {}",
                j.id
            );
            assert_eq!(
                count("select"),
                json_count(borealis_content::RuleKind::Select),
                "{name} {}",
                j.id
            );
            assert_eq!(
                count("stat"),
                json_count(borealis_content::RuleKind::Stat),
                "{name} {}",
                j.id
            );
            let setters = x.child("setters").map_or(0, |s| s.elements().count());
            assert_eq!(setters, j.setters.len(), "{name} {}", j.id);
            let supports = x.child("supports").map_or(0, |s| {
                s.text()
                    .split(',')
                    .filter(|t| !t.trim().is_empty())
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
            });
            assert_eq!(supports, j.supports.len(), "{name} {}", j.id);
            assert_eq!(
                x.child("multiclass").is_some(),
                j.multiclass.is_some(),
                "{name} {}",
                j.id
            );
        }
    }
}

#[test]
fn every_expression_and_template_in_the_fixtures_parses_and_round_trips() {
    use borealis_content::expr::{EquippedExpr, RequirementsExpr, SupportsExpr, Template};
    use borealis_content::{ContentFile, Rule};

    let Some(out) = run(PackOrigin::Core) else {
        return;
    };
    let (mut supports, mut requirements, mut equipped, mut templates) = (0, 0, 0, 0);
    for f in out.files.iter().filter(|f| f.path != "pack.json") {
        let file = ContentFile::from_json_str(&f.contents).unwrap();
        for e in &file.elements {
            if let Some(r) =
                RequirementsExpr::parse(&e.requirements).unwrap_or_else(|x| panic!("{}: {x}", e.id))
            {
                assert_eq!(RequirementsExpr::parse(&r.to_string()).unwrap().unwrap(), r);
                requirements += 1;
            }
            for sheet in e.sheet.iter().flat_map(|s| &s.descriptions) {
                let t = Template::parse(&sheet.text);
                assert_eq!(t.to_string(), sheet.text);
                templates += usize::from(!t.is_literal());
            }
            for rule in e.all_rules() {
                if let Some(r) = rule
                    .parse_requirements()
                    .unwrap_or_else(|x| panic!("{}: {x}", e.id))
                {
                    assert_eq!(RequirementsExpr::parse(&r.to_string()).unwrap().unwrap(), r);
                    requirements += 1;
                }
                match rule {
                    Rule::Select(s) => {
                        if let Some(x) = s
                            .parse_supports()
                            .unwrap_or_else(|x| panic!("{}: {x}", e.id))
                        {
                            assert_eq!(
                                SupportsExpr::parse(&x.to_string()).unwrap().unwrap(),
                                x,
                                "{}",
                                e.id
                            );
                            supports += 1;
                        }
                    }
                    Rule::Stat(s) => {
                        if let Some(x) = s
                            .parse_equipped()
                            .unwrap_or_else(|x| panic!("{}: {x}", e.id))
                        {
                            assert_eq!(EquippedExpr::parse(&x.to_string()).unwrap().unwrap(), x);
                            equipped += 1;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(sc) = &e.spellcasting {
                for l in sc.list.iter().chain(&sc.extends) {
                    let _ = l.expr().unwrap_or_else(|x| panic!("{}: {x}", e.id));
                }
            }
        }
    }
    // 246 select supports strings, 29 equipped expressions in the shipped data
    assert!(supports >= 240, "{supports}");
    assert_eq!(equipped, 29);
    assert!(requirements > 150, "{requirements}");
    assert!(templates > 50, "{templates}");
}
