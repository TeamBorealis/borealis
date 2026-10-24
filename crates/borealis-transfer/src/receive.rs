//! Receiver side: check the offer, preview it, wait for the user, stream into a temp file, verify
//! the SHA-256 and the manifest, then hand the bundle to the app to apply.
//!
//! The incoming bytes only ever live in an anonymous temp file inside the app's
//! `<data>/sync/incoming` folder, which is deleted on every exit path (cancel, disconnect, bad
//! hash, refusal). Nothing reaches the user's folders before the whole bundle verified.

use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::Instant;

use borealis_bundle::{ApplyReport, Bundle, Manifest, Plan};
use borealis_link::Connection;
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

use crate::error::TransferError;
use crate::limits::{Cancel, TransferLimits};
use crate::preview::{check_offer, preview};
use crate::session::{hello, recv_ctl, recv_frame, send_ctl, Event, Events, Local};
use crate::wire::{Control, Frame, Role, Summary};

/// What the receiving app provides.
pub trait ReceiveHost: Send + Sync {
    /// Plan `manifest` against this device's current state (A1 `plan_receive`).
    ///
    /// # Errors
    /// A user-facing reason (the offer is then refused).
    fn plan(&self, manifest: &Manifest) -> Result<Plan, String>;

    /// Apply a verified bundle (A1 `apply_plan` with the app's validators).
    ///
    /// # Errors
    /// A user-facing reason (the bundle was refused; nothing was written).
    fn apply(&self, bundle: &mut Bundle<std::fs::File>, plan: &Plan)
        -> Result<ApplyReport, String>;

    /// Folder for the temp file (same volume as the data folder).
    fn incoming_dir(&self) -> PathBuf;
}

pub(crate) fn summary(r: &ApplyReport) -> Summary {
    Summary {
        added: r.added.len(),
        replaced: r.replaced.len(),
        kept_local: r.kept_local.len(),
        identical: r.identical.len(),
    }
}

/// Run the receiver side on an authenticated connection. `decision` resolves with the user's
/// Accept (`true`) or Decline (`false`) after the [`Event::Preview`].
///
/// # Errors
/// [`TransferError`]; on every error no partial file remains.
pub async fn receive<C: Connection>(
    conn: &mut C,
    local: &Local,
    host: &dyn ReceiveHost,
    decision: oneshot::Receiver<bool>,
    cancel: &Cancel,
    events: &dyn Events,
    limits: &TransferLimits,
) -> Result<Summary, TransferError> {
    let peer = hello(conn, local, Role::Receive, cancel, limits, events).await?;
    let Control::Offer {
        manifest,
        bundle_size,
        bundle_sha256,
        labels,
    } = recv_ctl(conn, cancel, limits).await?
    else {
        return Err(TransferError::Protocol("expected an offer".into()));
    };
    if let Err(e) = check_offer(&manifest, bundle_size, &bundle_sha256, &labels, limits) {
        let _ = send_ctl(
            conn,
            &Control::Failed {
                reason: e.to_string(),
            },
        )
        .await;
        return Err(e);
    }
    let plan = host.plan(&manifest).map_err(TransferError::Local)?;
    events.event(Event::Preview(preview(
        &peer,
        &manifest,
        &plan,
        &labels,
        bundle_size,
    )));
    if !decide(conn, decision, cancel, limits).await? {
        let _ = send_ctl(conn, &Control::Decline).await;
        return Err(TransferError::Declined);
    }
    send_ctl(conn, &Control::Accept).await?;
    let file = download(
        conn,
        host,
        bundle_size,
        &bundle_sha256,
        cancel,
        events,
        limits,
    )
    .await?;
    events.event(Event::Applying);
    match verify_and_apply(file, &manifest, host, limits) {
        Ok(report) => {
            let s = summary(&report);
            send_ctl(conn, &Control::Finished { summary: s.clone() }).await?;
            Ok(s)
        }
        Err(e) => {
            let _ = send_ctl(
                conn,
                &Control::Failed {
                    reason: e.to_string(),
                },
            )
            .await;
            Err(e)
        }
    }
}

/// Wait for the user, keeping the connection alive; cancel and the decision timeout end it.
async fn decide<C: Connection>(
    conn: &mut C,
    mut decision: oneshot::Receiver<bool>,
    cancel: &Cancel,
    limits: &TransferLimits,
) -> Result<bool, TransferError> {
    let deadline = Instant::now() + limits.decision_timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(false);
        }
        tokio::select! {
            d = &mut decision => return Ok(d.unwrap_or(false)),
            () = cancel.cancelled() => {
                let _ = send_ctl(conn, &Control::Cancel).await;
                return Err(TransferError::Cancelled);
            }
            () = tokio::time::sleep(limits.keepalive.min(left)) => {
                send_ctl(conn, &Control::Waiting).await?;
            }
        }
    }
}

async fn download<C: Connection>(
    conn: &mut C,
    host: &dyn ReceiveHost,
    size: u64,
    sha: &str,
    cancel: &Cancel,
    events: &dyn Events,
    limits: &TransferLimits,
) -> Result<std::fs::File, TransferError> {
    let dir = host.incoming_dir();
    std::fs::create_dir_all(&dir)?;
    // Unnamed where the OS allows it: no path to leave behind, even on a crash.
    let mut file = tempfile::tempfile_in(&dir)?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    loop {
        match recv_frame(conn, cancel, limits).await? {
            Frame::Chunk(bytes) => {
                done = done.saturating_add(bytes.len() as u64);
                if done > size {
                    return Err(TransferError::Refused("more data than offered".into()));
                }
                hasher.update(&bytes);
                file.write_all(&bytes)?;
                events.event(Event::Progress { done, total: size });
            }
            Frame::Control(Control::Done) => break,
            Frame::Control(_) => return Err(TransferError::Protocol("unexpected message".into())),
        }
    }
    if done != size {
        return Err(TransferError::Refused("less data than offered".into()));
    }
    if borealis_link::random::hex(&hasher.finalize()) != sha {
        let e = TransferError::Refused("checksum mismatch".into());
        let _ = send_ctl(
            conn,
            &Control::Failed {
                reason: e.to_string(),
            },
        )
        .await;
        return Err(e);
    }
    file.flush()?;
    file.seek(SeekFrom::Start(0))?;
    Ok(file)
}

pub(crate) fn verify_and_apply(
    file: std::fs::File,
    offered: &Manifest,
    host: &dyn ReceiveHost,
    limits: &TransferLimits,
) -> Result<ApplyReport, TransferError> {
    let refused =
        |e: borealis_bundle::BundleError| TransferError::Refused(crate::error::plain_reason(&e));
    let mut bundle = Bundle::open_with(file, limits.bundle).map_err(refused)?;
    if bundle.manifest() != offered {
        return Err(TransferError::Refused(
            "the bundle does not match what was offered".into(),
        ));
    }
    bundle.verify_all().map_err(refused)?;
    let plan = host.plan(offered).map_err(TransferError::Local)?;
    host.apply(&mut bundle, &plan)
        .map_err(TransferError::Refused)
}
