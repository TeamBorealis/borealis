//! Homebrew packs (spec 14, 15.1 `pack`): the pack layout on disk, `creator.json`, child elements, and the pure
//! steps of saving and deleting (the app does the file writes and the reload). Deterministic, no AI (G7).
//!
//! Export and import of `.borealis-pack.zip` (spec 14.2, WB-24) build on these.

mod children;
mod creator;
mod layout;

pub(crate) use children::granted_ids;
pub use children::{child_types, granted_children, parent_of};
pub use creator::{
    CopiedFrom, CreatorElement, CreatorFile, CreatorFileError, CREATOR_FILE, CREATOR_FORMAT,
};
pub use layout::{
    bump_patch, content_with, digest, manifest_rulesets, manifest_with, new_content, new_manifest,
    pack_edition, pack_id_for, paragraph_html, source_element, source_name, source_of,
    with_elements, without_elements, PackSettings, SettingsProblem, CONTENT_FILE, FIRST_VERSION,
    PACK_NAME_MAX, SOURCE_TYPE,
};

#[cfg(test)]
mod tests;
