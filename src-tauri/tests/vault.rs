//! GitHub #15 (B-102): the Obsidian vault behind the notes view, against the fixture vault
//! (`tests/fixtures/vault`, original text). Each test works on its own copy, so the fixture is never written.

use borealis_lib::error::ErrorCode;
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::{AppPaths, AppState};
use borealis_lib::vault::dto::{DeleteStatus, SaveStatus};
use borealis_lib::vault::text;
use borealis_lib::vault::tree::EntryKind;
use serde_json::json;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vault")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// A copy of the fixture vault, outside the system temp folder (user folders may not live there).
fn vault_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy_dir(&fixture(), dir.path());
    dir
}

fn app() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: dir.path().to_path_buf(),
    })
    .unwrap();
    (dir, state)
}

/// An app with the vault copy open.
fn opened() -> (tempfile::TempDir, tempfile::TempDir, AppState) {
    let vault = vault_copy();
    let (data, state) = app();
    let token = state
        .picked
        .grant(vault.path().to_path_buf(), PickPurpose::VaultDir)
        .unwrap()
        .token;
    state.set_vault_dir(Some(&token)).unwrap();
    (vault, data, state)
}

fn read(dir: &Path, rel: &str) -> Vec<u8> {
    std::fs::read(dir.join(rel)).unwrap()
}

/// Every file under `dir`, with its bytes.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((
                    p.strip_prefix(dir).unwrap().to_path_buf(),
                    std::fs::read(&p).unwrap(),
                ));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn the_vault_is_a_token_only_setting_kept_by_restore_defaults() {
    let vault = vault_copy();
    let (_d, state) = app();
    assert_eq!(state.settings().unwrap().vault_dir, None);
    let tree = state.vault_tree().unwrap();
    assert!(tree.dir.is_none() && !tree.unavailable && tree.entries.is_empty());

    assert!(state.set_vault_dir(Some("not-a-token")).is_err());
    let wrong = state
        .picked
        .grant(vault.path().to_path_buf(), PickPurpose::PortraitLibraryDir)
        .unwrap()
        .token;
    assert!(
        state.set_vault_dir(Some(&wrong)).is_err(),
        "a token for another purpose"
    );
    let raw = json!({ "vaultDir": vault.path().to_string_lossy() });
    assert_eq!(
        state.save_settings(&raw).unwrap_err().code,
        ErrorCode::InvalidArgument
    );
    let temp = state
        .picked
        .grant(std::env::temp_dir(), PickPurpose::VaultDir)
        .unwrap()
        .token;
    assert!(state.set_vault_dir(Some(&temp)).is_err(), "the temp folder");

    let token = state
        .picked
        .grant(vault.path().to_path_buf(), PickPurpose::VaultDir)
        .unwrap()
        .token;
    let dirs = state.set_vault_dir(Some(&token)).unwrap();
    assert!(dirs.vault_dir.is_some() && !dirs.vault_unavailable);
    state
        .save_settings(&serde_json::to_value(state.settings().unwrap()).unwrap())
        .unwrap();
    assert!(
        state.settings().unwrap().vault_dir.is_some(),
        "a full save keeps it"
    );
    let sink = borealis_lib::content_packs::NullSink;
    state.restore_default_settings(&sink).unwrap();
    assert!(
        state.settings().unwrap().vault_dir.is_some(),
        "Restore Defaults keeps the vault"
    );
    state.set_vault_dir(None).unwrap();
    assert_eq!(state.settings().unwrap().vault_dir, None);
}

#[test]
fn the_tree_lists_visible_files_and_reads_the_obsidian_settings() {
    let (_v, _d, state) = opened();
    let tree = state.vault_tree().unwrap();
    assert!(tree.obsidian && !tree.truncated);
    let paths: Vec<&str> = tree.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "Archive",
            "Archive/Home.md",
            "Handouts",
            "Handouts/Ferry schedule.pdf",
            "Odd",
            "Odd/Empty.md",
            "Odd/Not unicode.md",
            "Odd/Odd front matter.md",
            "People",
            "People/Bramblewick.md",
            "Places",
            "Places/Harbor Town.md",
            "Board.canvas",
            "Home.md",
            "map.png",
            "Session 1.md",
        ]
    );
    let kind = |p: &str| tree.entries.iter().find(|e| e.path == p).unwrap().kind;
    assert_eq!(kind("Archive"), EntryKind::Folder);
    assert_eq!(kind("Home.md"), EntryKind::Note);
    assert_eq!(kind("map.png"), EntryKind::Image);
    assert_eq!(kind("Board.canvas"), EntryKind::Canvas);
    assert_eq!(kind("Handouts/Ferry schedule.pdf"), EntryKind::File);
    assert_eq!(tree.config.new_file_folder_path, "Inbox");
    assert_eq!(tree.config.attachment_folder_path, "./attachments");
}

#[test]
fn every_note_round_trips_byte_for_byte() {
    let (v, _d, state) = opened();
    for rel in [
        "Home.md",
        "Session 1.md",
        "Odd/Odd front matter.md",
        "Odd/Empty.md",
        "Places/Harbor Town.md",
    ] {
        let before = read(v.path(), rel);
        let note = state.vault_read(rel).unwrap();
        assert_eq!(note.hash, text::hash(&before));
        // The editor holds LF text (a text box turns CRLF into LF); saving it unchanged writes the same bytes.
        let in_editor = note.text.replace("\r\n", "\n");
        let saved = state.vault_save(rel, &in_editor, Some(&note.hash)).unwrap();
        assert_eq!(saved.status, SaveStatus::Saved, "{rel}");
        assert_eq!(read(v.path(), rel), before, "{rel} changed");
    }
}

#[test]
fn an_edit_to_a_crlf_note_with_a_bom_changes_only_that_line() {
    let (v, _d, state) = opened();
    let before = read(v.path(), "Session 1.md");
    assert!(before.starts_with(text::BOM));
    let note = state.vault_read("Session 1.md").unwrap();
    assert!(note.bom);
    assert_eq!(note.line_ending, text::LineEnding::Crlf);
    let edited = note
        .text
        .replace("\r\n", "\n")
        .replace("- [ ] Ask", "- [x] Ask");
    state
        .vault_save("Session 1.md", &edited, Some(&note.hash))
        .unwrap();
    let expected = String::from_utf8(before)
        .unwrap()
        .replace("- [ ] Ask", "- [x] Ask");
    assert_eq!(read(v.path(), "Session 1.md"), expected.into_bytes());
}

#[test]
fn a_note_changed_by_another_app_is_never_overwritten() {
    let (v, _d, state) = opened();
    let note = state.vault_read("People/Bramblewick.md").unwrap();
    std::fs::write(
        v.path().join("People/Bramblewick.md"),
        "Obsidian wrote this.\n",
    )
    .unwrap();
    let out = state
        .vault_save("People/Bramblewick.md", "Borealis edit\n", Some(&note.hash))
        .unwrap();
    assert_eq!(out.status, SaveStatus::Conflict);
    let disk = out.disk.unwrap();
    assert_eq!(disk.text, "Obsidian wrote this.\n");
    assert_eq!(
        read(v.path(), "People/Bramblewick.md"),
        b"Obsidian wrote this.\n"
    );
    // Keep mine: save again against what is on disk now.
    let kept = state
        .vault_save("People/Bramblewick.md", "Borealis edit\n", Some(&disk.hash))
        .unwrap();
    assert_eq!(kept.status, SaveStatus::Saved);
    assert_eq!(read(v.path(), "People/Bramblewick.md"), b"Borealis edit\n");

    std::fs::remove_file(v.path().join("People/Bramblewick.md")).unwrap();
    let gone = state
        .vault_save(
            "People/Bramblewick.md",
            "again\n",
            Some(&kept.hash.unwrap()),
        )
        .unwrap();
    assert_eq!(gone.status, SaveStatus::Missing);
    assert!(
        !v.path().join("People/Bramblewick.md").exists(),
        "not recreated silently"
    );
    let recreated = state
        .vault_save("People/Bramblewick.md", "again\n", None)
        .unwrap();
    assert_eq!(recreated.status, SaveStatus::Saved);
    assert!(
        state
            .vault_save("People/Bramblewick.md", "x", None)
            .is_err(),
        "create never replaces"
    );
}

#[test]
fn files_that_are_not_notes_or_not_utf8_open_read_only() {
    let (_v, _d, state) = opened();
    let latin = state.vault_read("Odd/Not unicode.md").unwrap();
    assert!(latin.read_only.is_some());
    assert!(state
        .vault_save("Odd/Not unicode.md", "x", Some(&latin.hash))
        .is_err());
    let canvas = state.vault_read("Board.canvas").unwrap();
    assert!(canvas.read_only.is_some() && canvas.text.contains("Plot board"));
    assert!(state
        .vault_save("Board.canvas", "{}", Some(&canvas.hash))
        .is_err());
    assert!(state.vault_read("Odd").is_err(), "a folder");
}

#[test]
fn nothing_outside_the_vault_or_hidden_is_reachable_and_obsidian_settings_are_never_written() {
    let (v, _d, state) = opened();
    let obsidian_before = snapshot(&v.path().join(".obsidian"));
    for bad in [
        "../outside.md",
        ".obsidian/app.json",
        ".trash/Old idea.md",
        "/etc/hosts",
        ".hidden-scratch.md",
        "Odd/../Home.md",
    ] {
        assert!(state.vault_read(bad).is_err(), "read {bad}");
        assert!(state.vault_save(bad, "x", None).is_err(), "save {bad}");
        assert!(state.vault_delete(bad, true).is_err(), "delete {bad}");
    }
    assert!(state
        .vault_create_note(Some(".obsidian/new"), None, None)
        .is_err());
    assert!(state
        .vault_create_note(Some("../escape"), None, None)
        .is_err());
    // Everything else the notes view does leaves `.obsidian/` alone.
    let n = state.vault_read("Home.md").unwrap();
    state
        .vault_save("Home.md", "changed", Some(&n.hash))
        .unwrap();
    state.vault_create_note(None, None, None).unwrap();
    state
        .vault_rename("People/Bramblewick.md", "Bram", true)
        .unwrap();
    assert_eq!(snapshot(&v.path().join(".obsidian")), obsidian_before);
}

#[cfg(unix)]
#[test]
fn symbolic_links_out_of_the_vault_are_not_listed_read_or_written() {
    let (v, _d, state) = opened();
    let outside = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    std::fs::write(outside.path().join("secret.md"), "secret").unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.md"), v.path().join("Escape.md"))
        .unwrap();
    std::os::unix::fs::symlink(outside.path(), v.path().join("Outside")).unwrap();
    std::os::unix::fs::symlink(v.path().join("Home.md"), v.path().join("Shortcut.md")).unwrap();
    let tree = state.vault_tree().unwrap();
    let paths: Vec<&str> = tree.entries.iter().map(|e| e.path.as_str()).collect();
    assert!(!paths.contains(&"Escape.md") && !paths.iter().any(|p| p.starts_with("Outside")));
    assert!(
        paths.contains(&"Shortcut.md"),
        "a link inside the vault is listed"
    );
    assert!(state.vault_read("Escape.md").is_err());
    assert!(state.vault_read("Outside/secret.md").is_err());
    assert!(state.vault_save("Outside/new.md", "x", None).is_err());
    assert!(state
        .vault_save("Escape.md", "x", Some(&text::hash(b"secret")))
        .is_err());
    assert_eq!(read(outside.path(), "secret.md"), b"secret");
    assert!(!outside.path().join("new.md").exists());
    // Writing through a link inside the vault keeps the link.
    let n = state.vault_read("Shortcut.md").unwrap();
    state
        .vault_save("Shortcut.md", "via the link\n", Some(&n.hash))
        .unwrap();
    assert!(std::fs::symlink_metadata(v.path().join("Shortcut.md"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(read(v.path(), "Home.md"), b"via the link\n");
}

#[cfg(unix)]
#[test]
fn links_into_hidden_folders_or_to_other_kinds_of_file_are_refused() {
    let (v, _d, state) = opened();
    let settings = v.path().join(".obsidian/workspace.json");
    let before = read(v.path(), ".obsidian/workspace.json");
    std::os::unix::fs::symlink(&settings, v.path().join("Workspace.md")).unwrap();
    std::os::unix::fs::symlink(v.path().join("Board.canvas"), v.path().join("Board.md")).unwrap();
    let tree = state.vault_tree().unwrap();
    assert!(
        !tree.entries.iter().any(|e| e.path == "Workspace.md"),
        "a link into .obsidian is not listed"
    );
    assert!(state.vault_read("Workspace.md").is_err());
    assert!(state
        .vault_save("Workspace.md", "{}", Some(&text::hash(&before)))
        .is_err());
    assert_eq!(read(v.path(), ".obsidian/workspace.json"), before);
    // A `.md` link to a canvas opens as the canvas it is, read-only, and is never saved as a note.
    let board = state.vault_read("Board.md").unwrap();
    assert!(board.read_only.is_some(), "the real file is a canvas");
    let canvas = read(v.path(), "Board.canvas");
    assert!(state
        .vault_save("Board.md", "# not a canvas", Some(&board.hash))
        .is_err());
    assert_eq!(read(v.path(), "Board.canvas"), canvas);
}

#[test]
fn only_notes_and_canvases_are_read_as_text() {
    let (_v, _d, state) = opened();
    assert!(state.vault_read("Handouts/Ferry schedule.pdf").is_err());
    assert!(state.vault_read("map.png").is_err());
    assert!(state.vault_read("Board.canvas").is_ok());
}

#[test]
fn an_edit_keeps_the_other_lines_of_a_file_with_mixed_line_endings() {
    let (v, _d, state) = opened();
    std::fs::write(v.path().join("Mixed.md"), b"one\r\ntwo\r\nthree\nfour\r\n").unwrap();
    let note = state.vault_read("Mixed.md").unwrap();
    let edited = note.text.replace("\r\n", "\n").replace("four", "FOUR");
    state
        .vault_save("Mixed.md", &edited, Some(&note.hash))
        .unwrap();
    assert_eq!(read(v.path(), "Mixed.md"), b"one\r\ntwo\r\nthree\nFOUR\r\n");
}

#[test]
fn links_resolve_the_obsidian_way() {
    let (_v, _d, state) = opened();
    let targets: Vec<String> = [
        "Harbor Town",
        "Bramblewick",
        "home",
        "Missing Note",
        "map.png",
        "People/Bramblewick",
        "Session 1",
    ]
    .map(str::to_string)
    .to_vec();
    let got = state.vault_resolve("Home.md", &targets).unwrap();
    assert_eq!(
        got,
        [
            Some("Places/Harbor Town.md".to_string()),
            Some("People/Bramblewick.md".to_string()),
            Some("Home.md".to_string()),
            None,
            Some("map.png".to_string()),
            Some("People/Bramblewick.md".to_string()),
            Some("Session 1.md".to_string()),
        ]
    );
    let from_archive = state
        .vault_resolve("Archive/Home.md", &["Home".to_string()])
        .unwrap();
    assert_eq!(
        from_archive,
        [Some("Archive/Home.md".to_string())],
        "the same folder wins"
    );
}

#[test]
fn backlinks_count_links_outside_code_from_other_notes() {
    let (_v, _d, state) = opened();
    let back = state.vault_backlinks("People/Bramblewick.md").unwrap();
    let from: Vec<&str> = back.iter().map(|b| b.path.as_str()).collect();
    assert_eq!(from, ["Home.md", "Places/Harbor Town.md", "Session 1.md"]);
    let home = &back[0];
    // [[People/Bramblewick|...]], [[Bramblewick#Appearance]], ![[Bramblewick#Appearance]], [[Bramblewick\|a favour]].
    assert_eq!(home.count, 4);
    assert!(home.lines[0].text.contains("tinker gnome"));
    let session = state.vault_backlinks("Session 1.md").unwrap();
    let from: Vec<&str> = session.iter().map(|b| b.path.as_str()).collect();
    assert_eq!(from, ["Home.md", "People/Bramblewick.md"]);
    assert!(state.vault_backlinks("Odd/Empty.md").unwrap().is_empty());
}

#[test]
fn tags_come_from_text_and_front_matter_and_search_finds_names_and_text() {
    let (_v, _d, state) = opened();
    let tags = state.vault_tags().unwrap();
    let names: Vec<(&str, usize)> = tags.iter().map(|t| (t.tag.as_str(), t.count)).collect();
    assert_eq!(
        names,
        [
            ("campaign", 1),
            ("campaign/arc-one", 1),
            ("coast", 1),
            ("house-rule", 1),
            ("index", 1),
            ("npc", 1),
            ("npc/met", 1),
            ("one", 1),
            ("place", 1),
            ("session", 1),
            ("two", 1),
        ]
    );
    assert!(
        !names.iter().any(|(t, _)| *t == "not-a-tag"),
        "code is not tagged"
    );
    let npc: Vec<String> = state
        .vault_search("", Some("#npc"))
        .unwrap()
        .into_iter()
        .map(|h| h.path)
        .collect();
    assert_eq!(
        npc,
        ["People/Bramblewick.md", "Session 1.md"],
        "nested tags count"
    );

    let hits = state.vault_search("harbor", None).unwrap();
    assert_eq!(hits[0].path, "Places/Harbor Town.md");
    assert!(hits[0].name_match);
    assert!(hits
        .iter()
        .any(|h| h.path == "Home.md" && !h.name_match && !h.lines.is_empty()));
    let lighthouse = state.vault_search("LIGHTHOUSE", None).unwrap();
    assert!(lighthouse.iter().all(|h| !h.name_match));
    assert_eq!(lighthouse.len(), 2);
    assert!(state.vault_search("   ", None).unwrap().is_empty());
}

#[test]
fn new_notes_go_where_the_obsidian_settings_say() {
    let (v, _d, state) = opened();
    let first = state
        .vault_create_note(None, Some("Places/Harbor Town.md"), None)
        .unwrap();
    assert_eq!(first.path, "Inbox/Untitled.md");
    let second = state.vault_create_note(None, None, None).unwrap();
    assert_eq!(second.path, "Inbox/Untitled 1.md");
    let in_folder = state.vault_create_note(None, None, Some("People")).unwrap();
    assert_eq!(in_folder.path, "People/Untitled.md");
    // An unresolved link: by name into the new-note folder, or at the path it names.
    let named = state
        .vault_create_note(Some("Missing Note"), Some("Home.md"), None)
        .unwrap();
    assert_eq!(named.path, "Inbox/Missing Note.md");
    let pathed = state
        .vault_create_note(Some("Places/Lighthouse"), Some("Home.md"), None)
        .unwrap();
    assert_eq!(pathed.path, "Places/Lighthouse.md");
    assert_eq!(read(v.path(), "Places/Lighthouse.md"), b"");
    assert!(
        state
            .vault_create_note(Some("Missing Note"), Some("Home.md"), None)
            .is_err(),
        "taken"
    );
    assert!(state
        .vault_create_note(Some("bad|name"), None, None)
        .is_err());
    assert_eq!(
        state
            .vault_resolve("Home.md", &["Missing Note".into()])
            .unwrap()[0]
            .as_deref(),
        Some("Inbox/Missing Note.md")
    );
}

#[test]
fn rename_shows_every_link_change_and_updates_links_only_when_asked() {
    let (v, _d, state) = opened();
    let plan = state
        .vault_rename_preview("People/Bramblewick.md", "Bramblewick the Bold")
        .unwrap();
    assert_eq!(plan.to, "People/Bramblewick the Bold.md");
    let files: Vec<&str> = plan.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(files, ["Home.md", "Places/Harbor Town.md", "Session 1.md"]);
    let home = &plan.files[0];
    assert!(home.changes.iter().any(|c| c
        .after
        .contains("[[People/Bramblewick the Bold|Bramblewick]]")));

    let home_before = String::from_utf8(read(v.path(), "Home.md")).unwrap();
    let session_before = read(v.path(), "Session 1.md");
    let done = state
        .vault_rename("People/Bramblewick.md", "Bramblewick the Bold", true)
        .unwrap();
    assert_eq!(done.updated.len(), 3);
    assert!(done.skipped.is_empty());
    assert!(v.path().join("People/Bramblewick the Bold.md").is_file());
    assert!(!v.path().join("People/Bramblewick.md").exists());
    let expected_home = home_before
        .replace("[[People/Bramblewick|", "[[People/Bramblewick the Bold|")
        .replace(
            "[[Bramblewick#Appearance]]",
            "[[Bramblewick the Bold#Appearance]]",
        )
        .replace(
            "[[Bramblewick\\|a favour]]",
            "[[Bramblewick the Bold\\|a favour]]",
        );
    assert_eq!(
        String::from_utf8(read(v.path(), "Home.md")).unwrap(),
        expected_home
    );
    let expected_session = String::from_utf8(session_before)
        .unwrap()
        .replace("[[Bramblewick]]", "[[Bramblewick the Bold]]");
    assert_eq!(
        read(v.path(), "Session 1.md"),
        expected_session.into_bytes(),
        "BOM and CRLF kept"
    );

    // Without the update, only the file moves.
    let before = read(v.path(), "Home.md");
    let done = state
        .vault_rename("Places/Harbor Town.md", "Port Town", false)
        .unwrap();
    assert!(done.updated.is_empty());
    assert_eq!(read(v.path(), "Home.md"), before);
    assert!(
        state.vault_rename("Home.md", "Session 1", true).is_err(),
        "taken"
    );
    assert!(state.vault_rename("Home.md", "a/b", true).is_err());
}

#[test]
fn delete_uses_the_trash_and_asks_before_deleting_for_good() {
    let (v, _d, state) = opened();
    let bin = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let to_bin = |p: &Path| {
        std::fs::rename(p, bin.path().join(p.file_name().unwrap())).map_err(|e| e.to_string())
    };
    assert_eq!(
        state
            .vault_delete_with("Odd/Empty.md", false, to_bin)
            .unwrap(),
        DeleteStatus::Trashed
    );
    assert!(!v.path().join("Odd/Empty.md").exists() && bin.path().join("Empty.md").exists());

    let no_trash = |_: &Path| Err("no trash here".to_string());
    assert_eq!(
        state.vault_delete_with("map.png", false, no_trash).unwrap(),
        DeleteStatus::TrashUnavailable
    );
    assert!(
        v.path().join("map.png").exists(),
        "nothing deleted without the user's answer"
    );
    assert_eq!(
        state.vault_delete_with("map.png", true, no_trash).unwrap(),
        DeleteStatus::Deleted
    );
    assert!(!v.path().join("map.png").exists());
    assert!(
        state.vault_delete("Odd", true).is_err(),
        "folders are not deleted"
    );
}

#[test]
fn images_attachments_and_inserted_links() {
    let (v, _d, state) = opened();
    let url = state.vault_image("map.png").unwrap();
    assert!(url.starts_with("data:image/png;base64,iVBOR"));
    assert!(state.vault_image("Home.md").is_err());

    let elsewhere = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    std::fs::write(
        elsewhere.path().join("sketch.png"),
        b"\x89PNG\r\n\x1a\nfake",
    )
    .unwrap();
    let pick = |state: &AppState| {
        state
            .picked
            .grant(
                elsewhere.path().join("sketch.png"),
                PickPurpose::VaultAttachment,
            )
            .unwrap()
            .token
    };
    let first = state
        .vault_attach(&pick(&state), "Places/Harbor Town.md")
        .unwrap();
    assert_eq!(first.path, "Places/attachments/sketch.png");
    assert_eq!(first.link, "![[sketch.png]]");
    let second = state
        .vault_attach(&pick(&state), "Places/Harbor Town.md")
        .unwrap();
    assert_eq!(second.path, "Places/attachments/sketch 1.png");
    assert_eq!(
        read(v.path(), "Places/attachments/sketch 1.png"),
        b"\x89PNG\r\n\x1a\nfake"
    );
    assert!(state.vault_attach("bad", "Home.md").is_err());

    assert_eq!(
        state
            .vault_link_text("Home.md", "Places/Harbor Town.md", false)
            .unwrap(),
        "[[Harbor Town]]"
    );
    assert_eq!(
        state
            .vault_link_text("Home.md", "Archive/Home.md", false)
            .unwrap(),
        "[[Archive/Home]]"
    );
    std::fs::write(
        v.path().join(".obsidian/app.json"),
        r#"{"useMarkdownLinks": true}"#,
    )
    .unwrap();
    assert_eq!(
        state
            .vault_link_text("Home.md", "Places/Harbor Town.md", false)
            .unwrap(),
        "[Harbor Town](Harbor%20Town.md)"
    );
    assert!(state.vault_link_text("Home.md", "Nope.md", false).is_err());
}

#[test]
fn an_unavailable_vault_is_reported_and_kept() {
    let (v, _d, state) = opened();
    let moved = v.path().with_file_name(format!(
        "{}-away",
        v.path().file_name().unwrap().to_string_lossy()
    ));
    std::fs::rename(v.path(), &moved).unwrap();
    let tree = state.vault_tree().unwrap();
    assert!(tree.unavailable && tree.entries.is_empty() && tree.dir.is_some());
    assert!(state.data_dirs().unwrap().vault_unavailable);
    assert!(state.vault_read("Home.md").is_err());
    state
        .save_settings(&serde_json::to_value(state.settings().unwrap()).unwrap())
        .unwrap();
    assert!(
        state.settings().unwrap().vault_dir.is_some(),
        "the setting is kept"
    );
    std::fs::rename(&moved, v.path()).unwrap();
    assert!(
        !state.vault_tree().unwrap().unavailable,
        "back once the folder is"
    );
}
