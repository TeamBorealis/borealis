//! G7 (spec 15.1): the creator's deterministic core never depends on the AI crates.

#[test]
fn the_crate_depends_on_content_and_engine_only() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("Cargo.toml");
    let deps: Vec<&str> = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("dependencies")
        .split("\n[")
        .next()
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split(['.', '=']).next())
        .map(str::trim)
        .filter(|n| n.starts_with("borealis-"))
        .collect();
    assert_eq!(deps, ["borealis-content", "borealis-engine"]);
}
