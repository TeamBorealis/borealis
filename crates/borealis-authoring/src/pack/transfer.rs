//! Export and import of a homebrew pack file (spec 14.2, HC7, HC8): the pure steps. The app reads and writes the files
//! and the `.borealis-pack.zip` itself.
//!
//! * **Leave text out** ([`without_copied_text`]): the elements `creator.json` marks `nonSrdText` (text copied from the
//!   user's own non-SRD imports, CS1) lose that text: the description and sheet texts are emptied, a creature's
//!   stat-block texts become [`LEFT_OUT_TEXT`] (a stat block needs text to stay valid) and its flavour text goes. The
//!   rules, numbers and names stay. The `nonSrdText` mark stays too, so the imported pack is still not redistributable.
//! * **Import as a copy** ([`pack_copy`]): the pack gets a new code, id and book name; every element id of the pack is
//!   given again (`ID_<OLD>_...` becomes `ID_<NEW>_...`; an id of another code, from a move, gets a new creator id),
//!   so is every other `ID_<OLD>_...` token the pack holds (a class's multiclass node, a deleted element's id), and
//!   every reference follows, as a whole token anywhere in the content (expressions included). Ids of other packs
//!   (the SRD a class grants, an append's bundled target) are left alone.
//!
//! Deterministic, no AI (G7).

use super::creator::CreatorFile;
use super::layout::{pack_id_for, PACK_NAME_MAX, SOURCE_TYPE};
use super::refs::id_tokens;
use crate::ids::{is_pack_code, IdGenerator, PACK_CODE_MAX};
use borealis_content::{ContentFile, Element, PackManifest, Setter};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};

/// The end of an exported pack file's name (`Ember Isles.borealis-pack.zip`).
pub const PACK_FILE_SUFFIX: &str = ".borealis-pack.zip";
/// What a creature's stat-block texts say once the copied text is left out.
pub const LEFT_OUT_TEXT: &str = "Text left out of this shared copy.";
/// What a copied pack's name ends with.
const COPY_SUFFIX: &str = " (copy)";
/// A creature's stat-block sections holding texts (`data` keys of `MonsterData`).
const STAT_BLOCK_LISTS: &[&str] = &["traits", "actions", "bonus_actions", "reactions"];
const STAT_BLOCK_SECTIONS: &[&str] = &["legendary", "lair_actions", "regional_effects"];

/// The ids `creator.json` marks as holding text copied from non-SRD packs that still have that text: what the export
/// dialog counts ("3 elements contain text copied from your imported books").
#[must_use]
pub fn copied_text_ids(content: &ContentFile, creator: &CreatorFile) -> Vec<String> {
    content
        .elements
        .iter()
        .filter(|e| creator.elements.get(&e.id).is_some_and(|c| c.non_srd_text))
        .filter(|e| has_text(e))
        .map(|e| e.id.clone())
        .collect()
}

/// The content with the copied non-SRD text left out (module docs), and the ids it changed.
#[must_use]
pub fn without_copied_text(
    content: &ContentFile,
    creator: &CreatorFile,
) -> (ContentFile, Vec<String>) {
    let ids: HashSet<String> = copied_text_ids(content, creator).into_iter().collect();
    let mut next = content.clone();
    for e in next.elements.iter_mut().filter(|e| ids.contains(&e.id)) {
        *e = blank_text(e);
    }
    let mut changed: Vec<String> = ids.into_iter().collect();
    changed.sort();
    (next, changed)
}

/// True when the element holds text: a description, sheet texts, or a creature's stat-block texts.
fn has_text(e: &Element) -> bool {
    let sheet = e
        .sheet
        .as_ref()
        .is_some_and(|s| s.descriptions.iter().any(|d| !d.text.trim().is_empty()));
    let mut stat_block = false;
    for v in stat_block_values(e) {
        visit_texts(&mut v.clone(), &mut |t| {
            stat_block |= !t.trim().is_empty() && t != LEFT_OUT_TEXT;
        });
    }
    let flavour = e
        .data
        .get("description")
        .and_then(Value::as_str)
        .is_some_and(|t| !t.trim().is_empty());
    !e.description.trim().is_empty() || sheet || stat_block || (is_creature(e) && flavour)
}

fn is_creature(e: &Element) -> bool {
    e.ty == "Monster"
}

fn stat_block_values(e: &Element) -> Vec<&Value> {
    if !is_creature(e) {
        return Vec::new();
    }
    STAT_BLOCK_LISTS
        .iter()
        .chain(STAT_BLOCK_SECTIONS)
        .filter_map(|k| e.data.get(*k))
        .collect()
}

/// Calls `f` on every text of a stat-block section: a feature's `text`, a section's `preamble` and `closing`, a lair
/// option's `text`.
fn visit_texts(v: &mut Value, f: &mut dyn FnMut(&mut String)) {
    match v {
        Value::Array(items) => items.iter_mut().for_each(|x| visit_texts(x, f)),
        Value::Object(obj) => {
            for (k, x) in obj.iter_mut() {
                match (k.as_str(), x) {
                    ("text" | "preamble" | "closing", Value::String(s)) => f(s),
                    ("actions" | "options", list @ Value::Array(_)) => visit_texts(list, f),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

/// The element without its text (module docs).
fn blank_text(e: &Element) -> Element {
    let mut next = e.clone();
    next.description = String::new();
    if let Some(sheet) = next.sheet.as_mut() {
        sheet.descriptions.clear();
    }
    if is_creature(e) {
        next.data.remove("description");
        for key in STAT_BLOCK_LISTS.iter().chain(STAT_BLOCK_SECTIONS) {
            if let Some(v) = next.data.get_mut(*key) {
                visit_texts(v, &mut |t| {
                    if !t.trim().is_empty() {
                        *t = LEFT_OUT_TEXT.to_string();
                    }
                });
            }
        }
    }
    next
}

/// A free code for an imported copy of the pack `code`: its letters and the next number (`EI` -> `EI2`, `EI2` ->
/// `EI3`), at most [`PACK_CODE_MAX`] long, not in `taken`.
#[must_use]
pub fn copy_code(code: &str, taken: &HashSet<String>) -> String {
    let stem = code.trim_end_matches(|c: char| c.is_ascii_digit());
    let stem = if stem.is_empty() { code } else { stem };
    (2u32..)
        .map(|n| {
            let n = n.to_string();
            let keep = PACK_CODE_MAX.saturating_sub(n.len()).min(stem.len());
            format!("{}{n}", &stem[..keep])
        })
        .find(|c| is_pack_code(c) && !taken.contains(c))
        .unwrap_or_else(|| code.to_string())
}

/// The name of an imported copy: "<Name> (copy)", within [`PACK_NAME_MAX`] characters.
#[must_use]
pub fn copy_name(name: &str) -> String {
    let room = PACK_NAME_MAX.saturating_sub(COPY_SUFFIX.chars().count());
    let base: String = name.trim().chars().take(room).collect();
    format!("{}{COPY_SUFFIX}", base.trim_end())
}

/// An imported pack made a copy: its files and the ids given again (`(old, new)`, in content order).
#[derive(Debug, Clone)]
pub struct PackCopy {
    pub manifest: PackManifest,
    pub content: ContentFile,
    pub creator: CreatorFile,
    pub ids: Vec<(String, String)>,
}

/// The pack as a copy with the code `code` and the name `name` (module docs). `ids` is a generator for `code` that
/// avoids every loaded id.
///
/// # Errors
/// The content can't go through JSON (never for content that was read from JSON).
pub fn pack_copy(
    manifest: &PackManifest,
    content: &ContentFile,
    creator: &CreatorFile,
    name: &str,
    ids: &mut IdGenerator,
) -> Result<PackCopy, serde_json::Error> {
    let code = ids.code().to_string();
    let old_prefix = format!("ID_{}_", creator.code);
    let mut map: Vec<(String, String)> = Vec::new();
    for e in &content.elements {
        let new = match e.id.strip_prefix(&old_prefix) {
            Some(rest) => free_id(ids, &format!("ID_{code}_{rest}")),
            None => ids.element_id(&e.ty, &e.name),
        };
        map.push((e.id.clone(), new));
    }
    let mut value = serde_json::to_value(content)?;
    // ids the pack made that are not element ids (a class's multiclass node) and ids of its deleted elements
    let mut own_tokens = BTreeSet::new();
    collect_tokens(&value, &old_prefix, &mut own_tokens);
    for token in own_tokens {
        if !map.iter().any(|(old, _)| *old == token) {
            let new = free_id(ids, &format!("ID_{code}_{}", &token[old_prefix.len()..]));
            map.push((token, new));
        }
    }
    let lookup: HashMap<&str, &str> = map.iter().map(|(o, n)| (o.as_str(), n.as_str())).collect();
    rewrite_ids(&mut value, &lookup);
    let mut next: ContentFile = serde_json::from_value(value)?;
    let old_book = next
        .elements
        .iter()
        .find(|e| e.ty == SOURCE_TYPE)
        .map(|s| s.name.clone());
    for e in &mut next.elements {
        if old_book.as_deref() == Some(e.source.as_str()) {
            e.source = name.to_string();
        }
        if e.ty == SOURCE_TYPE && old_book.as_deref() == Some(e.name.as_str()) {
            e.name = name.to_string();
            e.setters = with_setter(&e.setters, "abbreviation", &code);
        }
    }
    let mut copy_creator = creator.clone();
    copy_creator.code.clone_from(&code);
    copy_creator.retired_ids = Vec::new();
    copy_creator.elements = creator
        .elements
        .iter()
        .map(|(id, entry)| {
            let id = lookup
                .get(id.as_str())
                .map_or_else(|| id.clone(), |n| (*n).to_string());
            (id, entry.clone())
        })
        .collect();
    let mut copy_manifest = manifest.clone();
    copy_manifest.id = pack_id_for(&code);
    copy_manifest.name = name.to_string();
    Ok(PackCopy {
        manifest: copy_manifest,
        content: next,
        creator: copy_creator,
        ids: map,
    })
}

/// Every whole token of `v` (strings and object keys) that starts with `prefix`.
fn collect_tokens(v: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    fn take(s: &str, prefix: &str, out: &mut BTreeSet<String>) {
        out.extend(
            id_tokens(s)
                .filter(|t| t.starts_with(prefix) && t.len() > prefix.len())
                .map(str::to_string),
        );
    }
    match v {
        Value::String(s) => take(s, prefix, out),
        Value::Array(items) => items.iter().for_each(|x| collect_tokens(x, prefix, out)),
        Value::Object(obj) => {
            for (k, x) in obj {
                take(k, prefix, out);
                collect_tokens(x, prefix, out);
            }
        }
        _ => {}
    }
}

/// `base`, or `base_2`, `base_3`... when it is taken; reserved.
fn free_id(ids: &mut IdGenerator, base: &str) -> String {
    let id = if ids.is_taken(base) {
        (2u32..)
            .map(|n| format!("{base}_{n}"))
            .find(|c| !ids.is_taken(c))
            .unwrap_or_else(|| base.to_string())
    } else {
        base.to_string()
    };
    ids.reserve(&id);
    id
}

fn with_setter(setters: &[Setter], name: &str, value: &str) -> Vec<Setter> {
    setters
        .iter()
        .map(|s| {
            if s.name == name {
                let mut s = s.clone();
                s.value = value.to_string();
                s
            } else {
                s.clone()
            }
        })
        .collect()
}

/// Every whole id token (a run of `A-Z a-z 0-9 _`) that `map` names, replaced in every string and object key of `v`.
pub fn rewrite_ids(v: &mut Value, map: &HashMap<&str, &str>) {
    match v {
        Value::String(s) => {
            if let Some(t) = replaced(s, map) {
                *s = t;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|x| rewrite_ids(x, map)),
        Value::Object(obj) => {
            let renamed = obj.keys().any(|k| replaced(k, map).is_some());
            if renamed {
                let old = std::mem::take(obj);
                for (k, x) in old {
                    obj.insert(replaced(&k, map).unwrap_or(k), x);
                }
            }
            obj.values_mut().for_each(|x| rewrite_ids(x, map));
        }
        _ => {}
    }
}

/// `s` with each whole token `map` names replaced; `None` when nothing changes.
fn replaced(s: &str, map: &HashMap<&str, &str>) -> Option<String> {
    if !s.contains("ID_") {
        return None;
    }
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(s.len());
    let mut changed = false;
    let mut rest = s;
    while !rest.is_empty() {
        let n = rest.find(|c: char| !word(c)).unwrap_or(rest.len());
        let (token, after) = rest.split_at(n);
        match map.get(token) {
            Some(new) => {
                out.push_str(new);
                changed = true;
            }
            None => out.push_str(token),
        }
        let m = after.find(word).unwrap_or(after.len());
        out.push_str(&after[..m]);
        rest = &after[m..];
    }
    changed.then_some(out)
}
