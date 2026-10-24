//! `borealis-bundle/1`: the zip one of the user's devices hands to another (device sync, WP A1).
//!
//! * [`BundleWriter`] / [`build_bundle`] write it; [`Bundle`] reads it with caps and validation.
//! * [`plan_receive`] decides, per item, what a receiving device does (latest edit wins, SY4).
//! * [`is_excluded`] and [`portable_settings`] enforce what never travels.
//!
//! No network and no UI here. Format reference: `docs/bundle-format.md`.

mod build;
mod error;
mod exclude;
mod inventory;
mod limits;
mod manifest;
mod plan;
mod reader;
mod writer;

pub use build::{
    build_bundle, count_kind, trash_dir, BuildReport, BuildRequest, CharacterSelection,
    PackSelection, TRASH_RETENTION_DAYS,
};
pub use error::{BundleError, Result};
pub use exclude::{
    is_excluded, is_portable_only, is_safe_rel_path, is_safe_segment, portable_settings,
    PORTABLE_SETTINGS_KEYS,
};
pub use inventory::{
    list_characters, list_packs, mtime_ms, read_pack_dir, scan_characters, scan_packs,
    CharacterFile, LocalItem, PackFiles, PackIndex,
};
pub use limits::Limits;
pub use manifest::{
    item_digest, sha256_hex, FileEntry, Item, ItemKind, Manifest, PackRef, FORMAT, MANIFEST_PATH,
    SETTINGS_PATH,
};
pub use plan::{decide, plan_receive, Action, Plan, PlanEntry};
pub use reader::{Bundle, ItemData};
pub use writer::{BundleMeta, BundleWriter};
