//! The shared start of a session (hello exchange), events and frame helpers.

use borealis_link::registry::clean_name;
use borealis_link::{Connection, Platform};

use crate::error::TransferError;
use crate::limits::{Cancel, TransferLimits};
use crate::preview::Preview;
use crate::wire::{control, decode, link, Control, Frame, Role, PROTOCOL};

/// This device, as told to the peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Local {
    /// Device name.
    pub name: String,
    /// Platform.
    pub platform: Platform,
}

/// The peer, as it introduced itself (display only; its identity is the TLS certificate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    /// Cleaned device name.
    pub name: String,
    /// Platform.
    pub platform: Platform,
}

/// Progress of a running transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The hello exchange finished.
    Connected(PeerInfo),
    /// Receiver: the offer, planned against this device; waiting for Accept/Decline.
    Preview(Preview),
    /// Sender: offer sent, waiting for the other user.
    WaitingForPeer,
    /// Bundle bytes moved so far.
    Progress {
        /// Bytes done.
        done: u64,
        /// Bytes in total.
        total: u64,
    },
    /// Receiver: verifying and writing.
    Applying,
}

/// Where a transfer reports progress.
pub trait Events: Send + Sync {
    /// One event.
    fn event(&self, event: Event);
}

/// Send a control message.
pub(crate) async fn send_ctl<C: Connection>(
    conn: &mut C,
    msg: &Control,
) -> Result<(), TransferError> {
    conn.send(&control(msg)).await.map_err(link)
}

/// Receive the next frame, or fail with `Cancelled` (after telling the peer) when `cancel` fires.
pub(crate) async fn recv_frame<C: Connection>(
    conn: &mut C,
    cancel: &Cancel,
    limits: &TransferLimits,
) -> Result<Frame, TransferError> {
    let got = tokio::select! {
        f = conn.recv() => f,
        () = cancel.cancelled() => {
            let _ = send_ctl(conn, &Control::Cancel).await;
            return Err(TransferError::Cancelled);
        }
    };
    let frame = got
        .map_err(link)?
        .ok_or_else(|| TransferError::Link("the other device disconnected".into()))?;
    match decode(frame, limits.max_control_bytes)? {
        Frame::Control(Control::Cancel) => Err(TransferError::PeerCancelled),
        other => Ok(other),
    }
}

/// Receive a control message (a data frame here is a protocol error).
pub(crate) async fn recv_ctl<C: Connection>(
    conn: &mut C,
    cancel: &Cancel,
    limits: &TransferLimits,
) -> Result<Control, TransferError> {
    match recv_frame(conn, cancel, limits).await? {
        Frame::Control(c) => Ok(c),
        Frame::Chunk(_) => Err(TransferError::Protocol("unexpected data".into())),
    }
}

/// Exchange hellos; the peer must take the opposite role.
pub(crate) async fn hello<C: Connection>(
    conn: &mut C,
    local: &Local,
    role: Role,
    cancel: &Cancel,
    limits: &TransferLimits,
    events: &dyn Events,
) -> Result<PeerInfo, TransferError> {
    let mine = Control::Hello {
        v: PROTOCOL,
        name: local.name.clone(),
        platform: local.platform,
        role,
    };
    send_ctl(conn, &mine).await?;
    let Control::Hello {
        v,
        name,
        platform,
        role: theirs,
    } = recv_ctl(conn, cancel, limits).await?
    else {
        return Err(TransferError::Protocol("expected hello".into()));
    };
    if v != PROTOCOL {
        return Err(TransferError::Protocol(format!("unsupported protocol {v}")));
    }
    if theirs == role {
        let both = if role == Role::Send {
            "send"
        } else {
            "receive"
        };
        return Err(TransferError::Protocol(format!(
            "both devices want to {both}"
        )));
    }
    let peer = PeerInfo {
        name: clean_name(&name),
        platform,
    };
    events.event(Event::Connected(peer.clone()));
    Ok(peer)
}
