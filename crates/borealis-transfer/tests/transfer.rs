//! B3 one-shot transfer over loopback TLS: preview + accept, a 50 MB item, decline, cancel on
//! either side mid-stream (no partial files), and hostile senders (bad checksum, oversize, unsafe
//! names, invalid characters, a bundle that differs from the offer).

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use borealis_link::Connection;
use borealis_transfer::wire::{control, Control, Role, PROTOCOL, TAG_CHUNK};
use borealis_transfer::{
    receive, send, Cancel, Event, Outgoing, PreviewAction, TransferError, TransferLimits,
};
use common::{bundle_file, character, local, pair, Device, Recorder};
use tokio::sync::oneshot;

fn pack_files(big: usize) -> Vec<(String, Vec<u8>)> {
    // Incompressible filler so the zip really carries `big` bytes.
    let mut x: u32 = 0x9e37_79b9;
    let filler: Vec<u8> = (0..big)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x.to_le_bytes()[0]
        })
        .collect();
    vec![
        ("pack.json".into(), br#"{"id":"homebrew"}"#.to_vec()),
        ("files/map.bin".into(), filler),
    ]
}

struct Run {
    sent: Result<borealis_transfer::Summary, TransferError>,
    got: Result<borealis_transfer::Summary, TransferError>,
    receiver: Device,
    events: Arc<Recorder>,
}

async fn run(
    chars: Vec<(&'static str, Vec<u8>)>,
    big: Option<usize>,
    accept: bool,
    sender_cancel: Cancel,
    receiver_cancel: Cancel,
    events: Arc<Recorder>,
) -> Run {
    let src = tempfile::tempdir().unwrap();
    let packs = big
        .map(|n| vec![("homebrew", pack_files(n))])
        .unwrap_or_default();
    let (path, manifest) = bundle_file(src.path(), &chars, &packs);
    let (a, b) = pair().await;
    let receiver = Device::new();
    let (tx, rx) = oneshot::channel();
    let ev = Arc::clone(&events);
    let limits = TransferLimits::default();
    let labels: BTreeMap<String, String> = [("c_one".to_string(), "Mira".to_string())].into();
    let sender = async {
        // Owned here so the connection closes as soon as this side is done.
        let mut a = a;
        let out = Outgoing {
            bundle: &path,
            manifest: &manifest,
            labels,
        };
        send(
            &mut a,
            &local("PC"),
            &out,
            &sender_cancel,
            &Recorder::default(),
            &limits,
        )
        .await
    };
    let recv = async {
        let mut b = b;
        let mut tx = Some(tx);
        let me = local("Phone");
        let got = {
            let fut = receive(&mut b, &me, &receiver, rx, &receiver_cancel, &*ev, &limits);
            tokio::pin!(fut);
            loop {
                tokio::select! {
                    r = &mut fut => break r,
                    () = tokio::time::sleep(std::time::Duration::from_millis(20)) => {
                        let previewed = ev.events.lock().unwrap().iter().any(|e| matches!(e, Event::Preview(_)));
                        if previewed {
                            if let Some(tx) = tx.take() { let _ = tx.send(accept); }
                        }
                    }
                }
            }
        };
        got
    };
    let (sent, got) = tokio::join!(sender, recv);
    Run {
        sent,
        got,
        receiver,
        events,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn preview_then_accept_writes_the_items() {
    let r = run(
        vec![("c_one", character("Mira")), ("c_two", character("Tam"))],
        Some(1024),
        true,
        Cancel::new(),
        Cancel::new(),
        Arc::default(),
    )
    .await;
    let summary = r.got.unwrap();
    assert_eq!(r.sent.unwrap(), summary);
    assert_eq!(summary.added, 3);
    let preview = r
        .events
        .events
        .lock()
        .unwrap()
        .iter()
        .find_map(|e| match e {
            Event::Preview(p) => Some(p.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(preview.peer.name, "PC");
    assert!(preview.items.iter().all(|i| i.action == PreviewAction::New));
    assert_eq!(preview.items[0].label, "Mira", "sender label, display only");
    let files = r.receiver.files();
    assert!(files.contains(&"characters/c_one.json".to_string()));
    assert!(files.contains(&"library/homebrew/files/map.bin".to_string()));
    assert!(
        !files.iter().any(|f| f.starts_with("sync/incoming")),
        "{files:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_50_mb_item_arrives_intact() {
    let size = 50 * 1024 * 1024;
    let r = run(
        vec![],
        Some(size),
        true,
        Cancel::new(),
        Cancel::new(),
        Arc::default(),
    )
    .await;
    r.got.unwrap();
    let got = std::fs::read(
        r.receiver
            .root
            .path()
            .join("library/homebrew/files/map.bin"),
    )
    .unwrap();
    assert_eq!(got, pack_files(size)[1].1);
}

#[tokio::test(flavor = "multi_thread")]
async fn decline_writes_nothing() {
    let r = run(
        vec![("c_one", character("Mira"))],
        None,
        false,
        Cancel::new(),
        Cancel::new(),
        Arc::default(),
    )
    .await;
    assert_eq!(r.got.unwrap_err(), TransferError::Declined);
    assert_eq!(r.sent.unwrap_err(), TransferError::Declined);
    assert!(r.receiver.files().is_empty());
}

fn cancel_after(bytes: u64, cancel: &Cancel) -> Arc<Recorder> {
    let rec = Arc::new(Recorder::default());
    let c = cancel.clone();
    *rec.on_progress.lock().unwrap() = Some(Box::new(move |done| {
        if done >= bytes {
            c.cancel();
        }
    }));
    rec
}

#[tokio::test(flavor = "multi_thread")]
async fn receiver_cancel_mid_way_leaves_no_partial_files() {
    let cancel = Cancel::new();
    let events = cancel_after(4 * 1024 * 1024, &cancel);
    let r = run(
        vec![("c_one", character("Mira"))],
        Some(20 << 20),
        true,
        Cancel::new(),
        cancel,
        events,
    )
    .await;
    assert_eq!(r.got.unwrap_err(), TransferError::Cancelled);
    assert!(r.sent.is_err());
    assert!(r.receiver.files().is_empty(), "{:?}", r.receiver.files());
}

#[tokio::test(flavor = "multi_thread")]
async fn sender_cancel_mid_way_leaves_no_partial_files() {
    let cancel = Cancel::new();
    let c = cancel.clone();
    // Cancel the sender once the receiver has some bytes.
    let events = Arc::new(Recorder::default());
    *events.on_progress.lock().unwrap() = Some(Box::new(move |done| {
        if done >= 4 * 1024 * 1024 {
            c.cancel();
        }
    }));
    let r = run(vec![], Some(20 << 20), true, cancel, Cancel::new(), events).await;
    assert_eq!(r.sent.unwrap_err(), TransferError::Cancelled);
    assert!(r.got.is_err());
    assert!(r.receiver.files().is_empty(), "{:?}", r.receiver.files());
}

/// A hostile sender: says hello, sends `offer`, then (if accepted) `data` and Done.
async fn hostile(
    offer: Control,
    data: Vec<u8>,
) -> (Result<borealis_transfer::Summary, TransferError>, Device) {
    let (mut evil, b) = pair().await;
    let device = Device::new();
    let (tx, rx) = oneshot::channel();
    tx.send(true).unwrap();
    let attacker = async move {
        let hello = Control::Hello {
            v: PROTOCOL,
            name: "../../evil\u{0}".into(),
            platform: borealis_link::Platform::Unknown,
            role: Role::Send,
        };
        evil.send(&control(&hello)).await.unwrap();
        let _ = evil.recv().await;
        evil.send(&control(&offer)).await.unwrap();
        if let Ok(Some(reply)) = evil.recv().await {
            if reply.get(1..) == Some(br#"{"type":"accept"}"#.as_slice()) {
                for chunk in data.chunks(512 * 1024) {
                    let frame = [&[TAG_CHUNK][..], chunk].concat();
                    if evil.send(&frame).await.is_err() {
                        return;
                    }
                }
                let _ = evil.send(&control(&Control::Done)).await;
                let _ = evil.recv().await;
            }
        }
    };
    let limits = TransferLimits::default();
    let rec = Recorder::default();
    let me = local("Phone");
    let cancel = Cancel::new();
    let receiver = async {
        let mut b = b;
        receive(&mut b, &me, &device, rx, &cancel, &rec, &limits).await
    };
    let (_, got) = tokio::join!(attacker, receiver);
    (got, device)
}

fn offer_for(bytes: &[u8], manifest: borealis_bundle::Manifest) -> Control {
    Control::Offer {
        manifest: Box::new(manifest),
        bundle_size: bytes.len() as u64,
        bundle_sha256: borealis_bundle::sha256_hex(bytes),
        labels: BTreeMap::new(),
    }
}

fn real_bundle(chars: &[(&str, Vec<u8>)]) -> (Vec<u8>, borealis_bundle::Manifest) {
    let dir = tempfile::tempdir().unwrap();
    let (path, manifest) = bundle_file(dir.path(), chars, &[]);
    (std::fs::read(path).unwrap(), manifest)
}

fn assert_refused(r: &Result<borealis_transfer::Summary, TransferError>) {
    assert!(matches!(r, Err(TransferError::Refused(_))), "{r:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bad_checksum_is_refused() {
    let (bytes, manifest) = real_bundle(&[("c_one", character("Mira"))]);
    let mut offer = offer_for(&bytes, manifest);
    if let Control::Offer { bundle_sha256, .. } = &mut offer {
        *bundle_sha256 = "0".repeat(64);
    }
    let (got, device) = hostile(offer, bytes).await;
    assert_refused(&got);
    assert!(device.files().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversize_offer_is_refused_before_any_data() {
    let (bytes, manifest) = real_bundle(&[("c_one", character("Mira"))]);
    let mut offer = offer_for(&bytes, manifest);
    if let Control::Offer { bundle_size, .. } = &mut offer {
        *bundle_size = TransferLimits::default().max_bundle_bytes + 1;
    }
    let (got, device) = hostile(offer, Vec::new()).await;
    assert_refused(&got);
    assert!(device.files().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn more_data_than_offered_is_refused() {
    let (bytes, manifest) = real_bundle(&[("c_one", character("Mira"))]);
    let offer = offer_for(&bytes, manifest);
    let mut data = bytes.clone();
    data.extend(vec![0u8; 4096]);
    let (got, device) = hostile(offer, data).await;
    assert_refused(&got);
    assert!(device.files().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn traversal_names_in_the_offer_are_refused() {
    let (bytes, mut manifest) = real_bundle(&[("c_one", character("Mira"))]);
    manifest.items[0].key = "../../evil".into();
    let (got, device) = hostile(offer_for(&bytes, manifest), bytes).await;
    assert_refused(&got);
    assert!(device.files().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bundle_that_differs_from_the_offer_is_refused() {
    let (bytes, _) = real_bundle(&[("c_one", character("Mira"))]);
    let (_, other) = real_bundle(&[("c_two", character("Tam"))]);
    let (got, device) = hostile(offer_for(&bytes, other), bytes).await;
    assert_refused(&got);
    assert!(device.files().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invalid_character_refuses_the_whole_bundle() {
    let (bytes, manifest) = real_bundle(&[
        ("c_good", character("Mira")),
        ("c_bad", br#"{"not":"a character"}"#.to_vec()),
    ]);
    let (got, device) = hostile(offer_for(&bytes, manifest), bytes).await;
    assert_refused(&got);
    assert!(device.files().is_empty(), "{:?}", device.files());
}

/// Z: a hostile receiver's `Failed` reason is shown to the sending user; it arrives without
/// control or bidi characters and capped, so it cannot flood or reorder the status line.
#[tokio::test(flavor = "multi_thread")]
async fn a_hostile_receivers_reason_is_cleaned_and_capped() {
    let src = tempfile::tempdir().unwrap();
    let (path, manifest) = bundle_file(src.path(), &[("c_one", character("Mira"))], &[]);
    let (a, mut evil) = pair().await;
    let reason = format!("\u{202E}call\n{}\u{0}", "A".repeat(100_000));
    let attacker = async move {
        let _ = evil.recv().await;
        let hello = Control::Hello {
            v: PROTOCOL,
            name: "evil".into(),
            platform: borealis_link::Platform::Unknown,
            role: Role::Receive,
        };
        evil.send(&control(&hello)).await.unwrap();
        let _ = evil.recv().await;
        evil.send(&control(&Control::Failed { reason }))
            .await
            .unwrap();
        let _ = evil.recv().await;
    };
    let sender = async {
        let mut a = a;
        let out = Outgoing {
            bundle: &path,
            manifest: &manifest,
            labels: BTreeMap::new(),
        };
        let limits = TransferLimits::default();
        let rec = Recorder::default();
        send(&mut a, &local("PC"), &out, &Cancel::new(), &rec, &limits).await
    };
    let (_, sent) = tokio::join!(attacker, sender);
    let Err(TransferError::Refused(shown)) = sent else {
        panic!("{sent:?}");
    };
    assert!(shown.chars().count() <= borealis_transfer::MAX_PEER_REASON_CHARS);
    assert!(shown.starts_with("callAAA"), "{shown}");
    assert!(!shown
        .chars()
        .any(borealis_link::registry::is_unsafe_display_char));
}
