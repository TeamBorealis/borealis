//! A desktop peer on the real network for checking device transfer by hand against a phone,
//! simulator or emulator (no desktop GUI needed). Ignored by default; run one side at a time:
//!
//! ```text
//! BOREALIS_PEER=receive [BOREALIS_PEER_DECIDE=decline] \
//!   cargo test -p borealis --test lan_peer -- --ignored --nocapture
//! BOREALIS_PEER=send cargo test -p borealis --test lan_peer -- --ignored --nocapture
//! ```
//!
//! The peer binds this computer's LAN address (as the app does), prints the typed code and the
//! address to enter on the other device, and waits up to five minutes. `receive` previews the
//! offer, accepts it (or declines with `BOREALIS_PEER_DECIDE=decline`) and prints the received
//! character names; `send` offers one synthetic character, "Lan Peer Hero". The device-sync
//! handoff for the simulator/emulator run lists the steps on the phone side.
#![cfg(feature = "device-link")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use borealis_character::Character;
use borealis_lib::content_packs::{EventSink, NullSink};
use borealis_lib::device_sync::{DeviceSync, StateRef, SyncConfig, SyncEvents};
use borealis_lib::state::{AppPaths, AppState};
use borealis_lib::sync_dto::{LinkPackChoice, LinkPhase, LinkSelection, LinkStatus};
use borealis_link::{MemoryStore, SecureStore};

const WAIT: Duration = Duration::from_secs(300);
const POLL: Duration = Duration::from_millis(100);
const HERO_ID: &str = "c_1a2b3c4d5e6f708192a3b4c5d6e7f809";
const HERO_NAME: &str = "Lan Peer Hero";

struct Quiet;

impl SyncEvents for Quiet {
    fn status(&self, _: &LinkStatus) {}
    fn content(&self) -> &dyn EventSink {
        &NullSink
    }
}

fn peer(data: &std::path::Path) -> (Arc<AppState>, Arc<DeviceSync>) {
    let paths = AppPaths {
        content_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../content"),
        data: data.to_path_buf(),
    };
    let state = Arc::new(AppState::new(paths).unwrap());
    let store = Arc::new(MemoryStore::default());
    let config = SyncConfig {
        bind_ip: None,
        device_name: "Borealis LAN peer".into(),
        discovery: false,
    };
    let sync = DeviceSync::new(
        Arc::clone(&state) as Arc<dyn StateRef>,
        Arc::new(Quiet),
        config,
        move || Ok(Arc::clone(&store) as Arc<dyn SecureStore>),
    );
    (state, Arc::new(sync))
}

/// Wait until the session leaves the phases before `want`; answers the final status.
async fn until(sync: &DeviceSync, id: &str, want: &[LinkPhase]) -> LinkStatus {
    let start = Instant::now();
    loop {
        let s = sync.status(id).unwrap();
        let ended = matches!(
            s.phase,
            LinkPhase::Done
                | LinkPhase::Failed
                | LinkPhase::Cancelled
                | LinkPhase::Declined
                | LinkPhase::Expired
        );
        if want.contains(&s.phase) || ended {
            return s;
        }
        assert!(start.elapsed() < WAIT, "timed out at {:?}", s.phase);
        tokio::time::sleep(POLL).await;
    }
}

fn show(s: &LinkStatus) {
    println!(
        "PEER-CODE {} PEER-ADDRESS {}",
        s.code.as_deref().unwrap_or("-"),
        s.address.as_deref().unwrap_or("-")
    );
}

async fn receive(state: &AppState, sync: &Arc<DeviceSync>, accept: bool) {
    let shown = sync.start_receive().await.unwrap();
    show(&shown);
    let s = until(sync, &shown.session_id, &[LinkPhase::Preview]).await;
    let rows = s.preview.map(|p| p.items).unwrap_or_default();
    for r in &rows {
        println!("PEER-PREVIEW {} {} {}", r.kind, r.label, r.action);
    }
    if s.phase != LinkPhase::Preview {
        println!("PEER-RESULT {:?} {:?}", s.phase, s.error);
        return;
    }
    sync.decide(&shown.session_id, accept).unwrap();
    let end = until(sync, &shown.session_id, &[LinkPhase::Done]).await;
    println!("PEER-RESULT {:?} {:?}", end.phase, end.summary);
    let names: Vec<String> = std::fs::read_dir(state.characters_dir())
        .map(|dir| {
            dir.filter_map(Result::ok)
                .filter_map(|e| Character::load(&e.path()).ok())
                .map(|c| c.meta.name)
                .collect()
        })
        .unwrap_or_default();
    println!("PEER-CHARACTERS {names:?}");
}

async fn send(state: &AppState, sync: &Arc<DeviceSync>) {
    let mut c = Character::new(HERO_NAME);
    c.meta.id = HERO_ID.into();
    let dir = state.characters_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{HERO_ID}.json")),
        c.to_json_string().unwrap(),
    )
    .unwrap();
    let selection = LinkSelection {
        character_ids: vec![HERO_ID.into()],
        packs: LinkPackChoice::Required,
    };
    let shown = sync.start_send(selection).await.unwrap();
    show(&shown);
    let end = until(sync, &shown.session_id, &[LinkPhase::Done]).await;
    println!(
        "PEER-RESULT {:?} {:?} {:?}",
        end.phase, end.summary, end.error
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "manual: a desktop peer for a phone on the same network (BOREALIS_PEER=receive|send)"]
async fn desktop_peer() {
    let mode = std::env::var("BOREALIS_PEER").unwrap_or_default();
    let accept = std::env::var("BOREALIS_PEER_DECIDE").as_deref() != Ok("decline");
    let data = tempfile::tempdir().unwrap();
    let (state, sync) = peer(data.path());
    match mode.as_str() {
        "receive" => receive(&state, &sync, accept).await,
        "send" => send(&state, &sync).await,
        other => panic!("BOREALIS_PEER must be receive or send, not {other:?}"),
    }
}
