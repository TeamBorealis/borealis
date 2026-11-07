use super::*;

fn vault() -> FileSet {
    FileSet::new(
        [
            "Home.md",
            "Archive/Home.md",
            "Places/Harbor Town.md",
            "People/Bramblewick.md",
            "People/Notes/Bramblewick.md",
            "Session 1.md",
            "v1.2 rules.md",
            "map.png",
            "Places/map.png",
            "Board.canvas",
        ]
        .map(str::to_string),
    )
}

#[test]
fn a_unique_name_resolves_from_anywhere_ignoring_case() {
    let v = vault();
    assert_eq!(
        v.resolve("Home.md", "Harbor Town"),
        Some("Places/Harbor Town.md")
    );
    assert_eq!(
        v.resolve("Archive/Home.md", "harbor town"),
        Some("Places/Harbor Town.md")
    );
    assert_eq!(
        v.resolve("Home.md", "Harbor Town.md"),
        Some("Places/Harbor Town.md")
    );
    assert_eq!(v.resolve("Home.md", "SESSION 1"), Some("Session 1.md"));
}

#[test]
fn names_with_dots_still_mean_notes() {
    assert_eq!(
        vault().resolve("Home.md", "v1.2 rules"),
        Some("v1.2 rules.md")
    );
}

#[test]
fn other_files_need_their_extension() {
    let v = vault();
    assert_eq!(v.resolve("Home.md", "Board.canvas"), Some("Board.canvas"));
    assert_eq!(v.resolve("Home.md", "Board"), None);
}

#[test]
fn a_shared_name_prefers_the_same_folder_then_the_shortest_path() {
    let v = vault();
    assert_eq!(v.resolve("Home.md", "Home"), Some("Home.md"));
    assert_eq!(
        v.resolve("Archive/Other.md", "Home"),
        Some("Archive/Home.md")
    );
    assert_eq!(
        v.resolve("Session 1.md", "Bramblewick"),
        Some("People/Bramblewick.md")
    );
    assert_eq!(
        v.resolve("People/Notes/x.md", "Bramblewick"),
        Some("People/Notes/Bramblewick.md")
    );
    assert_eq!(
        v.resolve("Places/Harbor Town.md", "map.png"),
        Some("Places/map.png")
    );
    assert_eq!(v.resolve("Home.md", "map.png"), Some("map.png"));
}

#[test]
fn paths_resolve_from_the_vault_folder_or_by_their_end() {
    let v = vault();
    assert_eq!(
        v.resolve("Home.md", "Archive/Home"),
        Some("Archive/Home.md")
    );
    assert_eq!(
        v.resolve("Home.md", "/Archive/Home"),
        Some("Archive/Home.md")
    );
    assert_eq!(
        v.resolve("Home.md", "Notes/Bramblewick"),
        Some("People/Notes/Bramblewick.md")
    );
    assert_eq!(v.resolve("Home.md", "Elsewhere/Bramblewick"), None);
}

#[test]
fn relative_paths_resolve_from_the_note() {
    let v = vault();
    assert_eq!(
        v.resolve("People/Notes/x.md", "../Bramblewick"),
        Some("People/Bramblewick.md")
    );
    assert_eq!(
        v.resolve("Places/x.md", "./Harbor Town"),
        Some("Places/Harbor Town.md")
    );
    assert_eq!(v.resolve("Home.md", "../../etc/passwd"), None);
}

#[test]
fn unresolved_and_self_links() {
    let v = vault();
    assert_eq!(v.resolve("Home.md", "Missing Note"), None);
    assert_eq!(
        v.resolve("Home.md", ""),
        Some("Home.md"),
        "[[#heading]] is the note itself"
    );
}

#[test]
fn the_shortest_link_uses_the_name_only_when_it_is_unique() {
    let v = vault();
    assert_eq!(v.shortest("Places/Harbor Town.md"), "Harbor Town");
    assert_eq!(v.shortest("Archive/Home.md"), "Archive/Home");
    assert_eq!(v.shortest("Board.canvas"), "Board.canvas");
    assert_eq!(v.shortest("Places/map.png"), "Places/map.png");
}

#[test]
fn relative_paths_are_computed_between_folders() {
    assert_eq!(
        relative("People/Notes", "People/Bramblewick.md"),
        "../Bramblewick.md"
    );
    assert_eq!(
        relative("", "Places/Harbor Town.md"),
        "Places/Harbor Town.md"
    );
    assert_eq!(relative("Places", "Places/x.md"), "x.md");
    assert_eq!(relative("A/B", "C/d.md"), "../../C/d.md");
    assert_eq!(strip_md("Note.MD"), "Note");
    assert_eq!(strip_md("map.png"), "map.png");
}
