//! Sender side: offer a bundle file, wait for the receiving user, stream it, await the result.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use borealis_bundle::Manifest;
use borealis_link::Connection;
use sha2::{Digest, Sha256};

use crate::error::TransferError;
use crate::limits::{Cancel, TransferLimits};
use crate::session::{hello, recv_ctl, send_ctl, Event, Events, Local};
use crate::wire::{link, Control, Role, Summary, TAG_CHUNK};

/// What to send: a bundle file written by `borealis_bundle::build_bundle` and its manifest.
#[derive(Debug, Clone)]
pub struct Outgoing<'a> {
    /// The bundle file.
    pub bundle: &'a Path,
    /// Its manifest.
    pub manifest: &'a Manifest,
    /// Display names by item key.
    pub labels: BTreeMap<String, String>,
}

fn hash_file(path: &Path) -> Result<(u64, String), TransferError> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        size += n as u64;
    }
    Ok((size, hex(&h.finalize())))
}

fn hex(bytes: &[u8]) -> String {
    borealis_link::random::hex(bytes)
}

/// Run the sender side on an authenticated connection.
///
/// # Errors
/// [`TransferError`]: declined, cancelled (either side), refused by the receiver, link failure.
pub async fn send<C: Connection>(
    conn: &mut C,
    local: &Local,
    out: &Outgoing<'_>,
    cancel: &Cancel,
    events: &dyn Events,
    limits: &TransferLimits,
) -> Result<Summary, TransferError> {
    hello(conn, local, Role::Send, cancel, limits, events).await?;
    let (size, sha) = hash_file(out.bundle)?;
    let offer = Control::Offer {
        manifest: Box::new(out.manifest.clone()),
        bundle_size: size,
        bundle_sha256: sha,
        labels: out.labels.clone(),
    };
    send_ctl(conn, &offer).await?;
    events.event(Event::WaitingForPeer);
    loop {
        match recv_ctl(conn, cancel, limits).await? {
            Control::Waiting => {}
            Control::Accept => break,
            Control::Decline => return Err(TransferError::Declined),
            Control::Failed { reason } => return Err(TransferError::Refused(reason)),
            _ => {
                return Err(TransferError::Protocol(
                    "unexpected reply to the offer".into(),
                ))
            }
        }
    }
    stream(conn, out.bundle, size, cancel, events, limits).await?;
    send_ctl(conn, &Control::Done).await?;
    match recv_ctl(conn, cancel, limits).await? {
        Control::Finished { summary } => Ok(summary),
        Control::Failed { reason } => Err(TransferError::Refused(reason)),
        _ => Err(TransferError::Protocol(
            "unexpected reply after the data".into(),
        )),
    }
}

async fn stream<C: Connection>(
    conn: &mut C,
    path: &Path,
    total: u64,
    cancel: &Cancel,
    events: &dyn Events,
    limits: &TransferLimits,
) -> Result<(), TransferError> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; limits.chunk_bytes + 1];
    let mut done = 0u64;
    loop {
        buf[0] = TAG_CHUNK;
        let n = f.read(&mut buf[1..])?;
        if n == 0 {
            break;
        }
        tokio::select! {
            r = conn.send(&buf[..=n]) => r.map_err(link)?,
            // A frame may be half-written: the connection is dropped, never reused.
            () = cancel.cancelled() => return Err(TransferError::Cancelled),
        }
        done += n as u64;
        events.event(Event::Progress { done, total });
    }
    if done != total {
        return Err(TransferError::Local(
            "the bundle changed while sending".into(),
        ));
    }
    Ok(())
}
