//! Shared by the round-trip suites (spec 18.2): the bundled packs as the creator sees them, and a form with everything
//! but its editable values forgotten.

#![allow(dead_code)] // each test target uses its own part

use borealis_authoring::form::{FormState, Memo};
use borealis_content::{ContentFile, Element, PackFileKind, PackManifest};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The bundled content folder (`app/content`).
pub fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")
}

/// One pack folder: its manifest and its content files, in manifest order.
pub struct PackDir {
    pub dir: PathBuf,
    pub manifest: PackManifest,
    /// (path in the manifest, file).
    pub files: Vec<(String, ContentFile)>,
}

impl PackDir {
    /// The pack's id (`srd-5.2.1`).
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    /// Every element of the pack, in file order.
    pub fn elements(&self) -> Vec<Element> {
        self.files
            .iter()
            .flat_map(|(_, f)| f.elements.iter().cloned())
            .collect()
    }
}

/// Read one pack folder (content files only).
pub fn read_pack(dir: &Path) -> PackDir {
    let manifest =
        PackManifest::from_json_str(&fs::read_to_string(dir.join("pack.json")).expect("pack.json"))
            .expect("manifest");
    let files = manifest
        .files
        .iter()
        .filter(|f| f.kind == PackFileKind::Content)
        .map(|f| {
            let text = fs::read_to_string(dir.join(&f.path)).expect("file");
            let content =
                ContentFile::from_json_str(&text).unwrap_or_else(|e| panic!("{}: {e}", f.path));
            (f.path.clone(), content)
        })
        .collect();
    PackDir {
        dir: dir.to_path_buf(),
        manifest,
        files,
    }
}

/// Every pack folder under `root` (any depth), sorted by path.
pub fn pack_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if dir.join("pack.json").is_file() {
            out.push(dir);
            continue;
        }
        if let Ok(entries) = fs::read_dir(&dir) {
            stack.extend(
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.is_dir()),
            );
        }
    }
    out.sort();
    out
}

/// The bundled element packs, in folder order: `core`, `srd-5.1`, `srd-5.2.1` and the two monster packs. The two
/// rules-text packs hold documents (`docs.json`), no elements.
pub fn bundled_packs() -> Vec<PackDir> {
    let packs: Vec<PackDir> = pack_dirs(&content_dir())
        .iter()
        .map(|d| read_pack(d))
        .collect();
    assert_eq!(packs.len(), 5, "the bundled element packs are there");
    packs
}

/// (pack, element) for every element of every bundled pack, in pack and file order.
pub fn bundled() -> Vec<(String, Element)> {
    let mut out = Vec::new();
    for pack in bundled_packs() {
        for (path, file) in &pack.files {
            assert!(
                file.appends.is_empty(),
                "{path}: bundled appends are covered by unit tests only"
            );
        }
        let id = pack.id().to_string();
        out.extend(pack.elements().into_iter().map(|e| (id.clone(), e)));
    }
    assert!(
        out.len() >= 4000,
        "the bundled packs are there ({} elements)",
        out.len()
    );
    out
}

/// A builder value (`{value: {shape: ...}, original?}`) without the text it was read from, anywhere in `v`.
pub fn without_originals(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let is_expr = m
                .get("value")
                .and_then(Value::as_object)
                .is_some_and(|x| x.contains_key("shape"));
            Value::Object(
                m.iter()
                    .filter(|(k, _)| !(is_expr && k.as_str() == "original"))
                    .map(|(k, x)| (k.clone(), without_originals(x)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(without_originals).collect()),
        other => other.clone(),
    }
}

pub fn fields_without_originals(fields: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    fields
        .iter()
        .map(|(k, v)| (k.clone(), without_originals(v)))
        .collect()
}

/// The form with all stored text forgotten (the id kept): every field and block writes canonically.
pub fn forgetful(form: &FormState) -> FormState {
    let mut f = form.clone();
    f.memo.fields.clear();
    f.fields = fields_without_originals(&f.fields);
    for b in &mut f.blocks {
        b.memo = None;
        b.fields = fields_without_originals(&b.fields);
    }
    f
}

/// Spec 18.2 re-creation: the form as a user who typed every editable field would have it. Its stored text and
/// layout are forgotten ([`forgetful`]), its preserved bucket and every block's are emptied (so the 2024 mirror keys no
/// field writes go too, and the mirrors a field writes are written only when the stored bytes say the element had
/// them, which they no longer do). The ids stay (the element replaces the original in place); children too.
pub fn recreated(form: &FormState) -> FormState {
    let mut f = forgetful(form);
    f.memo = Memo {
        id: form.memo.id.clone(),
        multiclass_id: form.memo.multiclass_id.clone(),
        ..Memo::default()
    };
    f.preserved.clear();
    for b in &mut f.blocks {
        b.preserved.clear();
    }
    f.children = form.children.iter().map(recreated).collect();
    f
}

/// What a form's values are, stored text aside: fields, and each block's kind, values and kept parts.
pub fn values(form: &FormState) -> Value {
    let blocks: Vec<Value> = form
        .blocks
        .iter()
        .map(|b| {
            let kept: Vec<&str> = b.preserved.iter().map(|p| p.key.as_str()).collect();
            serde_json::json!({
                "kind": b.kind,
                "fields": fields_without_originals(&b.fields),
                "kept": kept,
            })
        })
        .collect();
    serde_json::json!({ "fields": fields_without_originals(&form.fields), "blocks": blocks })
}
