//! A desktop peer on the real network for checking device transfer by hand against a phone,
//! simulator or emulator (no desktop GUI needed). Ignored by default; run one side at a time:
//!
//! ```text
//! BOREALIS_PEER=receive [BOREALIS_PEER_DECIDE=decline] \
//!   cargo test -p borealis --test lan_peer -- --ignored --nocapture
//! BOREALIS_PEER=send cargo test -p borealis --test lan_peer -- --ignored --nocapture
//! BOREALIS_PEER=dial-send|dial-receive BOREALIS_PEER_CODE=42-ember-lantern \
//!   BOREALIS_PEER_HOST=127.0.0.1:41234 cargo test -p borealis --test lan_peer -- --ignored --nocapture
//! ```
//!
//! The peer binds this computer's LAN address (as the app does), prints the typed code and the
//! address to enter on the other device, and waits up to five minutes. `receive` previews the
//! offer, accepts it (or declines with `BOREALIS_PEER_DECIDE=decline`) and prints the received
//! character names; `send` offers one synthetic character, "Lan Peer Hero". `dial-send` and
//! `dial-receive` type a code shown on the other device instead (an emulator is reached through
//! `adb emu redir add tcp:<port>:<port>` and a loopback address, which SY8 counts as local).
//!
//! Optional: `BOREALIS_PEER_DATA=<dir>` keeps the peer's characters and packs between runs (a
//! "latest edit wins" or "packs travel" check needs the same copy twice); with it, a send offers
//! every character in that folder with the packs they use. `BOREALIS_PEER_EDIT=<text>` edits
//! "Lan Peer Hero" (its player name) before sending, so the peer's copy is the newer one. The
//! device-sync handoff for the simulator/emulator run lists the steps on the phone side.
#![cfg(feature = "device-link")]

use std::path::{Path, PathBuf};
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

fn peer(data: &Path) -> (Arc<AppState>, Arc<DeviceSync>) {
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

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
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
        "PEER-CODE {} PEER-ADDRESS {} PEER-FINGERPRINT {}",
        s.code.as_deref().unwrap_or("-"),
        s.address.as_deref().unwrap_or("-"),
        s.fingerprint.as_deref().unwrap_or("-")
    );
}

fn character_files(state: &AppState) -> Vec<PathBuf> {
    std::fs::read_dir(state.characters_dir())
        .map(|dir| {
            dir.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default()
}

/// Prints the peer's characters (name and player) and the user packs in its library.
fn report(state: &AppState) {
    let names: Vec<String> = character_files(state)
        .iter()
        .filter_map(|p| Character::load(p).ok())
        .map(|c| format!("{} (player {:?})", c.meta.name, c.meta.player))
        .collect();
    println!("PEER-CHARACTERS {names:?}");
    let packs: Vec<String> = std::fs::read_dir(state.paths.library())
        .map(|dir| {
            dir.filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    println!("PEER-PACKS {packs:?}");
}

/// Writes "Lan Peer Hero" when missing, applies `BOREALIS_PEER_EDIT`, answers what to send.
fn outgoing(state: &AppState) -> LinkSelection {
    let dir = state.characters_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let hero = dir.join(format!("{HERO_ID}.json"));
    let edit = env("BOREALIS_PEER_EDIT");
    if !hero.exists() || edit.is_some() {
        let mut c = Character::load(&hero).unwrap_or_else(|_| Character::new(HERO_NAME));
        c.meta.id = HERO_ID.into();
        if let Some(player) = edit {
            c.meta.player = player;
        }
        c.save(&hero).unwrap();
    }
    let character_ids = character_files(state)
        .iter()
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    LinkSelection {
        character_ids,
        packs: LinkPackChoice::Required,
    }
}

/// Receiver: wait for the preview, print it, accept or decline, print the result.
async fn decide(state: &AppState, sync: &Arc<DeviceSync>, id: &str, accept: bool) {
    let s = until(sync, id, &[LinkPhase::Preview]).await;
    let rows = s.preview.map(|p| p.items).unwrap_or_default();
    for r in &rows {
        println!("PEER-PREVIEW {} {} {}", r.kind, r.label, r.action);
    }
    if s.phase != LinkPhase::Preview {
        println!("PEER-RESULT {:?} {:?}", s.phase, s.error);
        return;
    }
    sync.decide(id, accept).unwrap();
    let end = until(sync, id, &[LinkPhase::Done]).await;
    println!("PEER-RESULT {:?} {:?}", end.phase, end.summary);
    report(state);
}

async fn finish(sync: &DeviceSync, id: &str) {
    let end = until(sync, id, &[LinkPhase::Done]).await;
    println!(
        "PEER-RESULT {:?} {:?} {:?}",
        end.phase, end.summary, end.error
    );
}

async fn receive(state: &AppState, sync: &Arc<DeviceSync>, accept: bool) {
    let shown = sync.start_receive().await.unwrap();
    show(&shown);
    decide(state, sync, &shown.session_id, accept).await;
}

async fn send(state: &AppState, sync: &Arc<DeviceSync>) {
    let selection = outgoing(state);
    report(state);
    let shown = sync.start_send(selection).await.unwrap();
    show(&shown);
    finish(sync, &shown.session_id).await;
}

/// Type a code shown on the other device (`BOREALIS_PEER_CODE` at `BOREALIS_PEER_HOST`).
async fn dial(state: &AppState, sync: &Arc<DeviceSync>, sending: bool, accept: bool) {
    let code = env("BOREALIS_PEER_CODE").expect("BOREALIS_PEER_CODE");
    let host = env("BOREALIS_PEER_HOST").expect("BOREALIS_PEER_HOST");
    let selection = sending.then(|| outgoing(state));
    report(state);
    match sync.connect(&code, Some(&host), selection).await {
        Err(e) => println!("PEER-RESULT refused {e:?}"),
        Ok(s) if sending => finish(sync, &s.session_id).await,
        Ok(s) => decide(state, sync, &s.session_id, accept).await,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "manual: a desktop peer for a phone on the same network (BOREALIS_PEER=receive|send)"]
async fn desktop_peer() {
    let mode = std::env::var("BOREALIS_PEER").unwrap_or_default();
    let accept = std::env::var("BOREALIS_PEER_DECIDE").as_deref() != Ok("decline");
    let temp = tempfile::tempdir().unwrap();
    let data = env("BOREALIS_PEER_DATA").map_or_else(|| temp.path().to_path_buf(), PathBuf::from);
    std::fs::create_dir_all(&data).unwrap();
    let (state, sync) = peer(&data);
    match mode.as_str() {
        "receive" => receive(&state, &sync, accept).await,
        "send" => send(&state, &sync).await,
        "dial-send" => dial(&state, &sync, true, accept).await,
        "dial-receive" => dial(&state, &sync, false, accept).await,
        other => {
            panic!("BOREALIS_PEER must be receive, send, dial-send or dial-receive: {other:?}")
        }
    }
}
