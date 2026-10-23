//! B-24: the phone projects offer Borealis for the files it opens. The iOS Info.plist is rewritten
//! from the iOS config's file associations on every `tauri ios build` (and committed); the Android
//! manifest's own filters are hand-written beside the generated block. These checks keep both from
//! drifting.

const INFO_PLIST: &str = include_str!("../gen/apple/borealis_iOS/Info.plist");
const ANDROID_MANIFEST: &str = include_str!("../gen/android/app/src/main/AndroidManifest.xml");
const CONFIG: &str = include_str!("../tauri.conf.json");
const IOS_CONFIG: &str = include_str!("../tauri.ios.conf.json");

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
    for uti in ["app.borealis.dnd5e", "app.borealis.index"] {
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
