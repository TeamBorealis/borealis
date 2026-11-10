//! Deterministic element ids: `ID_<PREFIX>_<TYPE>_<NAME>`. Moved to `borealis_authoring::ids` (shared with the
//! homebrew creator); re-exported here so callers are unchanged.

pub use borealis_authoring::ids::{make_id, reassign_ids, slug};
