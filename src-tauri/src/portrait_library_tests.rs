//! Unit tests of the portrait library listing and image reads (GitHub #32).

use super::*;
use std::fs;

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
const JPG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F'];
const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00";
const WEBP: &[u8] = b"RIFF\x10\x00\x00\x00WEBPVP8 ";

fn ids(scan: &Scan) -> Vec<&str> {
    scan.images.iter().map(|i| i.id.as_str()).collect()
}

fn folder() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("b-elf.png"), PNG).unwrap();
    fs::write(dir.path().join("A-dwarf.JPEG"), JPG).unwrap();
    fs::write(dir.path().join("c.gif"), GIF).unwrap();
    fs::write(dir.path().join("d.webp"), WEBP).unwrap();
    dir
}

#[test]
fn lists_only_real_images_of_the_accepted_types_in_name_order() {
    let dir = folder();
    fs::write(dir.path().join("notes.txt"), b"hello").unwrap();
    fs::write(dir.path().join("old.bmp"), b"BM\0\0").unwrap();
    fs::write(dir.path().join("vector.svg"), b"<svg/>").unwrap();
    fs::write(dir.path().join(".hidden.png"), PNG).unwrap();
    fs::write(dir.path().join("._b-elf.png"), b"\0\x05\x16\x07").unwrap();
    fs::write(dir.path().join("fake.png"), b"not a png at all").unwrap();
    fs::write(dir.path().join("empty.jpg"), b"").unwrap();
    let scan = scan(dir.path()).unwrap();
    assert_eq!(ids(&scan), ["A-dwarf.JPEG", "b-elf.png", "c.gif", "d.webp"]);
    assert_eq!(
        scan.refused, 2,
        "the fake PNG and the empty JPEG are refused"
    );
    assert!(!scan.truncated);
    assert!(scan.images.iter().all(|i| i.folder.is_none()));
}

#[test]
fn huge_files_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut big = PNG.to_vec();
    big.resize(MAX_PORTRAIT_BYTES + 1, 0);
    fs::write(dir.path().join("huge.png"), &big).unwrap();
    let mut fits = PNG.to_vec();
    fits.resize(MAX_PORTRAIT_BYTES, 0);
    fs::write(dir.path().join("fits.png"), &fits).unwrap();
    let scan = scan(dir.path()).unwrap();
    assert_eq!(ids(&scan), ["fits.png"]);
    assert_eq!(scan.refused, 1);
    assert!(read_image(dir.path(), "huge.png").is_err());
    assert!(read_image(dir.path(), "fits.png").is_ok());
}

#[test]
fn sub_folders_are_listed_one_level_down_after_the_top_level() {
    let dir = folder();
    fs::create_dir_all(dir.path().join("Elves/Deeper")).unwrap();
    fs::write(dir.path().join("Elves/wood.png"), PNG).unwrap();
    fs::write(dir.path().join("Elves/Deeper/too-deep.png"), PNG).unwrap();
    fs::create_dir(dir.path().join(".cache")).unwrap();
    fs::write(dir.path().join(".cache/thumb.png"), PNG).unwrap();
    let scan = scan(dir.path()).unwrap();
    assert_eq!(
        ids(&scan),
        [
            "A-dwarf.JPEG",
            "b-elf.png",
            "c.gif",
            "d.webp",
            "Elves/wood.png"
        ]
    );
    let wood = scan.images.last().unwrap();
    assert_eq!(wood.name, "wood.png");
    assert_eq!(wood.folder.as_deref(), Some("Elves"));
    assert!(read_image(dir.path(), "Elves/wood.png").is_ok());
    assert!(read_image(dir.path(), "Elves/Deeper/too-deep.png").is_err());
}

#[test]
fn the_listing_is_capped() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..=MAX_LIBRARY_IMAGES {
        fs::write(dir.path().join(format!("p{i:04}.png")), PNG).unwrap();
    }
    let scan = scan(dir.path()).unwrap();
    assert_eq!(scan.images.len(), MAX_LIBRARY_IMAGES);
    assert!(scan.truncated);
}

#[test]
fn a_missing_folder_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("unplugged");
    assert!(scan(&gone).is_err());
    assert!(read_image(&gone, "a.png").is_err());
}

#[test]
fn read_image_returns_a_data_url_that_set_portrait_accepts() {
    let dir = folder();
    for (id, mime) in [
        ("b-elf.png", "image/png"),
        ("A-dwarf.JPEG", "image/jpeg"),
        ("c.gif", "image/gif"),
        ("d.webp", "image/webp"),
    ] {
        let url = read_image(dir.path(), id).unwrap();
        assert!(
            url.starts_with(&format!("data:{mime};base64,")),
            "{id}: {url}"
        );
        assert!(crate::extras::portrait_from_data_url(&url).is_ok(), "{id}");
    }
}

#[test]
fn ids_that_leave_the_folder_or_name_other_files_are_refused() {
    let outer = tempfile::tempdir().unwrap();
    let dir = outer.path().join("lib");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("ok.png"), PNG).unwrap();
    fs::write(outer.path().join("secret.png"), PNG).unwrap();
    fs::write(dir.join("notes.txt"), b"x").unwrap();
    let abs = outer
        .path()
        .join("secret.png")
        .to_string_lossy()
        .into_owned();
    for bad in [
        "../secret.png",
        "./ok.png",
        "",
        "/",
        abs.as_str(),
        "a/b/c.png",
        "..\\secret.png",
        "C:secret.png",
        ".hidden.png",
        "notes.txt",
        "missing.png",
        "ok.png/",
    ] {
        assert!(read_image(&dir, bad).is_err(), "{bad:?} must be refused");
    }
    assert!(read_image(&dir, "ok.png").is_ok());
}

#[cfg(unix)]
#[test]
fn links_out_of_the_folder_are_skipped_and_links_inside_are_kept() {
    use std::os::unix::fs::symlink;
    let outer = tempfile::tempdir().unwrap();
    let dir = outer.path().join("lib");
    fs::create_dir_all(dir.join("real")).unwrap();
    fs::write(dir.join("real/inside.png"), PNG).unwrap();
    fs::write(outer.path().join("outside.png"), PNG).unwrap();
    fs::create_dir(outer.path().join("elsewhere")).unwrap();
    fs::write(outer.path().join("elsewhere/far.png"), PNG).unwrap();
    symlink(outer.path().join("outside.png"), dir.join("escape.png")).unwrap();
    symlink(dir.join("real/inside.png"), dir.join("alias.png")).unwrap();
    symlink(outer.path().join("elsewhere"), dir.join("linked")).unwrap();
    let scan = scan(&dir).unwrap();
    assert_eq!(ids(&scan), ["alias.png", "real/inside.png"]);
    assert_eq!(scan.refused, 1);
    assert!(read_image(&dir, "escape.png").is_err());
    assert!(read_image(&dir, "linked/far.png").is_err());
    assert!(read_image(&dir, "alias.png").is_ok());
}

#[cfg(unix)]
#[test]
fn an_unreadable_sub_folder_is_skipped() {
    use std::os::unix::fs::PermissionsExt;
    let dir = folder();
    let locked = dir.path().join("Locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("x.png"), PNG).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let readable = fs::read_dir(&locked).is_ok(); // root can read anything
    let scan = scan(dir.path());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    let scan = scan.unwrap();
    assert_eq!(scan.images.len(), if readable { 5 } else { 4 });
}
