use super::*;

fn files() -> FileSet {
    FileSet::new(
        [
            "Home.md",
            "Archive/Home.md",
            "Places/Harbor Town.md",
            "Places/attachments/sketch.png",
        ]
        .map(str::to_string),
    )
}

#[test]
fn a_missing_or_broken_file_means_obsidian_defaults() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(VaultConfig::load(dir.path()), VaultConfig::default());
    std::fs::create_dir(dir.path().join(".obsidian")).unwrap();
    std::fs::write(dir.path().join(APP_JSON), "{ not json").unwrap();
    let cfg = VaultConfig::load(dir.path());
    assert_eq!(cfg.new_file_location, NewFileLocation::Root);
    assert_eq!(cfg.new_link_format, LinkFormat::Shortest);
    assert!(!cfg.use_markdown_links);
    assert_eq!(VaultConfig::from_json("[]"), VaultConfig::default());
    assert_eq!(
        VaultConfig::from_json(r#"{"useMarkdownLinks":"yes","newFileLocation":7}"#),
        VaultConfig::default(),
        "wrong types fall back"
    );
}

#[test]
fn the_options_are_read() {
    let cfg = VaultConfig::from_json(
        r#"{"newFileLocation":"folder","newFileFolderPath":"/Inbox/","attachmentFolderPath":"./attachments",
            "useMarkdownLinks":true,"newLinkFormat":"relative","alwaysUpdateLinks":true}"#,
    );
    assert_eq!(cfg.new_file_location, NewFileLocation::Folder);
    assert_eq!(cfg.new_file_folder_path, "Inbox");
    assert_eq!(cfg.attachment_folder_path, "./attachments");
    assert!(cfg.use_markdown_links);
    assert_eq!(cfg.new_link_format, LinkFormat::Relative);
}

#[test]
fn new_notes_go_where_new_file_location_says() {
    let mut cfg = VaultConfig::default();
    assert_eq!(cfg.new_note_folder(Some("Places/Harbor Town.md")), "");
    cfg.new_file_location = NewFileLocation::Current;
    assert_eq!(cfg.new_note_folder(Some("Places/Harbor Town.md")), "Places");
    assert_eq!(cfg.new_note_folder(None), "");
    cfg.new_file_location = NewFileLocation::Folder;
    cfg.new_file_folder_path = "Inbox".into();
    assert_eq!(cfg.new_note_folder(Some("Places/Harbor Town.md")), "Inbox");
}

#[test]
fn attachments_go_where_attachment_folder_path_says() {
    let mut cfg = VaultConfig::default();
    assert_eq!(
        cfg.attachment_folder("Places/Harbor Town.md"),
        "",
        "default: the vault folder"
    );
    cfg.attachment_folder_path = "./".into();
    assert_eq!(cfg.attachment_folder("Places/Harbor Town.md"), "Places");
    cfg.attachment_folder_path = "./attachments".into();
    assert_eq!(
        cfg.attachment_folder("Places/Harbor Town.md"),
        "Places/attachments"
    );
    assert_eq!(cfg.attachment_folder("Home.md"), "attachments");
    cfg.attachment_folder_path = "Assets/Images".into();
    assert_eq!(
        cfg.attachment_folder("Places/Harbor Town.md"),
        "Assets/Images"
    );
    cfg.attachment_folder_path = "../outside".into();
    assert_eq!(
        cfg.attachment_folder("Home.md"),
        "",
        "never outside the vault"
    );
    cfg.attachment_folder_path = ".obsidian".into();
    assert_eq!(
        cfg.attachment_folder("Home.md"),
        "",
        "never a hidden folder"
    );
}

#[test]
fn inserted_links_follow_the_link_format() {
    let f = files();
    let mut cfg = VaultConfig::default();
    assert_eq!(
        cfg.link_text(&f, "Home.md", "Places/Harbor Town.md", false),
        "[[Harbor Town]]"
    );
    assert_eq!(
        cfg.link_text(&f, "Home.md", "Archive/Home.md", false),
        "[[Archive/Home]]"
    );
    assert_eq!(
        cfg.link_text(&f, "Home.md", "Places/attachments/sketch.png", true),
        "![[sketch.png]]"
    );
    cfg.new_link_format = LinkFormat::Absolute;
    assert_eq!(
        cfg.link_text(&f, "Home.md", "Places/Harbor Town.md", false),
        "[[Places/Harbor Town]]"
    );
    cfg.new_link_format = LinkFormat::Relative;
    assert_eq!(
        cfg.link_text(&f, "Archive/Home.md", "Places/Harbor Town.md", false),
        "[[../Places/Harbor Town]]"
    );
}

#[test]
fn markdown_links_keep_the_extension_and_encode_spaces() {
    let f = files();
    let cfg = VaultConfig {
        use_markdown_links: true,
        ..VaultConfig::default()
    };
    assert_eq!(
        cfg.link_text(&f, "Home.md", "Places/Harbor Town.md", false),
        "[Harbor Town](Harbor%20Town.md)"
    );
    assert_eq!(
        cfg.link_text(&f, "Home.md", "Places/attachments/sketch.png", true),
        "![](sketch.png)"
    );
    let absolute = VaultConfig {
        new_link_format: LinkFormat::Absolute,
        ..cfg
    };
    assert_eq!(
        absolute.link_text(&f, "Home.md", "Places/Harbor Town.md", false),
        "[Harbor Town](Places/Harbor%20Town.md)"
    );
}
