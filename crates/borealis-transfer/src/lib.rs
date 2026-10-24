//! One-shot device transfer (WP B3): the sender offers a `borealis-bundle/1`, the receiver
//! previews what Accept would do (A1 planner: new / replace / keep local), the user accepts, the
//! bundle streams over an authenticated [`borealis_link::Connection`], the receiver checks the
//! SHA-256 and that the bundle matches the offer, and the app applies it.
//!
//! Generic over the transport; authentication ([`borealis_link::auth`]) happens before. No UI,
//! no Tauri: the app supplies a [`ReceiveHost`] and [`Events`]. Protocol: [`wire`].

mod error;
mod file;
mod limits;
mod preview;
mod receive;
mod send;
mod session;
pub mod wire;

pub use error::{plain_reason, TransferError};
pub use limits::{Cancel, TransferLimits};
pub use file::{apply_bundle_file, open_bundle_file};
pub use preview::{check_manifest, check_offer, Preview, PreviewAction, PreviewItem};
pub use receive::{receive, ReceiveHost};
pub use send::{send, Outgoing};
pub use session::{Event, Events, Local, PeerInfo};
pub use wire::{Role, Summary};
