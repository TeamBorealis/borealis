//! Rich text: `{@tag}` markup and `entries` to the description HTML subset.

pub mod entries;
pub mod tags;

pub use entries::{LinkFn, Renderer};
pub use tags::{strip_tags, RefKind};
