use super::*;

#[test]
fn plain_relative_paths_are_split() {
    assert_eq!(parts("Note.md").unwrap(), ["Note.md"]);
    assert_eq!(
        parts("Places/Harbor Town.md").unwrap(),
        ["Places", "Harbor Town.md"]
    );
}

#[test]
fn paths_that_leave_the_vault_or_reach_hidden_files_are_refused() {
    for bad in [
        "",
        "../outside.md",
        "Places/../../outside.md",
        "./Note.md",
        "/etc/passwd",
        "C:/Windows/win.ini",
        "C:\\Windows\\win.ini",
        "Places\\..\\Note.md",
        ".obsidian/app.json",
        "Places/.hidden.md",
        ".trash/Old.md",
        "Places//Note.md",
        "Places/",
        "Note.md\0",
        "Thumbs.db",
        "Folder/desktop.ini",
        "a:b.md",
    ] {
        assert!(parts(bad).is_err(), "{bad:?} should be refused");
    }
    assert!(parts(&"a/".repeat(MAX_PATH_LEN)).is_err());
}

#[test]
fn new_names_follow_obsidian_rules() {
    assert!(check_new_name("Session 2").is_ok());
    assert!(check_new_name("Café notes").is_ok());
    for bad in [
        "", " lead", "trail ", "dot.", "a/b", "a\\b", "a:b", "what?", "x|y", "#tag", "[[x]]",
        ".hidden", "a^b",
    ] {
        assert!(check_new_name(bad).is_err(), "{bad:?} should be refused");
    }
}

#[test]
fn name_helpers_split_folders_and_extensions() {
    assert_eq!(parent("A/B/Note.md"), "A/B");
    assert_eq!(parent("Note.md"), "");
    assert_eq!(file_name("A/B/Note.md"), "Note.md");
    assert_eq!(join("", "x.md"), "x.md");
    assert_eq!(join("A", "x.md"), "A/x.md");
    assert_eq!(stem("Session 1.md"), "Session 1");
    assert_eq!(stem("v1.2 notes.md"), "v1.2 notes");
    assert_eq!(stem("README"), "README");
    assert_eq!(extension("Map.PNG").as_deref(), Some("png"));
    assert_eq!(extension("README"), None);
    assert_eq!(extension(".env"), None);
}

#[test]
fn existing_files_resolve_inside_the_vault() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("Places")).unwrap();
    std::fs::write(dir.path().join("Places/Harbor.md"), "x").unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let found = vault.existing_file("Places/Harbor.md").unwrap();
    assert_eq!(found.real, vault.root().join("Places/Harbor.md"));
    assert_eq!(
        vault.relative(&found.real).as_deref(),
        Some("Places/Harbor.md")
    );
    assert_eq!(
        vault.existing("Nope.md").unwrap_err().code,
        crate::error::ErrorCode::NotFound
    );
    assert!(
        vault.existing_file("Places").is_err(),
        "a folder is not a file"
    );
}

#[cfg(unix)]
#[test]
fn symbolic_links_out_of_the_vault_are_refused_and_links_inside_work() {
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.md"), "secret").unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Real.md"), "real").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.md"),
        dir.path().join("Escape.md"),
    )
    .unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("Linked")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("Real.md"), dir.path().join("Alias.md")).unwrap();
    let vault = Vault::open(dir.path()).unwrap();

    assert!(vault.existing_file("Escape.md").is_err());
    assert!(vault.existing_file("Linked/secret.md").is_err());
    assert!(
        vault.new_file("Linked/new.md").is_err(),
        "no new file through a linked folder"
    );
    assert!(!outside.path().join("new.md").exists());

    let alias = vault.existing_file("Alias.md").unwrap();
    assert_eq!(alias.path, vault.root().join("Alias.md"));
    assert_eq!(alias.real, vault.root().join("Real.md"));
}

#[test]
fn new_files_get_their_folders_and_never_replace_a_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Taken.md"), "x").unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let p = vault.new_file("Inbox/Deep/New.md").unwrap();
    assert_eq!(p, vault.root().join("Inbox/Deep/New.md"));
    assert!(vault.root().join("Inbox/Deep").is_dir());
    assert!(vault.new_file("Taken.md").is_err());
    assert!(
        vault.new_file("Taken.md/child.md").is_err(),
        "a file is in the way"
    );
    assert!(vault.new_file(".obsidian/new.json").is_err());
}
