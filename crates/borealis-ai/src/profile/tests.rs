use super::*;
use tempfile::TempDir;

fn mcp() -> McpCommand {
    McpCommand {
        program: PathBuf::from("/app/borealis"),
        args: vec!["--mcp".into()],
    }
}

fn installed() -> TempDir {
    let t = TempDir::new().unwrap();
    install_profile(&default_assets_dir(), t.path()).unwrap();
    t
}

fn strings(v: Vec<OsString>) -> Vec<String> {
    v.into_iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn claude_args_golden() {
    let args = strings(profile_args(Backend::Claude, Path::new("/p"), &mcp()));
    // Joined with the platform separator (`/p\profile\claude\...` on Windows).
    let claude = Path::new("/p").join("profile").join("claude");
    let under = |name: &str| claude.join(name).to_string_lossy().into_owned();
    assert_eq!(
        args,
        [
            "--mcp-config",
            r#"{"mcpServers":{"borealis":{"args":["--mcp"],"command":"/app/borealis"}}}"#,
            "--settings",
            &under("settings.json"),
            "--plugin-dir",
            &under("plugin"),
            "--permission-mode",
            "dontAsk",
        ]
    );
}

#[test]
fn codex_args_golden() {
    let t = installed();
    let args = strings(profile_args(Backend::Codex, t.path(), &mcp()));
    assert_eq!(
        args,
        [
            "-c",
            r#"mcp_servers.borealis.command="/app/borealis""#,
            "-c",
            r#"mcp_servers.borealis.args=["--mcp"]"#,
            "-c",
            "mcp_servers.borealis.startup_timeout_sec=20",
            "-c",
            "mcp_servers.borealis.tool_timeout_sec=60",
        ]
    );
}

#[test]
fn codex_args_without_installed_overrides_still_has_mcp() {
    let args = profile_args(Backend::Codex, Path::new("/nonexistent"), &mcp());
    assert_eq!(args.len(), 4);
}

#[test]
fn install_then_verify_ok_and_reinstall_is_idempotent() {
    let t = installed();
    assert_eq!(verify_profile(t.path()), ProfileStatus::Ok);
    install_profile(&default_assets_dir(), t.path()).unwrap();
    assert_eq!(verify_profile(t.path()), ProfileStatus::Ok);
    assert!(t
        .path()
        .join("profile/claude/plugin/skills/borealis-content-authoring/SKILL.md")
        .is_file());
}

#[cfg(unix)]
#[test]
fn installed_files_and_dirs_are_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let t = installed();
    let mode = |p: &str| fs::metadata(t.path().join(p)).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode("profile/claude/settings.json"), 0o444);
    assert_eq!(mode("manifest.json"), 0o444);
    assert_eq!(mode("profile/claude"), 0o555);
    assert_eq!(mode("profile"), 0o555);
    assert!(fs::write(t.path().join("profile/claude/settings.json"), b"x").is_err() || is_root());
}

#[cfg(unix)]
fn is_root() -> bool {
    std::env::var("USER").as_deref() == Ok("root")
}

fn unlock_and_write(t: &TempDir, rel: &str, body: &str) {
    set_tree_writable(&t.path().join("profile"), true).unwrap();
    fs::write(t.path().join(rel), body).unwrap();
}

#[test]
fn modified_file_is_tampered_and_repair_restores() {
    let t = installed();
    unlock_and_write(
        &t,
        "profile/claude/settings.json",
        "{\"permissions\":{\"allow\":[\"Bash\"]}}",
    );
    assert_eq!(
        verify_profile(t.path()),
        ProfileStatus::Tampered(vec!["claude/settings.json".into()])
    );
    let before = repair_profile(&default_assets_dir(), t.path()).unwrap();
    assert!(matches!(before, ProfileStatus::Tampered(_)));
    assert_eq!(verify_profile(t.path()), ProfileStatus::Ok);
    let restored = fs::read_to_string(t.path().join("profile/claude/settings.json")).unwrap();
    assert!(restored.contains("mcp__borealis__*"));
}

#[test]
fn added_and_deleted_files_are_tampered() {
    let t = installed();
    unlock_and_write(&t, "profile/claude/plugin/skills/evil.md", "x");
    fs::remove_file(t.path().join("profile/codex/overrides.toml")).unwrap();
    assert_eq!(
        verify_profile(t.path()),
        ProfileStatus::Tampered(vec![
            "claude/plugin/skills/evil.md".into(),
            "codex/overrides.toml".into()
        ])
    );
    repair_profile(&default_assets_dir(), t.path()).unwrap();
    assert_eq!(verify_profile(t.path()), ProfileStatus::Ok);
}

#[test]
fn missing_or_corrupt_manifest_is_tampered() {
    let t = TempDir::new().unwrap();
    assert_eq!(
        verify_profile(t.path()),
        ProfileStatus::Tampered(vec!["manifest.json".into()])
    );
    let t = installed();
    set_mode(&t.path().join("manifest.json"), false, true).unwrap();
    fs::write(t.path().join("manifest.json"), "not json").unwrap();
    assert!(matches!(
        verify_profile(t.path()),
        ProfileStatus::Tampered(_)
    ));
}

#[test]
fn repair_is_noop_when_intact() {
    let t = installed();
    assert_eq!(
        repair_profile(&default_assets_dir(), t.path()).unwrap(),
        ProfileStatus::Ok
    );
}

#[test]
fn default_settings_deny_shell_and_allow_only_borealis_tools() {
    let v: serde_json::Value = serde_json::from_slice(
        &fs::read(default_assets_dir().join("claude/settings.json")).unwrap(),
    )
    .unwrap();
    let allow = v["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|a| a == "mcp__borealis__*"));
    assert!(!allow.iter().any(|a| a == "Bash"));
    assert!(v["permissions"]["deny"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "Bash"));
}

#[test]
fn shipped_skills_have_frontmatter_and_are_in_the_manifest() {
    const SKILLS: [&str; 5] = [
        "borealis-content-authoring",
        "borealis-class-and-subclass",
        "borealis-editions",
        "borealis-balance",
        "borealis-tools",
    ];
    let t = installed();
    let manifest = fs::read_to_string(t.path().join("manifest.json")).unwrap();
    for name in SKILLS {
        let rel = format!("claude/plugin/skills/{name}/SKILL.md");
        assert!(manifest.contains(&rel), "{rel} missing from manifest");
        let body = fs::read_to_string(t.path().join("profile").join(&rel)).unwrap();
        assert!(body.starts_with("---\n"), "{name}: no frontmatter");
        assert!(
            body.contains(&format!("\nname: {name}\n")),
            "{name}: bad name"
        );
        assert!(body.contains("\ndescription: "), "{name}: no description");
        assert!(body.lines().count() <= 250, "{name}: over 250 lines");
    }
    assert_eq!(verify_profile(t.path()), ProfileStatus::Ok);
}

#[test]
fn rewriting_the_manifest_too_is_still_tampered() {
    let t = installed();
    let evil = "{\"permissions\":{\"allow\":[\"Bash\"]}}";
    unlock_and_write(&t, "profile/claude/settings.json", evil);
    // A self-consistent manifest for the edited tree (what a careful attacker would write).
    let manifest = t.path().join(MANIFEST);
    set_mode(&manifest, false, true).unwrap();
    let body = serde_json::to_vec(&Manifest {
        profile_version: PROFILE_VERSION,
        files: hash_tree(&t.path().join(PROFILE_SUBDIR)).unwrap(),
    })
    .unwrap();
    fs::write(&manifest, body).unwrap();
    assert_eq!(
        verify_profile(t.path()),
        ProfileStatus::Tampered(vec![
            "claude/settings.json (differs from the bundled profile)".into()
        ])
    );
}

#[test]
fn bundled_hashes_match_the_shipped_assets() {
    let on_disk = hash_tree(&default_assets_dir()).unwrap();
    assert_eq!(bundled_hashes(), on_disk);
}

#[test]
fn codex_overrides_accept_only_borealis_mcp_keys() {
    let text = "# comment\n\
        mcp_servers.borealis.tool_timeout_sec = 60\n\
        sandbox_mode = \"danger-full-access\"\n\
        approval_policy=\"never\"\n\
        mcp_servers.other.command = \"/bin/sh\"\n\
        mcp_servers.borealis = {}\n\
        mcp_servers.borealis.x\"y = 1\n\
        garbage\n";
    assert_eq!(
        strings(codex_override_args(text)),
        ["-c", "mcp_servers.borealis.tool_timeout_sec=60"]
    );
}

#[test]
fn prepared_claude_args_keep_paths_out_of_argv() {
    let t = installed();
    let scratch = TempDir::new().unwrap();
    let args =
        strings(prepare_profile_args(Backend::Claude, t.path(), &mcp(), scratch.path()).unwrap());
    assert!(
        !args.iter().any(|a| a.contains("/app/borealis")),
        "{args:?}"
    );
    let pos = args.iter().position(|a| a == "--mcp-config").unwrap();
    let mcp_file = fs::read_to_string(&args[pos + 1]).unwrap();
    assert!(mcp_file.contains("/app/borealis"));
    let pos = args.iter().position(|a| a == "--settings").unwrap();
    assert!(args[pos + 1].starts_with(&*scratch.path().to_string_lossy()));
    let settings = fs::read(&args[pos + 1]).unwrap();
    assert_eq!(settings, bundled_file("claude/settings.json").unwrap());
}

#[test]
fn prepared_codex_args_match_golden_and_refuse_tampering() {
    let t = installed();
    let scratch = TempDir::new().unwrap();
    let prepared = prepare_profile_args(Backend::Codex, t.path(), &mcp(), scratch.path()).unwrap();
    assert_eq!(prepared, profile_args(Backend::Codex, t.path(), &mcp()));
    unlock_and_write(
        &t,
        "profile/codex/overrides.toml",
        "sandbox_mode = \"danger-full-access\"\n",
    );
    assert!(prepare_profile_args(Backend::Codex, t.path(), &mcp(), scratch.path()).is_err());
}
