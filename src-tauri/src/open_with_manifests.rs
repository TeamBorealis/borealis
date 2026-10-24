//! B-24: the phone projects offer Borealis for the files it opens. The iOS Info.plist is rewritten
//! from the iOS config's file associations on every `tauri ios build` (and committed); the Android
//! manifest's own filters are hand-written beside the generated block. These checks keep both from
//! drifting.

const INFO_PLIST: &str = include_str!("../gen/apple/borealis_iOS/Info.plist");
const ANDROID_MANIFEST: &str = include_str!("../gen/android/app/src/main/AndroidManifest.xml");
const CONFIG: &str = include_str!("../tauri.conf.json");
const IOS_CONFIG: &str = include_str!("../tauri.ios.conf.json");
const WINDOWS_CONFIG: &str = include_str!("../tauri.windows.conf.json");
const LINUX_MIME: &str = include_str!("../linux/borealis-mime.xml");
const PROJECT_YML: &str = include_str!("../gen/apple/project.yml");

fn associations(config: &str) -> Vec<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(config).expect("config parses");
    v["bundle"]["fileAssociations"]
        .as_array()
        .expect("file associations")
        .clone()
}

/// The `<dict>` bodies of the `CFBundleDocumentTypes` array.
fn document_types() -> Vec<&'static str> {
    let start = INFO_PLIST
        .find("<key>CFBundleDocumentTypes</key>")
        .expect("Info.plist declares document types");
    let rest = &INFO_PLIST[start..];
    let array = &rest[..rest
        .find("</array>\n\t<key>")
        .expect("end of the document types")];
    array
        .split("<dict>")
        .skip(1)
        .map(|d| d.split("</dict>").next().unwrap_or_default())
        .collect()
}

fn document_type_for(ext: &str) -> &'static str {
    let tag = format!("<string>{ext}</string>");
    document_types()
        .into_iter()
        .find(|d| {
            d.split("<key>LSItemContentTypes</key>")
                .next()
                .unwrap_or_default()
                .contains(&tag)
        })
        .unwrap_or_else(|| panic!("no iOS document type for .{ext}"))
}

#[test]
fn ios_config_names_a_content_type_for_every_association() {
    // The iOS config replaces the whole list (JSON merge), so it must carry every association, unchanged
    // but for `contentTypes` (kept out of tauri.conf.json: on macOS LSItemContentTypes would hide the
    // extensions, and macOS declares no Borealis UTI).
    let base = associations(CONFIG);
    let ios = associations(IOS_CONFIG);
    assert_eq!(base.len(), ios.len());
    for (b, i) in base.iter().zip(&ios) {
        let mut stripped = i.clone();
        let types = stripped
            .as_object_mut()
            .and_then(|o| o.remove("contentTypes"))
            .and_then(|t| t.as_array().cloned())
            .unwrap_or_default();
        assert!(!types.is_empty(), "{} has content types", b["ext"]);
        assert_eq!(&stripped, b, "the iOS association matches tauri.conf.json");
    }
    assert!(base.iter().all(|a| a.get("contentTypes").is_none()));
}

#[test]
fn ios_document_types_name_their_content_types() {
    // iOS matches document types by UTI only (CFBundleTypeExtensions is ignored), so a type without
    // LSItemContentTypes never offers Borealis in Files > Share (seen for .dnd5e on the simulator).
    for (ext, uti) in [
        ("dnd5e", "app.borealis.dnd5e"),
        ("borealis", "app.borealis.bundle"),
        ("index", "app.borealis.index"),
        ("xml", "public.xml"),
        ("zip", "public.zip-archive"),
    ] {
        let dict = document_type_for(ext);
        let types = dict
            .split("<key>LSItemContentTypes</key>")
            .nth(1)
            .unwrap_or_else(|| panic!(".{ext} has no LSItemContentTypes"));
        assert!(
            types.contains(&format!("<string>{uti}</string>")),
            ".{ext} should name {uti}"
        );
    }
}

#[test]
fn ios_declares_the_borealis_utis() {
    let imported = INFO_PLIST
        .split("<key>UTImportedTypeDeclarations</key>")
        .nth(1)
        .expect("imported type declarations");
    for uti in [
        "app.borealis.dnd5e",
        "app.borealis.index",
        "app.borealis.bundle",
    ] {
        assert!(
            imported.contains(&format!("<string>{uti}</string>")),
            "{uti} is declared"
        );
    }
}

#[test]
fn android_offers_borealis_for_downloads_provider_files() {
    // The Downloads provider names a file by an opaque id, so only a MIME-type filter can match a
    // .dnd5e or .index opened from there (it arrives as application/octet-stream).
    let filter = ANDROID_MANIFEST
        .split("<intent-filter>")
        .skip(1)
        .map(|f| f.split("</intent-filter>").next().unwrap_or_default())
        .find(|f| {
            f.contains("android.intent.action.VIEW")
                && f.contains(r#"android:mimeType="application/octet-stream""#)
                && !f.contains("android:pathPattern")
        })
        .expect("a VIEW filter for application/octet-stream without a path pattern");
    assert!(filter.contains(r#"android:scheme="content""#));
    assert!(filter.contains("android.intent.category.DEFAULT"));
}

fn association<'a>(list: &'a [serde_json::Value], ext: &str) -> &'a serde_json::Value {
    list.iter()
        .find(|a| a["ext"].as_array().is_some_and(|e| e.iter().any(|x| x == ext)))
        .unwrap_or_else(|| panic!("no association for .{ext}"))
}

#[test]
fn every_desktop_config_registers_the_bundle_file_type() {
    // macOS and Linux read the base config (Linux also the shared-mime-info file below); Windows has
    // its own list, which replaces the base one.
    for (label, config) in [("base", CONFIG), ("windows", WINDOWS_CONFIG)] {
        let list = associations(config);
        let a = association(&list, "borealis");
        assert_eq!(a["mimeType"], "application/x-borealis-bundle", "{label}");
        assert_eq!(a["role"], "Editor", "{label}");
        assert!(a.get("contentTypes").is_none(), "{label}");
    }
    let names: Vec<_> = associations(WINDOWS_CONFIG)
        .iter()
        .map(|a| a["name"].as_str().unwrap_or_default().to_string())
        .collect();
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "Windows ProgIDs are distinct");
    assert!(LINUX_MIME.contains(r#"<mime-type type="application/x-borealis-bundle">"#));
    assert!(LINUX_MIME.contains(r#"<glob pattern="*.borealis"/>"#));
}

#[test]
fn the_ios_project_spec_declares_the_bundle_type_too() {
    // The project spec regenerates Info.plist, so both must carry it.
    assert!(PROJECT_YML.contains("app.borealis.bundle"));
    assert!(PROJECT_YML.contains("public.filename-extension: [borealis]"));
}

#[test]
fn android_matches_borealis_bundles_by_name_and_by_mime_type() {
    let filters: Vec<&str> = ANDROID_MANIFEST
        .split("<intent-filter>")
        .skip(1)
        .map(|f| f.split("</intent-filter>").next().unwrap_or_default())
        .collect();
    let has_pattern = |f: &&str| f.contains(r#"android:pathPattern=".*\\.borealis""#);
    // The hand-written VIEW filter (content and file URIs with a readable name).
    assert!(
        filters
            .iter()
            .any(|f| has_pattern(f) && f.contains(r#"android:scheme="content""#)),
        "a VIEW filter with the .borealis path pattern"
    );
    // The generated block: VIEW and SEND with the bundle MIME type.
    let generated = filters
        .iter()
        .find(|f| f.contains(r#"android:mimeType="application/x-borealis-bundle""#))
        .expect("a filter for the bundle MIME type");
    assert!(has_pattern(generated));
    assert!(generated.contains("android.intent.action.SEND"));
    assert!(generated.contains("android.intent.action.VIEW"));
    // Downloads URIs carry no extension: the octet-stream filter (tested above) is what opens them.
}
