//! Command ACL parity: every command registered in `generate_handler!` is in
//! the app manifest (`build.rs` `COMMANDS`) and granted in `capabilities/default.json`, and
//! nothing extra is. A new command that skips any of the three fails here.

mod contract_support;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn build_rs_commands() -> BTreeSet<String> {
    let src = std::fs::read_to_string(manifest_dir().join("build.rs")).expect("build.rs");
    let start = src
        .find("const COMMANDS: &[&str] = &[")
        .expect("COMMANDS const");
    let body = &src[start..];
    let body = &body[body.find('[').unwrap() + 1..];
    let body = &body[body.find('[').unwrap() + 1..body.find("];").expect("end of COMMANDS")];
    body.split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn capability_permissions() -> Vec<String> {
    let text = std::fs::read_to_string(manifest_dir().join("capabilities/default.json"))
        .expect("capabilities/default.json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("capability json");
    v["permissions"]
        .as_array()
        .expect("permissions array")
        .iter()
        .map(|p| p.as_str().expect("string permission").to_string())
        .collect()
}

#[test]
fn handler_and_app_manifest_list_the_same_commands() {
    let handler: BTreeSet<String> = contract_support::rust_commands().into_iter().collect();
    let manifest = build_rs_commands();
    let missing: Vec<_> = handler.difference(&manifest).collect();
    let extra: Vec<_> = manifest.difference(&handler).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "registered but not in build.rs COMMANDS: {missing:?}\nin build.rs COMMANDS but not registered: {extra:?}"
    );
}

#[test]
fn the_capability_grants_exactly_the_app_commands() {
    let perms = capability_permissions();
    let granted: BTreeSet<String> = perms
        .iter()
        .filter_map(|p| p.strip_prefix("allow-"))
        .map(|p| p.replace('-', "_"))
        .collect();
    let manifest = build_rs_commands();
    let missing: Vec<_> = manifest.difference(&granted).collect();
    let extra: Vec<_> = granted.difference(&manifest).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "not granted: {missing:?}\ngranted but unknown: {extra:?}"
    );
}

/// The only core window permissions the custom title bar needs (min / max / close / drag).
const WINDOW_CHROME: [&str; 4] = [
    "core:window:allow-minimize",
    "core:window:allow-toggle-maximize",
    "core:window:allow-close",
    "core:window:allow-start-dragging",
];

#[test]
fn the_webview_gets_no_dialog_fs_or_shell_plugin_access() {
    for p in capability_permissions() {
        assert!(
            p == "core:default" || p.starts_with("allow-") || WINDOW_CHROME.contains(&p.as_str()),
            "unexpected capability permission {p}: native dialogs are opened by the backend"
        );
    }
}

/// Phones open the download page through the opener plugin's Rust API. The plugin must stay a
/// phone-only dependency (desktop spawns the OS opener itself), must not intercept web view links,
/// and the web view must get none of its permissions (checked above: only `allow-*` app commands).
#[test]
fn the_opener_plugin_is_phone_only_and_backend_only() {
    let cargo = std::fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("Cargo.toml");
    let dep_at = cargo
        .find("tauri-plugin-opener")
        .expect("opener dependency");
    let table = cargo[..dep_at].rfind('[').expect("dependency table");
    assert!(
        cargo[table..dep_at].starts_with(
            r#"[target.'cfg(any(target_os = "android", target_os = "ios"))'.dependencies]"#
        ),
        "tauri-plugin-opener must be a phone-only dependency"
    );
    assert_eq!(cargo.matches("tauri-plugin-opener").count(), 1);
    let lib = std::fs::read_to_string(manifest_dir().join("src/lib.rs")).expect("lib.rs");
    assert!(lib.contains(".open_js_links_on_click(false)"));
    assert!(!lib.contains("tauri_plugin_opener::init()"));
    assert!(capability_permissions()
        .iter()
        .all(|p| !p.contains("opener")));
}
