use super::*;
use serde_json::json;

fn content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content")
}

fn drive() -> (tempfile::TempDir, Drive) {
    let dir = tempfile::tempdir().expect("temp dir");
    let d = Drive::open(&dir.path().join("lib"), content_root()).expect("drive");
    (dir, d)
}

#[test]
fn scoped_paths_stay_inside_the_library() {
    let (dir, d) = drive();
    let inside = d.scoped_path("out/sheet.pdf").expect("relative inside");
    assert!(inside.starts_with(d.root()));
    let abs = d.root().join("x.pdf");
    assert!(d.scoped_path(abs.to_str().unwrap()).is_ok());
    for bad in ["../escape.pdf", "out/../../escape.pdf", "/etc/passwd", ""] {
        let e = d.scoped_path(bad).expect_err(bad);
        assert!(
            e.code == "outside_library" || e.code == "invalid_argument",
            "{bad}: {e}"
        );
    }
    let sibling = dir.path().join("other.pdf");
    assert_eq!(
        d.scoped_path(sibling.to_str().unwrap()).unwrap_err().code,
        "outside_library"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_library_is_refused() {
    let (dir, d) = drive();
    let outside = dir.path().join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, d.root().join("link")).unwrap();
    assert_eq!(
        d.scoped_path("link/sheet.pdf").unwrap_err().code,
        "outside_library"
    );
}

#[test]
fn every_tool_is_routed_and_unknown_names_are_refused() {
    let (_dir, d) = drive();
    let names = specs::names();
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "tool names are unique");
    for name in &names {
        if let Err(e) = d.call(name, &json!({})) {
            assert_ne!(e.code, "unknown_tool", "{name} is routed");
        }
    }
    assert_eq!(d.call("nope", &json!({})).unwrap_err().code, "unknown_tool");
    assert_eq!(
        d.call("list_characters", &json!([])).unwrap_err().code,
        "invalid_argument"
    );
}

#[test]
fn specs_are_object_schemas_without_extra_fields() {
    for spec in specs::all() {
        let name = spec["name"].as_str().unwrap();
        assert_eq!(spec["inputSchema"]["type"], "object", "{name}");
        assert_eq!(spec["inputSchema"]["additionalProperties"], false, "{name}");
        assert_eq!(spec["outputSchema"]["type"], "object", "{name}");
        let props = spec["inputSchema"]["properties"].as_object().unwrap();
        for req in spec["inputSchema"]["required"].as_array().unwrap() {
            assert!(props.contains_key(req.as_str().unwrap()), "{name}: {req}");
        }
    }
}

#[test]
fn unknown_arguments_are_rejected() {
    let (_dir, d) = drive();
    let e = d
        .call("list_slots", &json!({"id": "x", "bogus": 1}))
        .unwrap_err();
    assert_eq!(e.code, "invalid_argument");
    assert!(e.message.contains("bogus"), "{e}");
}

#[test]
fn a_refused_edit_leaves_the_saved_character_unchanged() {
    let (_dir, d) = drive();
    let made = d
        .call("create_character", &json!({"name": "T", "ruleset": "2024"}))
        .unwrap();
    let id = made["id"].as_str().unwrap();
    let e = d
        .call(
            "set_abilities",
            &json!({"id": id, "scores": {"str": 13, "dex": 99}}),
        )
        .unwrap_err();
    assert!(!e.message.is_empty());
    let abilities = d
        .call("get_character", &json!({"id": id, "section": "abilities"}))
        .unwrap();
    assert_ne!(
        abilities["base"]["str"], 13,
        "the partial edit was discarded"
    );
}

#[test]
fn deleting_moves_to_trash_unless_permanent() {
    let (_dir, d) = drive();
    let a = d
        .call("create_character", &json!({"name": "A", "ruleset": "2014"}))
        .unwrap();
    let id = a["id"].as_str().unwrap();
    let out = d.call("delete_character", &json!({"id": id})).unwrap();
    let trashed = d.root().join(out["trashed"].as_str().unwrap());
    assert!(trashed.is_file(), "kept in the trash");
    assert_eq!(d.call("list_characters", &json!({})).unwrap()["count"], 0);
    let b = d
        .call("create_character", &json!({"name": "B", "ruleset": "2014"}))
        .unwrap();
    let id = b["id"].as_str().unwrap();
    let out = d
        .call("delete_character", &json!({"id": id, "permanent": true}))
        .unwrap();
    assert_eq!(out["deleted"], true);
    assert_eq!(
        d.call("open_character", &json!({"id": id}))
            .unwrap_err()
            .code,
        "not_found"
    );
}

#[test]
fn export_needs_a_matching_extension_inside_the_library() {
    let (_dir, d) = drive();
    let a = d
        .call("create_character", &json!({"name": "A", "ruleset": "2024"}))
        .unwrap();
    let id = a["id"].as_str().unwrap();
    let e = d
        .call("export_sheet", &json!({"id": id, "path": "settings.json"}))
        .unwrap_err();
    assert_eq!(e.code, "invalid_argument");
    let e = d
        .call("export_sheet", &json!({"id": id, "path": "../a.pdf"}))
        .unwrap_err();
    assert_eq!(e.code, "outside_library");
}

#[test]
fn generic_magic_armor_needs_a_base_from_list_base_items() {
    const ARMOR_1: &str = "ID_WOTC_DMG_MAGIC_ITEM_ARMOR_1";
    const CHAIN_MAIL: &str = "ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL";
    const LONGSWORD: &str = "ID_WOTC_PHB_WEAPON_LONGSWORD";
    let (_dir, d) = drive();
    let made = d
        .call(
            "create_character",
            &json!({"name": "Armorer", "ruleset": "2014"}),
        )
        .expect("create");
    let id = made["id"].as_str().expect("id").to_string();

    let listed = d
        .call("list_base_items", &json!({"id": id, "item_id": ARMOR_1}))
        .expect("list");
    assert_eq!(listed["needsBase"], true, "{listed}");
    let ids: Vec<&str> = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["id"].as_str())
        .collect();
    assert!(
        ids.contains(&CHAIN_MAIL) && !ids.contains(&LONGSWORD),
        "{ids:?}"
    );
    let plain = d
        .call("list_base_items", &json!({"id": id, "item_id": CHAIN_MAIL}))
        .expect("plain");
    assert_eq!(plain["needsBase"], false);

    let missing = d
        .call("add_item", &json!({"id": id, "item_id": ARMOR_1}))
        .unwrap_err();
    assert_eq!(missing.code, "requirement_failed");
    let wrong = d
        .call(
            "add_item",
            &json!({"id": id, "item_id": ARMOR_1, "base_item_id": LONGSWORD}),
        )
        .unwrap_err();
    assert_eq!(wrong.code, "requirement_failed");
    let added = d
        .call(
            "add_item",
            &json!({"id": id, "item_id": ARMOR_1, "base_item_id": CHAIN_MAIL}),
        )
        .expect("add on a base");
    assert!(
        added["inventory"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["name"] == "Chain Mail +1"),
        "{added}"
    );
}
