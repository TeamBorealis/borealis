//! Every field of the Rust model serializes to JSON that the shipped schema accepts.

use borealis_content::element::{
    Equipment, Extract, ExtractItem, MapKind, MapsTo, Multiclass, SourceRef,
};
use borealis_content::file::{IndexFormat, PackFile, PackFileKind, PackFormat};
use borealis_content::rules::{CustomRule, GrantRule, ListItem, SelectRule, StatRule, UnknownRule};
use borealis_content::schema::{validate_content, validate_document, DocumentKind};
use borealis_content::sheet::{Compendium, Sheet, SheetEntry};
use borealis_content::spellcasting::{Spellcasting, SpellcastingList};
use borealis_content::{
    Append, Author, ContentFile, Element, Extra, IndexEntry, IndexFile, Info, PackManifest,
    PackOrigin, Rule, Ruleset, Setter, UnknownNode, Update, UpdateFile,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn rich_element() -> Element {
    let mut extra = Extra::default();
    extra.attributes.insert("weird".into(), "1".into());
    extra.children.push(UnknownNode {
        tag: "future".into(),
        attributes: BTreeMap::new(),
        text: Some("t".into()),
    });
    let mut setter = Setter::new("cost", "5");
    setter.attributes.insert("currency".into(), "gp".into());
    let mut e = Element::new("ID_RICH", "Rich", "Class", "Source");
    e.ruleset = Some(Ruleset::Dnd2024);
    e.supports = vec!["A".into(), "B".into()];
    e.prerequisite = "Str 13".into();
    e.requirements = "[str:13]".into();
    e.description = "<p>x</p>".into();
    e.sheet = Some(Sheet {
        display: Some(false),
        alt: Some("R".into()),
        usage: Some("1/Day".into()),
        action: Some("Action".into()),
        descriptions: vec![SheetEntry {
            text: "t".into(),
            level: Some(2),
            usage: Some("u".into()),
            action: Some(String::new()),
            extra: extra.clone(),
        }],
        extra: extra.clone(),
    });
    e.compendium = Some(Compendium {
        display: Some(false),
        extra: extra.clone(),
    });
    e.setters = vec![setter.clone()];
    e.rules = vec![
        Rule::Grant(GrantRule {
            id: "ID_G".into(),
            ty: Some("T".into()),
            level: Some(1),
            requirements: Some("ID_AA".into()),
            spellcasting: Some("Wizard".into()),
            prepared: Some(true),
            setters: vec![setter.clone()],
            extra: extra.clone(),
        }),
        Rule::Select(SelectRule {
            ty: "List".into(),
            name: "n".into(),
            number: Some(2),
            level: Some(3),
            requirements: Some("ID_AA".into()),
            supports: Some("A".into()),
            optional: Some(true),
            default_id: Some("ID_D".into()),
            spellcasting: Some("W".into()),
            prepared: Some(false),
            allow_replace: Some(true),
            default_behaviour: Some("x".into()),
            existing: Some(true),
            stat: Some("s".into()),
            items: vec![ListItem {
                id: 1,
                text: "t".into(),
                extra: extra.clone(),
            }],
            setters: vec![setter.clone()],
            extra: extra.clone(),
        }),
        Rule::Stat(StatRule {
            name: "ac".into(),
            value: "1".into(),
            bonus: Some("b".into()),
            level: Some(2),
            requirements: Some("ID_AA".into()),
            equipped: Some("[armor:any]".into()),
            not_equipped: Some("x".into()),
            inline: Some(false),
            maximum: Some("5".into()),
            minimum: Some("0".into()),
            alt: Some("a".into()),
            merge: Some(true),
            setters: vec![setter.clone()],
            extra: extra.clone(),
        }),
        Rule::Unknown(UnknownRule {
            tag: "inline".into(),
            attributes: BTreeMap::new(),
            text: Some("t".into()),
        }),
        Rule::Custom(CustomRule {
            kind: "grant.resource".into(),
            fields: json!({"id":"focus","max":"proficiency"})
                .as_object()
                .unwrap()
                .clone(),
        }),
    ];
    e.spellcasting = Some(Spellcasting {
        name: Some("W".into()),
        ability: Some("Int".into()),
        prepare: Some(true),
        allow_replace: Some(true),
        extend: Some(false),
        all: Some(false),
        list: Some(SpellcastingList {
            supports: "W".into(),
            known: Some(true),
            prepared: None,
            extra: extra.clone(),
        }),
        extends: vec![SpellcastingList {
            prepared: Some(true),
            ..SpellcastingList::new("ID_S")
        }],
        extra: extra.clone(),
    });
    e.equipment = Some(Equipment {
        name: Some("n".into()),
        description: Some("d".into()),
        extra: extra.clone(),
    });
    e.source_ref = Some(SourceRef {
        name: Some("n".into()),
        id: Some("i".into()),
        url: Some("u".into()),
        page: Some("1".into()),
        extra: extra.clone(),
    });
    e.multiclass = Some(Multiclass {
        id: "ID_MC".into(),
        prerequisite: "p".into(),
        requirements: "[str:13]".into(),
        description: "<p/>".into(),
        setters: vec![setter],
        rules: vec![],
        extra: extra.clone(),
    });
    e.extract = Some(Extract {
        items: vec![ExtractItem {
            id: "ID_I".into(),
            amount: Some(5),
            extra: extra.clone(),
        }],
        description: Some("d".into()),
        extra: extra.clone(),
    });
    e.maps_to = Some(MapsTo {
        dnd2014: Some("a".into()),
        dnd2024: Some("b".into()),
        kind: Some(MapKind::None),
    });
    e.renamed_from = Some("ID_OLD".into());
    e.legacy_alias = Some("Race".into());
    e.profile_overrides.insert(
        "dnd2024".into(),
        BTreeMap::from([("damage".to_string(), json!("1d8"))]),
    );
    e.data.insert("category".into(), json!("Origin"));
    e.extra = extra;
    e
}

#[test]
fn fully_populated_file_validates_and_round_trips() {
    let mut file = ContentFile::new(vec![rich_element()]);
    file.ruleset = Some(Ruleset::Dnd2014);
    file.min_app_version = Some("1.0".into());
    file.ignore = Some(false);
    file.info = Some(Info {
        name: Some("n".into()),
        description: Some("d".into()),
        author: Some(Author {
            name: "a".into(),
            url: Some("u".into()),
            extra: Extra::default(),
        }),
        update: Some(Update {
            version: "1.2.3".into(),
            revised: Some("2020".into()),
            file: Some(UpdateFile {
                name: "f.xml".into(),
                url: "https://e.invalid/f.xml".into(),
                extra: Extra::default(),
            }),
            extra: Extra::default(),
        }),
        extra: Extra::default(),
    });
    file.appends.push(Append {
        id: "ID_RICH".into(),
        ty: Some("Spell".into()),
        supports: vec!["X".into()],
        setters: vec![Setter::new("a", "b")],
        rules: vec![],
        spellcasting: None,
        extra: Extra::default(),
    });
    file.extra.attributes.insert("x".into(), "y".into());

    let text = file.to_json_pretty().unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    let violations = validate_content(&value).unwrap();
    assert!(violations.is_empty(), "{violations:#?}");
    let back = ContentFile::from_json_str(&text).unwrap();
    assert_eq!(back, file);
    assert_eq!(back.to_json_pretty().unwrap(), text);
}

#[test]
fn index_and_manifest_documents_validate() {
    let index = IndexFile {
        format: IndexFormat,
        min_app_version: Some("1".into()),
        info: Info::default(),
        files: vec![IndexEntry {
            name: "a.xml".into(),
            url: "u".into(),
            obsolete: Some(true),
            extra: Extra::default(),
        }],
        extra: Extra::default(),
    };
    let v: Value = serde_json::from_str(&index.to_json_pretty().unwrap()).unwrap();
    assert!(validate_document(DocumentKind::Index, &v)
        .unwrap()
        .is_empty());

    let manifest = PackManifest {
        format: PackFormat,
        id: "p".into(),
        name: "P".into(),
        version: "1".into(),
        origin: PackOrigin::Core,
        rulesets: vec![Ruleset::Dnd2014, Ruleset::Dnd2024],
        license: Some("CC-BY-4.0".into()),
        attribution: Some("a".into()),
        description: Some("d".into()),
        files: vec![PackFile {
            path: "a.json".into(),
            kind: PackFileKind::Index,
        }],
    };
    let v: Value = serde_json::from_str(&manifest.to_json_pretty().unwrap()).unwrap();
    assert!(validate_document(DocumentKind::Pack, &v)
        .unwrap()
        .is_empty());
}
