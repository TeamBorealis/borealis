use super::*;

fn targets(text: &str) -> Vec<String> {
    scan(text).links.into_iter().map(|l| l.target).collect()
}

#[test]
fn front_matter_needs_the_first_line_and_a_closing_line() {
    let t = "---\ntags: [a]\n---\nBody";
    let fm = front_matter(t).unwrap();
    assert_eq!(&t[fm.yaml.clone()], "tags: [a]\n");
    assert_eq!(&t[fm.range.end..], "Body");
    assert!(
        front_matter("\n---\na: 1\n---\n").is_none(),
        "not on the first line"
    );
    assert!(front_matter("---\na: 1\nno end").is_none());
    let crlf = "---\r\na: 1\r\n---\r\nBody";
    assert_eq!(&crlf[front_matter(crlf).unwrap().range.end..], "Body");
    let empty = "---\n---\n";
    assert_eq!(front_matter(empty).unwrap().yaml, 4..4);
}

#[test]
fn wikilinks_keep_the_file_part_apart_from_headings_blocks_and_aliases() {
    let t = "See [[Harbor Town]], [[People/Bramblewick#Appearance|the gnome]], [[Session 1#^loot]] and ![[map.png]].";
    let links = scan(t).links;
    assert_eq!(links.len(), 4);
    assert_eq!(links[0].target, "Harbor Town");
    assert_eq!(&t[links[0].target_range.clone()], "Harbor Town");
    assert_eq!(links[1].target, "People/Bramblewick");
    assert_eq!(links[1].subpath.as_deref(), Some("Appearance"));
    assert_eq!(links[2].subpath.as_deref(), Some("^loot"));
    assert!(links[3].embed && !links[3].markdown);
    assert_eq!(&t[links[3].range.clone()], "![[map.png]]");
}

#[test]
fn links_inside_code_do_not_count() {
    let t = "Real [[A]].\n\n```\n[[Not B]] #nottag\n```\n\nInline `[[Not C]]` and ~~~\n\n    [[Indented D]]\n\nLast [[E]]";
    assert_eq!(targets(t), ["A", "E"]);
}

#[test]
fn a_heading_only_link_points_at_the_note_itself() {
    let l = &scan("[[#Loot]]").links[0];
    assert_eq!(l.target, "");
    assert_eq!(l.subpath.as_deref(), Some("Loot"));
}

#[test]
fn escaped_pipes_in_tables_split_the_alias() {
    let t = "| a | [[Harbor Town\\|harbor]] |\n|---|---|\n";
    let l = &scan(t).links[0];
    assert_eq!(l.target, "Harbor Town");
}

#[test]
fn markdown_links_to_vault_files_are_decoded_and_web_links_skipped() {
    let t = "[town](Places/Harbor%20Town.md) [x](<My Note.md#Top>) ![](map.png) [web](https://example.org) [mail](mailto:a@b.c) [here](#Top)";
    let links = scan(t).links;
    assert_eq!(links.len(), 3);
    assert_eq!(links[0].target, "Places/Harbor Town.md");
    assert_eq!(&t[links[0].target_range.clone()], "Places/Harbor%20Town.md");
    assert!(links[0].markdown);
    assert_eq!(links[1].target, "My Note.md");
    assert_eq!(links[1].subpath.as_deref(), Some("Top"));
    assert!(links[2].embed);
}

#[test]
fn tags_follow_whitespace_and_need_a_letter() {
    let t = "#campaign start, then #npc/ally and #2024 or #y2024.\nA heading:\n# Not a tag\nurl.com/#frag [[Note#Heading]] `#code`\n#last";
    let tags = scan(t).tags;
    assert_eq!(tags, ["campaign", "npc/ally", "y2024", "last"]);
}

#[test]
fn front_matter_tags_come_as_lists_or_strings() {
    assert_eq!(
        front_matter_tags("tags:\n  - place\n  - '#coast'\n"),
        ["place", "coast"]
    );
    assert_eq!(front_matter_tags("tags: [a, b]"), ["a", "b"]);
    assert_eq!(
        front_matter_tags("tags: one, two three"),
        ["one", "two", "three"]
    );
    assert_eq!(front_matter_tags("tag: solo"), ["solo"]);
    assert_eq!(front_matter_tags("tags: 2024"), ["2024"]);
    assert!(front_matter_tags("tags:").is_empty());
    assert!(
        front_matter_tags("not: [valid").is_empty(),
        "broken YAML has no tags"
    );
    assert!(front_matter_tags("- just\n- a list").is_empty());
}

#[test]
fn a_full_note_scans_front_matter_links_and_tags() {
    let t = "---\ntags: [session]\nlocation: \"[[Harbor Town]]\"\n---\n# Session\n\nWe met [[Bramblewick]] #npc.\n";
    let s = scan(t);
    assert!(s.front_matter.is_some());
    assert_eq!(targets(t), ["Harbor Town", "Bramblewick"]);
    assert_eq!(s.tags, ["npc", "session"]);
}
