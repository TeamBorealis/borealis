//! Homebrew packs (spec 14, 15.1 `pack`): the pack layout on disk, `creator.json`, child elements, and the pure
//! steps of saving and deleting (the app does the file writes and the reload). Deterministic, no AI (G7).
//!
//! Export and import of `.borealis-pack.zip` (spec 14.2, WB-24): `transfer` (leave text out, import as a copy).

mod children;
mod creator;
mod layout;
mod refs;
mod spell_lists;
mod transfer;

pub use children::{child_types, descendants, granted_children, parent_of};
pub(crate) use children::{granted_ids, MAX_DEPTH};
pub use creator::{
    CopiedFrom, CreatorElement, CreatorFile, CreatorFileError, CREATOR_FILE, CREATOR_FORMAT,
};
pub use layout::{
    bump_patch, content_with, digest, manifest_rulesets, manifest_with, new_content, new_manifest,
    pack_edition, pack_id_for, paragraph_html, source_element, source_name, source_of,
    with_elements, without_elements, PackSettings, SettingsProblem, CONTENT_FILE, FIRST_VERSION,
    PACK_NAME_MAX, SOURCE_TYPE,
};
pub use refs::{id_tokens, mentions, names_any, offers, use_of, UseRelation};
pub use spell_lists::{
    appended, appended_expanded, expanded_ids, on_list, removable, with_expanded_list,
    with_spell_list, ListChange,
};
pub use transfer::{
    copied_text_ids, copy_code, copy_name, pack_copy, rewrite_ids, without_copied_text, PackCopy,
    LEFT_OUT_TEXT, PACK_FILE_SUFFIX,
};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod transfer_tests;
