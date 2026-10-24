//! B2/B3 through the app service: two app states on localhost play PC and phone. Covers the four
//! directions (PC -> phone and phone -> PC by QR, phone -> phone by QR, PC -> PC by typed code),
//! pairing memory (registry + key in the secure store), a wrong typed code, and an invalid
//! character refused by the real character parser.
#![cfg(feature = "device-link")]

mod contract_support;

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use borealis_character::Character;
use borealis_lib::content_packs::{EventSink, NullSink};
use borealis_lib::device_sync::{DeviceSync, StateRef, SyncConfig, SyncEvents};
use borealis_lib::state::{AppPaths, AppState};
use borealis_lib::sync_dto::{LinkPackChoice, LinkPhase, LinkSelection, LinkStatus};
use borealis_link::{DeviceId, MemoryStore, SecureStore};

#[derive(Default)]
struct Rec(Mutex<Vec<LinkStatus>>);

impl SyncEvents for Rec {
    fn status(&self, status: &LinkStatus) {
        self.0.lock().unwrap().push(status.clone());
    }
    fn content(&self) -> &dyn EventSink {
        &NullSink
    }
}

struct Dev {
    _data: tempfile::TempDir,
    state: Arc<AppState>,
    sync: Arc<DeviceSync>,
    store: Arc<MemoryStore>,
}

fn dev(name: &str) -> Dev {
    let data = tempfile::tempdir().unwrap();
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    };
    let state = Arc::new(AppState::new(paths).unwrap());
    let store = Arc::new(MemoryStore::default());
    let s = Arc::clone(&store);
    let config = SyncConfig {
        bind_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        device_name: name.into(),
        discovery: false,
    };
    let sync = DeviceSync::new(
        Arc::clone(&state) as Arc<dyn StateRef>,
        Arc::new(Rec::default()),
        config,
        move || Ok(Arc::clone(&s) as Arc<dyn SecureStore>),
    );
    Dev {
        _data: data,
        state,
        sync: Arc::new(sync),
        store,
    }
}

const ID: &str = "c_0123456789abcdef0123456789abcdef";

fn put_character(d: &Dev, name: &str) {
    let mut c = Character::new(name);
    c.meta.id = ID.into();
    let dir = d.state.characters_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{ID}.json")), c.to_json_string().unwrap()).unwrap();
}

fn selection() -> LinkSelection {
    LinkSelection {
        character_ids: vec![ID.into()],
        packs: LinkPackChoice::Required,
    }
}

async fn wait(d: &Dev, id: &str, want: LinkPhase) -> LinkStatus {
    let start = Instant::now();
    loop {
        let s = d.sync.status(id).unwrap();
        if s.phase == want {
            return s;
        }
        assert!(
            !matches!(
                s.phase,
                LinkPhase::Failed | LinkPhase::Cancelled | LinkPhase::Declined
            ) || want == s.phase,
            "{id}: wanted {want:?}, got {s:?}"
        );
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "{id}: timed out waiting for {want:?}, at {s:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Every status the UI sees matches the TS `LinkStatus` type in the generated contract.
fn assert_contract(status: &LinkStatus) {
    let value = serde_json::to_value(status).unwrap();
    let errs = contract_support::Contract::load().def("LinkStatus", &value);
    assert!(errs.is_empty(), "{errs:?}\n{value:#}");
}

fn received(d: &Dev) -> Character {
    Character::load(&d.state.characters_dir().join(format!("{ID}.json"))).unwrap()
}

fn assert_paired(a: &Dev, b: &Dev) {
    let reg = borealis_link::DeviceRegistry::load(&a.state.paths.data).unwrap();
    assert_eq!(reg.devices.len(), 1, "one paired device remembered");
    let peer = &reg.devices[0];
    let key = borealis_link::store::peer_token(a.store.as_ref(), &DeviceId::of(&peer.fingerprint))
        .unwrap();
    assert!(key.is_some(), "pair key in the secure store");
    let _ = b;
}

/// The listener sends; the other device scans the QR and receives.
async fn listener_sends(sender: &Dev, receiver: &Dev) {
    put_character(sender, "Mira");
    let shown = sender.sync.start_send(selection()).await.unwrap();
    assert_eq!(shown.phase, LinkPhase::Waiting);
    let qr = shown.qr_payload.clone().unwrap();
    assert!(qr.starts_with("borealis-link:v1?") && qr.ends_with("&m=send"));
    let scan = receiver.sync.connect(&qr, None, None).await.unwrap();
    let preview = wait(receiver, &scan.session_id, LinkPhase::Preview).await;
    assert_contract(&shown);
    assert_contract(&preview);
    let rows = preview.preview.unwrap().items;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].label.as_str(), rows[0].action.as_str()),
        ("Mira", "new")
    );
    receiver.sync.decide(&scan.session_id, true).unwrap();
    let done = wait(receiver, &scan.session_id, LinkPhase::Done).await;
    assert_contract(&done);
    assert_eq!(done.summary.unwrap().added, 1);
    wait(sender, &shown.session_id, LinkPhase::Done).await;
    assert_eq!(received(receiver).meta.name, "Mira");
    assert_paired(receiver, sender);
    assert_paired(sender, receiver);
}

/// The listener receives; the other device scans (or types) and sends.
async fn listener_receives(receiver: &Dev, sender: &Dev, typed: bool) {
    put_character(sender, "Tam");
    let shown = receiver.sync.start_receive().await.unwrap();
    let (target, host) = if typed {
        (shown.code.clone().unwrap(), shown.address.clone())
    } else {
        (shown.qr_payload.clone().unwrap(), None)
    };
    let dial = sender
        .sync
        .connect(&target, host.as_deref(), Some(selection()))
        .await
        .unwrap();
    wait(receiver, &shown.session_id, LinkPhase::Preview).await;
    receiver.sync.decide(&shown.session_id, true).unwrap();
    wait(receiver, &shown.session_id, LinkPhase::Done).await;
    let sent = wait(sender, &dial.session_id, LinkPhase::Done).await;
    assert_eq!(sent.summary.unwrap().added, 1);
    assert_eq!(received(receiver).meta.name, "Tam");
    assert_paired(receiver, sender);
}

#[tokio::test(flavor = "multi_thread")]
async fn pc_to_phone_by_qr() {
    listener_sends(&dev("PC"), &dev("Phone")).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn phone_to_pc_by_qr() {
    listener_receives(&dev("PC"), &dev("Phone"), false).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn phone_to_phone_by_qr() {
    listener_receives(&dev("Phone A"), &dev("Phone B"), false).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pc_to_pc_by_typed_code() {
    listener_receives(&dev("PC 1"), &dev("PC 2"), true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_typed_code_fails_both_sides_and_kills_the_code() {
    let (pc1, pc2) = (dev("PC 1"), dev("PC 2"));
    put_character(&pc2, "Tam");
    let shown = pc1.sync.start_receive().await.unwrap();
    let code = shown.code.clone().unwrap();
    let nameplate = code.split('-').next().unwrap();
    let wrong = if code.ends_with("-oak") {
        format!("{nameplate}-oak-owl")
    } else {
        format!("{nameplate}-oak-oak")
    };
    let dial = pc2
        .sync
        .connect(&wrong, shown.address.as_deref(), Some(selection()))
        .await
        .unwrap();
    let failed = wait(&pc2, &dial.session_id, LinkPhase::Failed).await;
    assert_contract(&failed);
    assert_eq!(failed.error_code.as_deref(), Some("wrong_code"));
    let closed = wait(&pc1, &shown.session_id, LinkPhase::Failed).await;
    assert_eq!(closed.error_code.as_deref(), Some("wrong_code"));
    assert!(closed.code.is_none() && closed.qr_payload.is_none());
    // The right code no longer works: the listener is gone.
    let retry = pc2
        .sync
        .connect(&code, shown.address.as_deref(), Some(selection()))
        .await
        .unwrap();
    wait(&pc2, &retry.session_id, LinkPhase::Failed).await;
    assert!(!pc1
        .state
        .characters_dir()
        .join(format!("{ID}.json"))
        .exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_character_the_parser_rejects_is_refused() {
    let (phone, pc) = (dev("Phone"), dev("PC"));
    let dir = phone.state.characters_dir();
    std::fs::create_dir_all(&dir).unwrap();
    // Valid JSON object (so the sender can bundle it), but not a Borealis character.
    std::fs::write(
        dir.join(format!("{ID}.json")),
        r#"{"format":"not-a-character"}"#,
    )
    .unwrap();
    let shown = pc.sync.start_receive().await.unwrap();
    let dial = phone
        .sync
        .connect(
            shown.qr_payload.as_deref().unwrap(),
            None,
            Some(selection()),
        )
        .await
        .unwrap();
    wait(&pc, &shown.session_id, LinkPhase::Preview).await;
    pc.sync.decide(&shown.session_id, true).unwrap();
    let refused = wait(&pc, &shown.session_id, LinkPhase::Failed).await;
    assert_eq!(
        refused.error_code.as_deref(),
        Some("refused"),
        "{refused:?}"
    );
    wait(&phone, &dial.session_id, LinkPhase::Failed).await;
    assert!(!pc
        .state
        .characters_dir()
        .join(format!("{ID}.json"))
        .exists());
    let incoming = pc.state.paths.data.join("sync/incoming");
    assert_eq!(
        std::fs::read_dir(incoming).map(|d| d.count()).unwrap_or(0),
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scanned_qr_must_match_the_chosen_direction() {
    let (pc, phone) = (dev("PC"), dev("Phone"));
    let shown = pc.sync.start_receive().await.unwrap();
    let err = phone
        .sync
        .connect(shown.qr_payload.as_deref().unwrap(), None, None)
        .await
        .unwrap_err();
    assert!(err.message.contains("Send"), "{}", err.message);
    pc.sync.cancel(&shown.session_id).unwrap();
    wait(&pc, &shown.session_id, LinkPhase::Cancelled).await;
}
