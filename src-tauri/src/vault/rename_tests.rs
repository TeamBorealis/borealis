use super::*;

fn before() -> FileSet {
    FileSet::new(
        [
            "Home.md",
            "People/Bramblewick.md",
            "Places/Harbor Town.md",
            "Archive/Old Bramble.md",
            "map.png",
        ]
        .map(str::to_string),
    )
}

/// Rename `from` to `to` and return the edited text of `note`.
fn renamed_text(text: &str, note: &str, from: &str, to: &str) -> String {
    let b = before();
    let a = renamed(&b, from, to);
    let note_after = if note == from { to } else { note };
    apply(text, &edits_for(&b, &a, note, note_after, text, from, to))
}

#[test]
fn only_the_file_part_of_each_link_changes() {
    let text = "Met [[Bramblewick]], see [[Bramblewick#Appearance|the gnome]] and ![[People/Bramblewick#^quote]].\n\
                Also [[bramblewick.md]] and [[Harbor Town]].";
    let out = renamed_text(
        text,
        "Home.md",
        "People/Bramblewick.md",
        "People/Bramblewick the Bold.md",
    );
    assert_eq!(
        out,
        "Met [[Bramblewick the Bold]], see [[Bramblewick the Bold#Appearance|the gnome]] and ![[People/Bramblewick the Bold#^quote]].\n\
         Also [[Bramblewick the Bold.md]] and [[Harbor Town]]."
    );
}

#[test]
fn code_and_unrelated_text_stay_byte_for_byte() {
    let text = "---\r\nfriend: \"[[Bramblewick]]\"\r\n---\r\nText  \r\n```\r\n[[Bramblewick]]\r\n```\r\n`[[Bramblewick]]` [[Bramblewick]]\r\n";
    let out = renamed_text(text, "Home.md", "People/Bramblewick.md", "People/Bram.md");
    assert_eq!(
        out,
        "---\r\nfriend: \"[[Bram]]\"\r\n---\r\nText  \r\n```\r\n[[Bramblewick]]\r\n```\r\n`[[Bramblewick]]` [[Bram]]\r\n"
    );
}

#[test]
fn markdown_links_stay_markdown_and_keep_their_encoding() {
    let text = "[g](People/Bramblewick.md) [t](Places/Harbor%20Town.md#Docks) [a](<Places/Harbor Town.md>)";
    let out = renamed_text(
        text,
        "Home.md",
        "Places/Harbor Town.md",
        "Places/Port Town.md",
    );
    assert_eq!(
        out,
        "[g](People/Bramblewick.md) [t](Places/Port%20Town.md#Docks) [a](<Places/Port Town.md>)"
    );
    let plain = "[g](People/Bramblewick.md)";
    assert_eq!(
        renamed_text(
            plain,
            "Home.md",
            "People/Bramblewick.md",
            "People/Bram Bold.md"
        ),
        "[g](People/Bram%20Bold.md)",
        "a space in a bare destination is encoded so the link still works"
    );
}

#[test]
fn a_name_that_would_mean_another_file_gets_the_full_path() {
    // "Old Bramble" already exists in Archive; renaming Bramblewick to it would make [[Old Bramble]] ambiguous.
    let text = "[[Bramblewick]]";
    let out = renamed_text(
        text,
        "Home.md",
        "People/Bramblewick.md",
        "People/Old Bramble.md",
    );
    assert_eq!(out, "[[People/Old Bramble]]");
}

#[test]
fn links_to_other_files_and_unresolved_links_are_left_alone() {
    let text = "[[Missing]] [[Harbor Town]] ![[map.png]]";
    assert_eq!(
        renamed_text(text, "Home.md", "People/Bramblewick.md", "People/Bram.md"),
        text
    );
}

#[test]
fn the_renamed_note_updates_links_to_itself_but_not_heading_links() {
    let text = "# Bramblewick\nSee [[#Appearance]] and [[Bramblewick#Appearance]].";
    let out = renamed_text(
        text,
        "People/Bramblewick.md",
        "People/Bramblewick.md",
        "People/Bram.md",
    );
    assert_eq!(
        out,
        "# Bramblewick\nSee [[#Appearance]] and [[Bram#Appearance]]."
    );
}

#[test]
fn attachments_keep_their_extension() {
    let text = "![[map.png]] ![](map.png)";
    let out = renamed_text(text, "Home.md", "map.png", "world map.png");
    assert_eq!(out, "![[world map.png]] ![](world%20map.png)");
}

#[test]
fn line_changes_show_before_and_after() {
    let text = "one\r\ntwo [[Bramblewick]] and [[Bramblewick]]\r\nthree";
    let b = before();
    let a = renamed(&b, "People/Bramblewick.md", "People/Bram.md");
    let edits = edits_for(
        &b,
        &a,
        "Home.md",
        "Home.md",
        text,
        "People/Bramblewick.md",
        "People/Bram.md",
    );
    assert_eq!(edits.len(), 2);
    assert_eq!(
        line_changes(text, &edits),
        [LineChange {
            line: 2,
            before: "two [[Bramblewick]] and [[Bramblewick]]".into(),
            after: "two [[Bram]] and [[Bram]]".into(),
        }]
    );
}
