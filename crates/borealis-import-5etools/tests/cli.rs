//! The command line tool, run as a real process.

mod support;

use std::process::Command;
use support::fixtures;

fn tool() -> Command {
    Command::new(env!("CARGO_BIN_EXE_borealis-import-5etools"))
}

#[test]
fn imports_the_fixtures_and_writes_packs_and_report() {
    let out = tempfile::tempdir().unwrap();
    let report = out.path().join("report.json");
    let run = tool()
        .arg(fixtures())
        .args(["-o"])
        .arg(out.path().join("packs"))
        .args(["--report"])
        .arg(&report)
        .output()
        .unwrap();
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8(run.stdout).unwrap();
    assert!(stdout.contains("5etools-phb\t"));
    assert!(stdout.contains("4 packs"));
    for id in ["5etools-dmg", "5etools-phb", "5etools-xdmg", "5etools-xphb"] {
        let dir = out.path().join("packs").join(id);
        assert!(dir.join("pack.json").is_file(), "{id}");
        assert!(dir.join("import.json").is_file(), "{id}");
    }
    let parsed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(report).unwrap()).unwrap();
    assert_eq!(parsed["packs"].as_array().unwrap().len(), 4);
}

#[test]
fn source_filter_and_scan() {
    let out = tempfile::tempdir().unwrap();
    let run = tool()
        .arg(fixtures())
        .arg("-o")
        .arg(out.path())
        .args(["--source", "xphb", "--ruleset", "2024"])
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(0));
    assert!(out.path().join("5etools-xphb").is_dir());
    assert!(!out.path().join("5etools-phb").exists());

    let scan = tool().arg(fixtures()).arg("--scan").output().unwrap();
    assert_eq!(scan.status.code(), Some(0));
    let text = String::from_utf8(scan.stdout).unwrap();
    assert!(text.lines().any(|l| l.starts_with("PHB\tdnd2014\t")));
}

#[test]
fn exit_codes() {
    assert_eq!(tool().output().unwrap().status.code(), Some(2));
    assert_eq!(
        tool().arg("--nope").output().unwrap().status.code(),
        Some(2)
    );
    let help = tool().arg("--help").output().unwrap();
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8(help.stdout).unwrap().contains("Usage:"));
    let out = tempfile::tempdir().unwrap();
    let bad = tool()
        .arg(out.path().join("missing"))
        .arg("-o")
        .arg(out.path())
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(1));
    assert!(String::from_utf8(bad.stderr).unwrap().contains("error:"));
}
