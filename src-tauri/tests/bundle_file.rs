//! A2 Stage 0: bundles as files. Export -> open (preview) -> accept through the app service, the
//! same validated apply path a device transfer uses; hostile files are refused with a plain message
//! and nothing is written.
#![cfg(feature = "device-link")]

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use borealis_character::Character;
use borealis_lib::content_packs::NullSink;
use borealis_lib::device_sync::{inbox, DeviceSync, StateRef, SyncConfig, SyncEvents};
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::{AppPaths, AppState};
use borealis_lib::sync_dto::{LinkPackChoice, LinkSelection, LinkStatus};
use borealis_link::{MemoryStore, SecureStore};

struct Quiet;

impl SyncEvents for Quiet {
    fn status(&self, _: &LinkStatus) {}
    fn content(&self) -> &dyn borealis_lib::content_packs::EventSink {
        &NullSink
    }
}

struct Dev {
    data: tempfile::TempDir,
    state: Arc<AppState>,
    sync: Arc<DeviceSync>,
}

fn dev(name: &str) -> Dev {
    let data = tempfile::tempdir().unwrap();
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    };
    let state = Arc::new(AppState::new(paths).unwrap());
    let store = Arc::new(MemoryStore::default());
    let config = SyncConfig {
        bind_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        device_name: name.into(),
        discovery: false,
    };
    let sync = DeviceSync::new(
        Arc::clone(&state) as Arc<dyn StateRef>,
        Arc::new(Quiet),
        config,
        move || Ok(Arc::clone(&store) as Arc<dyn SecureStore>),
    );
    Dev {
        data,
        state,
        sync: Arc::new(sync),
    }
}

const ID: &str = "c_0123456789abcdef0123456789abcdef";

fn put_character(d: &Dev, name: &str, age: Duration) {
    let mut c = Character::new(name);
    c.meta.id = ID.into();
    let dir = d.state.characters_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{ID}.json"));
    std::fs::write(&path, c.to_json_string().unwrap()).unwrap();
    let when = SystemTime::now() - age;
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

fn selection() -> LinkSelection {
    LinkSelection {
        character_ids: vec![ID.into()],
        packs: LinkPackChoice::Required,
    }
}

fn export(from: &Dev, dir: &Path) -> PathBuf {
    let built = from.sync.build_bundle_file(&selection()).unwrap();
    let dest = dir.join("Mira.borealis");
    let done = DeviceSync::save_bundle(&built, &dest).unwrap();
    assert_eq!(done.file_name, "Mira.borealis");
    assert_eq!((done.characters, done.packs), (1, 0));
    assert!(done.bytes > 0);
    dest
}

fn token(to: &Dev, path: &Path) -> String {
    to.state
        .picked
        .grant(path.to_path_buf(), PickPurpose::BundleFile)
        .unwrap()
        .token
}

fn open(to: &Dev, path: &Path) -> borealis_lib::sync_dto::BundlePreview {
    to.sync.open_bundle(&token(to, path)).unwrap()
}

fn local_name(d: &Dev) -> Option<String> {
    Character::load(&d.state.characters_dir().join(format!("{ID}.json")))
        .ok()
        .map(|c| c.meta.name)
}

/// Rewrite a bundle zip entry by entry; `f` may change an entry's bytes.
fn rewrite(src: &Path, dst: &Path, f: impl Fn(&str, &mut Vec<u8>)) {
    let mut zin = zip::ZipArchive::new(std::fs::File::open(src).unwrap()).unwrap();
    let mut zout = zip::ZipWriter::new(std::fs::File::create(dst).unwrap());
    for i in 0..zin.len() {
        let mut entry = zin.by_index(i).unwrap();
        let name = entry.name().to_string();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        f(&name, &mut bytes);
        zout.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zout.write_all(&bytes).unwrap();
    }
    zout.finish().unwrap();
}

#[test]
fn export_open_preview_accept_round_trip() {
    let (a, b) = (dev("Mac"), dev("Phone"));
    put_character(&a, "Mira", Duration::from_secs(60));
    let files = tempfile::tempdir().unwrap();
    let file = export(&a, files.path());

    // Preview: one new character, labelled with its name; nothing written yet.
    let preview = open(&b, &file);
    assert_eq!(preview.file_name, "Mira.borealis");
    assert_eq!(preview.source_device, "Mac");
    let rows = &preview.preview.items;
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].label.as_str(), rows[0].action.as_str()), ("Mira", "new"));
    assert_eq!(local_name(&b), None, "previewing writes nothing");

    // Accept: applied through the validated apply path, with the source edit time stamped.
    let summary = b.sync.accept_bundle(&preview.bundle_id).unwrap();
    assert_eq!((summary.added, summary.replaced, summary.kept_local), (1, 0, 0));
    assert_eq!(local_name(&b).as_deref(), Some("Mira"));
    let src_time = std::fs::metadata(a.state.characters_dir().join(format!("{ID}.json")))
        .unwrap()
        .modified()
        .unwrap();
    let got_time = std::fs::metadata(b.state.characters_dir().join(format!("{ID}.json")))
        .unwrap()
        .modified()
        .unwrap();
    let skew = src_time
        .duration_since(got_time)
        .unwrap_or_else(|e| e.duration());
    assert!(skew < Duration::from_secs(2), "edit time kept: {skew:?}");
    assert!(file.exists(), "the user's file is never deleted");

    // Opening it again: already here.
    let again = open(&b, &file);
    assert_eq!(again.preview.items[0].action, "identical");
    b.sync.discard_bundle(&again.bundle_id);
    assert!(b.sync.accept_bundle(&again.bundle_id).is_err());
}

#[test]
fn newest_edit_wins_and_the_replaced_copy_goes_to_the_trash() {
    let (a, b) = (dev("Mac"), dev("Phone"));
    let files = tempfile::tempdir().unwrap();
    // The phone has an older copy: the bundle replaces it, the old one is kept in the trash.
    put_character(&b, "Mira (old)", Duration::from_secs(3600));
    put_character(&a, "Mira", Duration::from_secs(60));
    let newer = export(&a, files.path());
    let preview = open(&b, &newer);
    assert_eq!(preview.preview.items[0].action, "replace");
    let summary = b.sync.accept_bundle(&preview.bundle_id).unwrap();
    assert_eq!((summary.replaced, summary.added), (1, 0));
    assert_eq!(local_name(&b).as_deref(), Some("Mira"));
    let trash = b.data.path().join("sync").join("trash");
    assert_eq!(std::fs::read_dir(trash).unwrap().count(), 1, "old copy kept");

    // The phone's copy is newer than an old bundle: kept.
    let (c, d) = (dev("Mac 2"), dev("Phone 2"));
    put_character(&c, "Mira (stale)", Duration::from_secs(7200));
    put_character(&d, "Mira (mine)", Duration::from_secs(10));
    let older = export(&c, files.path());
    let preview = open(&d, &older);
    assert_eq!(preview.preview.items[0].action, "keep_local");
    let summary = d.sync.accept_bundle(&preview.bundle_id).unwrap();
    assert_eq!((summary.kept_local, summary.replaced), (1, 0));
    assert_eq!(local_name(&d).as_deref(), Some("Mira (mine)"));
}

#[test]
fn a_tampered_bundle_is_refused_with_a_plain_message_and_writes_nothing() {
    let (a, b) = (dev("Mac"), dev("Phone"));
    put_character(&a, "Mira", Duration::from_secs(60));
    let files = tempfile::tempdir().unwrap();
    let good = export(&a, files.path());
    let bad = files.path().join("tampered.borealis");
    // Same size, different content: the manifest's checksum no longer matches.
    rewrite(&good, &bad, |name, bytes| {
        if name == format!("characters/{ID}.json") {
            let at = bytes.iter().position(|&c| c == b'M').unwrap();
            bytes[at] = b'N';
        }
    });
    let err = b.sync.open_bundle(&token(&b, &bad)).unwrap_err();
    assert!(
        err.message.contains("damaged or was changed") && err.message.contains("Nothing was changed"),
        "{}",
        err.message
    );
    assert!(!err.message.contains("sha") && !err.message.contains("zip"));
    assert_eq!(local_name(&b), None);
}

#[test]
fn a_file_changed_between_preview_and_accept_is_refused() {
    let (a, b) = (dev("Mac"), dev("Phone"));
    put_character(&a, "Mira", Duration::from_secs(60));
    let files = tempfile::tempdir().unwrap();
    let file = export(&a, files.path());
    let preview = open(&b, &file);
    let other = dev("Other");
    put_character(&other, "Mallory", Duration::from_secs(30));
    let elsewhere = tempfile::tempdir().unwrap();
    let swapped = export(&other, elsewhere.path());
    std::fs::copy(&swapped, &file).unwrap();
    let err = b.sync.accept_bundle(&preview.bundle_id).unwrap_err();
    assert!(err.message.contains("Nothing was changed"), "{}", err.message);
    assert_eq!(local_name(&b), None);
}

#[test]
fn something_that_is_not_a_bundle_or_is_oversize_is_refused_plainly() {
    let b = dev("Phone");
    let files = tempfile::tempdir().unwrap();
    let junk = files.path().join("notes.borealis");
    std::fs::write(&junk, b"just some text, not a zip").unwrap();
    let err = b.sync.open_bundle(&token(&b, &junk)).unwrap_err();
    assert!(err.message.contains("not a Borealis bundle"), "{}", err.message);

    // A zip with no manifest.
    let plain_zip = files.path().join("plain.borealis");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&plain_zip).unwrap());
    z.start_file("hello.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"hi").unwrap();
    z.finish().unwrap();
    let err = b.sync.open_bundle(&token(&b, &plain_zip)).unwrap_err();
    assert!(err.message.contains("not a Borealis bundle"), "{}", err.message);

    // A file whose single entry is far over the per-file cap (64 MiB), compressed to almost nothing.
    let huge = files.path().join("huge.borealis");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&huge).unwrap());
    z.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(&vec![b' '; 70 << 20]).unwrap();
    z.finish().unwrap();
    let err = b.sync.open_bundle(&token(&b, &huge)).unwrap_err();
    assert!(
        err.message.contains("larger than Borealis accepts") || err.message.contains("damaged"),
        "{}",
        err.message
    );
    assert!(err.message.ends_with("Nothing was changed."));
    assert_eq!(local_name(&b), None);
}

#[test]
fn an_unknown_or_foreign_token_is_refused() {
    let b = dev("Phone");
    assert!(b.sync.open_bundle("pick_unknown").is_err());
    // A token minted for another purpose cannot open a bundle.
    let files = tempfile::tempdir().unwrap();
    let f = files.path().join("x.borealis");
    std::fs::write(&f, b"x").unwrap();
    let t = b.state.picked.grant(f, PickPurpose::ImportFile).unwrap().token;
    assert!(b.sync.open_bundle(&t).is_err());
}

#[test]
fn os_opened_bundles_are_queued_by_path_and_android_bytes_become_files() {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    let b = dev("Phone");
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("Shared.BOREALIS");
    std::fs::write(&path, b"zip bytes").unwrap();
    assert!(inbox::push_opened_path(&path));
    assert!(!inbox::push_opened_path(&files.path().join("a.dnd5e")));
    assert_eq!(inbox::take_opened_paths(), vec![path]);
    assert!(inbox::take_opened_paths().is_empty());

    // Android: bytes with a .borealis name are set aside as a file in sync/incoming; others stay.
    let opened = vec![
        borealis_lib::content_preview::OpenedFile {
            file_name: "Mira.borealis".into(),
            content_base64: STANDARD.encode(b"PK-not-checked-here"),
        },
        borealis_lib::content_preview::OpenedFile {
            file_name: "set.index".into(),
            content_base64: STANDARD.encode(b"<index/>"),
        },
        borealis_lib::content_preview::OpenedFile {
            file_name: "msf:1234".into(),
            content_base64: STANDARD.encode(b"not a zip at all"),
        },
    ];
    let rest = inbox::claim_bundles(&b.state, opened);
    let names: Vec<_> = rest.iter().map(|f| f.file_name.as_str()).collect();
    assert_eq!(names, ["set.index", "msf:1234"]);
    let queued = inbox::take_opened_paths();
    assert_eq!(queued.len(), 1);
    assert!(queued[0].starts_with(borealis_lib::device_sync::incoming_dir(&b.state)));
}

#[test]
fn a_bundle_copied_by_the_app_is_deleted_after_accept_or_discard() {
    let (a, b) = (dev("Mac"), dev("Phone"));
    put_character(&a, "Mira", Duration::from_secs(60));
    let files = tempfile::tempdir().unwrap();
    let file = export(&a, files.path());
    let incoming = borealis_lib::device_sync::incoming_dir(&b.state);
    std::fs::create_dir_all(&incoming).unwrap();
    let mine = incoming.join("open-1.borealis");
    std::fs::copy(&file, &mine).unwrap();
    let p = open(&b, &mine);
    b.sync.accept_bundle(&p.bundle_id).unwrap();
    assert!(!mine.exists(), "our own copy is removed after Accept");
    std::fs::copy(&file, &mine).unwrap();
    let p = open(&b, &mine);
    b.sync.discard_bundle(&p.bundle_id);
    assert!(!mine.exists(), "and after Cancel");
}
