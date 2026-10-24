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
    dev_on(name, IpAddr::V4(Ipv4Addr::LOCALHOST))
}

fn dev_on(name: &str, bind_ip: IpAddr) -> Dev {
    let data = tempfile::tempdir().unwrap();
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.path().to_path_buf(),
    };
    let state = Arc::new(AppState::new(paths).unwrap());
    let store = Arc::new(MemoryStore::default());
    let s = Arc::clone(&store);
    let config = SyncConfig {
        bind_ip: Some(bind_ip),
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

/// Z: Accept counts only while a preview is shown. An Accept sent before the offer arrived is
/// refused and does not pre-approve the offer: the preview still waits for the user.
#[tokio::test(flavor = "multi_thread")]
async fn an_accept_before_the_preview_is_refused_and_approves_nothing() {
    let (pc, phone) = (dev("PC"), dev("Phone"));
    put_character(&phone, "Tam");
    let shown = pc.sync.start_receive().await.unwrap();
    assert!(pc.sync.decide(&shown.session_id, true).is_err());
    phone
        .sync
        .connect(
            shown.qr_payload.as_deref().unwrap(),
            None,
            Some(selection()),
        )
        .await
        .unwrap();
    wait(&pc, &shown.session_id, LinkPhase::Preview).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let still = pc.sync.status(&shown.session_id).unwrap();
    assert_eq!(still.phase, LinkPhase::Preview, "nothing was applied");
    assert!(pc.sync.decide(&shown.session_id, false).is_ok());
    wait(&pc, &shown.session_id, LinkPhase::Declined).await;
}

/// Z / SY1 "same network only": a QR, a typed address or this device's own window address
/// outside the local network is refused with a plain message before anything connects.
#[tokio::test(flavor = "multi_thread")]
async fn addresses_outside_the_local_network_are_refused() {
    const MSG: &str = "Borealis only connects to devices on the same network.";
    let (pc, phone) = (dev("PC"), dev("Phone"));
    put_character(&phone, "Tam");
    let shown = pc.sync.start_receive().await.unwrap();
    let qr = shown.qr_payload.clone().unwrap();
    assert!(qr.contains("h=127.0.0.1:"), "{qr}");
    for far in ["8.8.8.8", "100.64.0.1", "[2001:db8::1]"] {
        let evil = qr.replacen("127.0.0.1", far, 1);
        let err = phone
            .sync
            .connect(&evil, None, Some(selection()))
            .await
            .unwrap_err();
        assert_eq!(err.message, MSG, "{far}");
    }
    let code = shown.code.clone().unwrap();
    for far in [
        "8.8.8.8:41234",
        "[2606:4700::1111]:41234",
        "[::ffff:1.1.1.1]:5",
    ] {
        let err = phone
            .sync
            .connect(&code, Some(far), Some(selection()))
            .await
            .unwrap_err();
        assert_eq!(err.message, MSG, "{far}");
    }
    let err = phone
        .sync
        .connect(&code, Some("router.example:41234"), Some(selection()))
        .await
        .unwrap_err();
    assert!(
        err.message.contains("Enter the address as shown"),
        "{}",
        err.message
    );
    pc.sync.cancel(&shown.session_id).unwrap();
    wait(&pc, &shown.session_id, LinkPhase::Cancelled).await;

    let public = dev_on("Public PC", IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)));
    let err = public.sync.start_receive().await.unwrap_err();
    assert_eq!(err.message, MSG);
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

/// Settings > Devices (C1): both sides list each other after a transfer (name, platform,
/// last synced, 180-day expiry); a second transfer keeps the pairing date; Remove deletes the
/// registry entry and the pair key; an expired pairing is dropped when the list is read.
#[tokio::test(flavor = "multi_thread")]
async fn devices_lists_paired_peers_and_remove_forgets_them() {
    let (pc, phone) = (dev("PC"), dev("Phone"));
    listener_sends(&pc, &phone).await;
    let listed = pc.sync.devices().unwrap();
    assert_eq!(listed.len(), 1);
    let d = listed[0].clone();
    assert_eq!(d.name, "Phone");
    assert_eq!(d.platform, borealis_link::Platform::current());
    assert_eq!(d.expires_at, d.last_synced_at + 180 * 86_400);
    let value = serde_json::to_value(&listed).unwrap();
    let errs = contract_support::Contract::load().def("LinkDevice", &value[0]);
    assert!(errs.is_empty(), "{errs:?}\n{value:#}");

    // A second Send (same character, now identical) keeps the original pairing date.
    let shown = pc.sync.start_send(selection()).await.unwrap();
    let scan = phone
        .sync
        .connect(shown.qr_payload.as_deref().unwrap(), None, None)
        .await
        .unwrap();
    wait(&phone, &scan.session_id, LinkPhase::Preview).await;
    phone.sync.decide(&scan.session_id, true).unwrap();
    wait(&pc, &shown.session_id, LinkPhase::Done).await;
    let again = pc.sync.devices().unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].paired_at, d.paired_at);

    let id = DeviceId::parse(&d.id).unwrap();
    let key = |dev: &Dev| borealis_link::store::peer_token(dev.store.as_ref(), &id).unwrap();
    assert!(key(&pc).is_some());
    assert!(pc.sync.forget_device(&d.id).unwrap().is_empty());
    assert!(borealis_link::DeviceRegistry::load(&pc.state.paths.data)
        .unwrap()
        .get(&id)
        .is_none());
    assert!(key(&pc).is_none(), "the removed device's key is deleted");
    // Removing again is a no-op; a malformed id is refused.
    assert!(pc.sync.forget_device(&d.id).unwrap().is_empty());
    assert!(pc.sync.forget_device("../etc").is_err());
    // The phone still lists the PC until it is removed there (no ongoing sync in stage 1).
    assert_eq!(phone.sync.devices().unwrap().len(), 1);

    // A pairing past its expiry disappears from the list (and the file) when read.
    let data = &phone.state.paths.data;
    let mut reg = borealis_link::DeviceRegistry::load(data).unwrap();
    for paired in &mut reg.devices {
        paired.paired_at = 1;
        paired.last_seen = 1;
        paired.expires_at = 2;
    }
    reg.save(data).unwrap();
    assert!(phone.sync.devices().unwrap().is_empty());
    assert!(borealis_link::DeviceRegistry::load(data)
        .unwrap()
        .devices
        .is_empty());
}
