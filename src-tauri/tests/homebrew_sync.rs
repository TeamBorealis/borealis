//! WB-24 sync checks (homebrew creator spec 14.4, 10.1, HC7, SY2-SY4) through two real devices and a `.borealis` bundle
//! (the same validated apply path as Send/Receive):
//!
//! * a homebrew pack travels with its `creator.json` (byte for byte) and arrives editable; drafts never travel;
//! * the pack is one item: a newer copy replaces the older one (the old one to the sync trash), an older one is kept
//!   out (most recently edited wins, SY4);
//! * a received pack that changed under a local draft makes that draft report `baseChanged` (the 10.1 prompt "This
//!   element changed since you started editing").
#![cfg(feature = "device-link")]

use borealis_lib::content_dto::{ContentProgress, PackKind};
use borealis_lib::content_packs::{EventSink, NullSink};
use borealis_lib::device_sync::{DeviceSync, StateRef, SyncConfig, SyncEvents};
use borealis_lib::dto::Ruleset;
use borealis_lib::homebrew_dto::HomebrewPackRequest;
use borealis_lib::picked_paths::PickPurpose;
use borealis_lib::state::{AppPaths, AppState};
use borealis_lib::sync_dto::{BundlePreview, LinkPackChoice, LinkSelection, LinkStatus};
use borealis_link::{MemoryStore, SecureStore};
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

const PACK: &str = "homebrew-ei";
const FIREBALL: &str = "ID_2024_SPELL_FIREBALL";

struct Quiet;
impl SyncEvents for Quiet {
    fn status(&self, _: &LinkStatus) {}
    fn content(&self) -> &dyn EventSink {
        &NullSink
    }
}

struct Sink;
impl EventSink for Sink {
    fn content_progress(&self, _: &ContentProgress) {}
    fn import_progress(&self, _: &ContentProgress) {}
    fn content_reloaded(&self, _: &str) {}
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

impl Dev {
    fn pack_dir(&self) -> PathBuf {
        self.data.path().join("library").join(PACK)
    }

    fn file(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.pack_dir().join(name)).unwrap()
    }

    /// Every file of the pack edited `age` ago (negative: in the future), so the newest edit is known.
    fn edited(&self, age: i64) {
        let when = if age >= 0 {
            SystemTime::now() - Duration::from_secs(age.unsigned_abs())
        } else {
            SystemTime::now() + Duration::from_secs(age.unsigned_abs())
        };
        for e in std::fs::read_dir(self.pack_dir()).unwrap() {
            let path = e.unwrap().path();
            if path.is_file() {
                std::fs::File::options()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_modified(when)
                    .unwrap();
            }
        }
    }

    /// The element's draft with the name changed (a written draft, HC7: it stays here).
    fn draft_rename(&self, id: &str, name: &str) -> String {
        let v = self
            .state
            .homebrew_open_draft(PACK, Some(id), None)
            .unwrap();
        let mut form = v.form.clone();
        form.fields.insert("name".into(), json!(name));
        self.state
            .homebrew_update_draft(&v.draft_id, &form)
            .unwrap();
        v.draft_id
    }

    fn rename_and_save(&self, id: &str, name: &str) {
        let draft = self.draft_rename(id, name);
        self.state.homebrew_save_draft(&draft, &Sink).unwrap();
    }

    /// Send every user pack to `to` as a bundle file; the preview it opens.
    fn bundle_to(&self, to: &Dev, files: &Path, name: &str) -> BundlePreview {
        let built = self
            .sync
            .build_bundle_file(&LinkSelection {
                character_ids: Vec::new(),
                packs: LinkPackChoice::All,
            })
            .unwrap();
        let dest = files.join(format!("{name}.borealis"));
        DeviceSync::save_bundle(&built, &dest).unwrap();
        let token = to
            .state
            .picked
            .grant(dest, PickPurpose::BundleFile)
            .unwrap()
            .token;
        to.sync.open_bundle(&token).unwrap()
    }
}

fn with_pack(name: &str) -> (Dev, String) {
    let d = dev(name);
    d.state
        .homebrew_create_pack(
            &HomebrewPackRequest {
                name: "Ember Isles".into(),
                code: "EI".into(),
                edition: Ruleset::Dnd2024,
                author: None,
                description: None,
                license: None,
            },
            &Sink,
        )
        .unwrap();
    let v = d
        .state
        .homebrew_new_draft(PACK, "spell", Some(FIREBALL), None, Some(false))
        .unwrap();
    let id = d
        .state
        .homebrew_save_draft(&v.draft_id, &Sink)
        .unwrap()
        .element_ids[0]
        .clone();
    (d, id)
}

#[test]
fn a_homebrew_pack_travels_with_creator_json_and_without_drafts() {
    let (a, spell) = with_pack("Mac");
    a.edited(60);
    // an edited draft on the sender stays there
    a.draft_rename(&spell, "Not sent");
    let b = dev("Phone");
    let files = tempfile::tempdir().unwrap();
    let preview = a.bundle_to(&b, files.path(), "first");
    let rows = &preview.preview.items;
    assert_eq!(rows.len(), 1, "the pack is one item: {rows:?}");
    assert_eq!(rows[0].action, "new");
    b.sync.accept_bundle(&preview.bundle_id).unwrap();

    assert_eq!(b.file("creator.json"), a.file("creator.json"));
    assert_eq!(b.file("content.json"), a.file("content.json"));
    let pack = b
        .state
        .list_packs()
        .unwrap()
        .into_iter()
        .find(|p| p.id == PACK)
        .expect("received");
    assert_eq!(pack.kind, PackKind::Homebrew, "editable on the phone");
    let rows = b.state.homebrew_list_elements(PACK).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].has_draft, "the sender's draft never travels");
    assert!(b.state.homebrew_list_drafts().unwrap().is_empty());
    assert!(!b.data.path().join("homebrew-drafts").exists());
    let v = b
        .state
        .homebrew_open_draft(PACK, Some(&spell), None)
        .unwrap();
    assert_eq!(v.form.fields["name"], json!("Fireball (copy)"));
}

#[test]
fn a_pack_received_under_a_draft_asks_which_to_keep_and_the_newest_edit_wins() {
    let (a, spell) = with_pack("Mac");
    a.edited(3600);
    let b = dev("Phone");
    let files = tempfile::tempdir().unwrap();
    let first = a.bundle_to(&b, files.path(), "first");
    b.sync.accept_bundle(&first.bundle_id).unwrap();

    // the phone starts editing the spell; the Mac edits and saves it later
    let draft = b.draft_rename(&spell, "Phone draft");
    let fresh = b
        .state
        .homebrew_open_draft(PACK, Some(&spell), None)
        .unwrap();
    assert!(!fresh.base_changed);
    a.rename_and_save(&spell, "Emberstorm");
    a.edited(10);
    let second = a.bundle_to(&b, files.path(), "second");
    assert_eq!(second.preview.items.len(), 1);
    assert_eq!(second.preview.items[0].action, "replace");
    let summary = b.sync.accept_bundle(&second.bundle_id).unwrap();
    assert_eq!(summary.replaced, 1);
    assert_eq!(b.file("creator.json"), a.file("creator.json"));
    let trash = b.data.path().join("sync").join("trash");
    assert_eq!(
        std::fs::read_dir(trash).unwrap().count(),
        1,
        "old copy kept"
    );

    // the draft stayed on the phone, and now reports that its element changed (spec 10.1 prompt)
    let v = b
        .state
        .homebrew_open_draft(PACK, Some(&spell), None)
        .unwrap();
    assert_eq!(v.draft_id, draft);
    assert_eq!(v.form.fields["name"], json!("Phone draft"));
    assert!(v.base_changed, "Keep my draft / Use the saved version");
    // "Use the saved version": the received element
    b.state.homebrew_discard_draft(&draft).unwrap();
    let saved = b
        .state
        .homebrew_open_draft(PACK, Some(&spell), None)
        .unwrap();
    assert_eq!(saved.form.fields["name"], json!("Emberstorm"));
    assert!(!saved.base_changed);

    // the phone saves an edit after the Mac's: an older bundle of the pack is kept out
    b.rename_and_save(&spell, "Phone edit");
    b.edited(-5);
    let stale = a.bundle_to(&b, files.path(), "stale");
    assert_eq!(stale.preview.items.len(), 1);
    assert_eq!(stale.preview.items[0].action, "keep_local");
    let summary = b.sync.accept_bundle(&stale.bundle_id).unwrap();
    assert_eq!((summary.kept_local, summary.replaced), (1, 0));
    let v = b
        .state
        .homebrew_open_draft(PACK, Some(&spell), None)
        .unwrap();
    assert_eq!(v.form.fields["name"], json!("Phone edit"));
}

#[test]
fn a_pack_s_overrides_travel_in_their_own_file() {
    // HC20 (spec 22.2): the overrides file goes with the pack and the override applies on the other device
    let (a, _) = with_pack("Mac");
    let v = a.state.homebrew_new_override(PACK, FIREBALL).unwrap();
    let mut form = v.form.clone();
    form.fields.insert("range".into(), json!("300 feet"));
    let v = a.state.homebrew_update_draft(&v.draft_id, &form).unwrap();
    a.state.homebrew_save_draft(&v.draft_id, &Sink).unwrap();
    a.edited(60);
    let b = dev("Phone");
    let files = tempfile::tempdir().unwrap();
    let preview = a.bundle_to(&b, files.path(), "overrides");
    b.sync.accept_bundle(&preview.bundle_id).unwrap();
    let name = borealis_content::OVERRIDES_FILE;
    assert_eq!(b.file(name), a.file(name));
    let hub = b.state.hub().unwrap();
    assert_eq!(
        hub.store.element(FIREBALL).unwrap().setter_value("range"),
        Some("300 feet")
    );
    let rows = b.state.homebrew_list_elements(PACK).unwrap();
    assert!(rows
        .iter()
        .any(|r| r.id == FIREBALL && r.override_info.is_some()));
}
