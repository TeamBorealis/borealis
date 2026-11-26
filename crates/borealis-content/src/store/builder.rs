use super::{apply_append, load_order, ContentStore, LoadedIndex, Overridden, StoredElement};
use crate::element::{Append, Element};
use crate::error::ContentError;
use crate::file::{ContentFile, IndexFile, PackFileKind, PackManifest, PackOrigin};
use crate::issue::{Issue, IssueCode};
use crate::ruleset::Ruleset;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Summary of a loaded pack manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackInfo {
    /// Pack id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Pack version.
    pub version: String,
    /// Core or custom.
    pub origin: PackOrigin,
    /// Rulesets targeted.
    pub rulesets: Vec<Ruleset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PendingFile {
    pub(super) name: String,
    pub(super) origin: PackOrigin,
    /// Shared by every copy of the builder (a copy of the builder copies no element).
    pub(super) file: Arc<ContentFile>,
}

/// One pack folder read and parsed but not yet added to a builder ([`ParsedPack::read`],
/// [`ContentStoreBuilder::add_parsed_pack`]).
#[derive(Debug, Clone)]
pub struct ParsedPack {
    info: PackInfo,
    origin: PackOrigin,
    entries: Vec<ParsedEntry>,
}

#[derive(Debug, Clone)]
enum ParsedEntry {
    Content(String, ContentFile),
    Index(String, IndexFile),
}

impl ParsedPack {
    /// Read a pack directory: `pack.json` plus the files it lists, in manifest order.
    ///
    /// # Errors
    /// I/O and JSON errors, and [`ContentError::UnsafePath`] for manifest entries that
    /// escape the directory.
    pub fn read(dir: &Path) -> Result<Self, ContentError> {
        let manifest_path = dir.join("pack.json");
        let manifest = PackManifest::from_json_str(&read_text(&manifest_path)?)
            .map_err(|e| e.in_file(&manifest_path))?;
        let mut entries = Vec::with_capacity(manifest.files.len());
        for entry in &manifest.files {
            let path = safe_join(dir, &entry.path)?;
            let text = read_text(&path)?;
            entries.push(match entry.kind {
                PackFileKind::Content => ParsedEntry::Content(
                    entry.path.clone(),
                    ContentFile::from_json_str(&text).map_err(|e| e.in_file(&path))?,
                ),
                PackFileKind::Index => ParsedEntry::Index(
                    entry.path.clone(),
                    IndexFile::from_json_str(&text).map_err(|e| e.in_file(&path))?,
                ),
            });
        }
        Ok(Self::assemble(manifest, entries))
    }

    /// A pack from its manifest and its content files held in memory, `(path in the manifest, file)`: the same
    /// result as [`ParsedPack::read`] on a folder holding exactly these files (the homebrew creator builds the pack it
    /// is about to write, checks it, then writes it). Files the manifest lists that are not given are left out.
    #[must_use]
    pub fn from_parts(manifest: PackManifest, files: Vec<(String, ContentFile)>) -> Self {
        let entries = manifest
            .files
            .iter()
            .filter(|f| f.kind == PackFileKind::Content)
            .filter_map(|f| {
                files
                    .iter()
                    .find(|(path, _)| *path == f.path)
                    .map(|(path, file)| ParsedEntry::Content(path.clone(), file.clone()))
            })
            .collect();
        Self::assemble(manifest, entries)
    }

    /// Entries are named `<pack id>/<path>`; a content file without its own ruleset takes the pack's only one.
    fn assemble(manifest: PackManifest, entries: Vec<ParsedEntry>) -> Self {
        let entries = entries
            .into_iter()
            .map(|e| match e {
                ParsedEntry::Content(path, mut file) => {
                    if file.ruleset.is_none() && manifest.rulesets.len() == 1 {
                        file.ruleset = manifest.rulesets.first().copied();
                    }
                    ParsedEntry::Content(format!("{}/{path}", manifest.id), file)
                }
                ParsedEntry::Index(path, index) => {
                    ParsedEntry::Index(format!("{}/{path}", manifest.id), index)
                }
            })
            .collect();
        Self {
            info: PackInfo {
                id: manifest.id,
                name: manifest.name,
                version: manifest.version,
                origin: manifest.origin,
                rulesets: manifest.rulesets,
            },
            origin: manifest.origin,
            entries,
        }
    }

    /// The pack's manifest summary.
    #[must_use]
    pub fn info(&self) -> &PackInfo {
        &self.info
    }

    /// Number of content elements over the pack's files.
    #[must_use]
    pub fn element_count(&self) -> usize {
        self.entries
            .iter()
            .map(|e| match e {
                ParsedEntry::Content(_, f) => f.elements.len(),
                ParsedEntry::Index(..) => 0,
            })
            .sum()
    }
}

/// Collects files and packs, then applies the load semantics in [`build`](Self::build).
///
/// Methods consume and return the builder so a store is assembled as one expression. The files are shared, not
/// copied, by a clone of the builder, so keeping a builder next to the store built from it, or a copy with one pack
/// replaced, costs only what differs.
#[derive(Debug, Clone, Default)]
pub struct ContentStoreBuilder {
    pub(super) files: Vec<PendingFile>,
    pub(super) packs: Vec<PackInfo>,
    pub(super) indexes: Vec<LoadedIndex>,
    pub(super) diagnostics: Vec<Issue>,
    /// IM14: the user's edition per imported book, by [`super::source_key`].
    pub(super) source_editions: BTreeMap<String, Ruleset>,
}

fn read_text(path: &Path) -> Result<String, ContentError> {
    std::fs::read_to_string(path).map_err(|e| ContentError::io(path, e))
}

fn safe_join(base: &Path, relative: &str) -> Result<PathBuf, ContentError> {
    let rel = Path::new(relative);
    let ok = rel
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if relative.is_empty() || !ok {
        return Err(ContentError::UnsafePath(relative.to_string()));
    }
    Ok(base.join(rel))
}

impl ContentStoreBuilder {
    /// An empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an in-memory file. Files are applied in the order they are added.
    #[must_use]
    pub fn add_file(
        mut self,
        name: impl Into<String>,
        origin: PackOrigin,
        file: ContentFile,
    ) -> Self {
        self.files.push(PendingFile {
            name: name.into(),
            origin,
            file: Arc::new(file),
        });
        self
    }

    /// Add an index file (metadata only).
    #[must_use]
    pub fn add_index(mut self, name: impl Into<String>, index: IndexFile) -> Self {
        self.indexes.push(LoadedIndex {
            file: name.into(),
            index,
        });
        self
    }

    /// Load one content JSON file.
    ///
    /// # Errors
    /// I/O and JSON errors, tagged with the path.
    pub fn add_json_file(self, path: &Path, origin: PackOrigin) -> Result<Self, ContentError> {
        let text = read_text(path)?;
        let file = ContentFile::from_json_str(&text).map_err(|e| e.in_file(path))?;
        Ok(self.add_file(path.display().to_string(), origin, file))
    }

    /// Load a pack directory: `pack.json` plus the files it lists, in manifest order.
    ///
    /// # Errors
    /// I/O and JSON errors, and [`ContentError::UnsafePath`] for manifest entries that
    /// escape the directory.
    pub fn add_pack_dir(self, dir: &Path) -> Result<Self, ContentError> {
        Ok(self.add_parsed_pack(ParsedPack::read(dir)?))
    }

    /// Add a pack read with [`ParsedPack::read`]: the same result as [`add_pack_dir`](Self::add_pack_dir)
    /// on that folder. Reading first lets a caller skip a broken pack without touching the builder.
    #[must_use]
    pub fn add_parsed_pack(mut self, pack: ParsedPack) -> Self {
        self.packs.push(pack.info);
        for entry in pack.entries {
            self = match entry {
                ParsedEntry::Content(name, file) => self.add_file(name, pack.origin, file),
                ParsedEntry::Index(name, index) => self.add_index(name, index),
            };
        }
        self
    }

    /// [`add_parsed_pack`](Self::add_parsed_pack), taking each content file that equals the file of the same name
    /// (and load semantics) in `previous` as that file, so this builder shares it instead of keeping a second copy
    /// (a reload reads every pack again; only what changed stays new).
    #[must_use]
    pub fn add_parsed_pack_sharing(self, pack: ParsedPack, previous: Option<&Self>) -> Self {
        let Some(previous) = previous else {
            return self.add_parsed_pack(pack);
        };
        let from = self.files.len();
        let mut next = self.add_parsed_pack(pack);
        let added = &mut next.files[from..];
        let names: std::collections::HashSet<&str> =
            added.iter().map(|f| f.name.as_str()).collect();
        let old: HashMap<&str, &PendingFile> = previous
            .files
            .iter()
            .filter(|f| names.contains(f.name.as_str()))
            .map(|f| (f.name.as_str(), f))
            .collect();
        for f in added {
            if let Some(o) = old.get(f.name.as_str()) {
                if o.origin == f.origin && o.file == f.file {
                    f.file = Arc::clone(&o.file);
                }
            }
        }
        next
    }

    /// True when a pack with this id was added ([`add_parsed_pack`](Self::add_parsed_pack)).
    #[must_use]
    pub fn has_pack(&self, id: &str) -> bool {
        self.packs.iter().any(|p| p.id == id)
    }

    /// The builder with the pack `pack.info().id` replaced by `pack`, in the same load position: its files and
    /// indexes take the place of the old pack's, so the result equals adding the new pack in the old one's place
    /// (the homebrew creator's fast reload, spec 14.1). `None` when no such pack was added.
    #[must_use]
    pub fn replace_pack(mut self, pack: ParsedPack) -> Option<Self> {
        let id = pack.info.id.clone();
        let pack_at = self.packs.iter().position(|p| p.id == id)?;
        let prefix = format!("{id}/");
        let file_at = self
            .files
            .iter()
            .position(|f| f.name.starts_with(&prefix))
            .unwrap_or_else(|| self.files_before_pack(pack_at));
        let index_at = self
            .indexes
            .iter()
            .position(|i| i.file.starts_with(&prefix))
            .unwrap_or(self.indexes.len());
        self.files.retain(|f| !f.name.starts_with(&prefix));
        self.indexes.retain(|i| !i.file.starts_with(&prefix));
        let file_at = file_at.min(self.files.len());
        let index_at = index_at.min(self.indexes.len());
        let mut files = Vec::new();
        let mut indexes = Vec::new();
        for entry in pack.entries {
            match entry {
                ParsedEntry::Content(name, file) => files.push(PendingFile {
                    name,
                    origin: pack.origin,
                    file: Arc::new(file),
                }),
                ParsedEntry::Index(name, index) => indexes.push(LoadedIndex { file: name, index }),
            }
        }
        self.files.splice(file_at..file_at, files);
        self.indexes.splice(index_at..index_at, indexes);
        self.packs[pack_at] = pack.info;
        Some(self)
    }

    /// The ids of the packs added, in load order.
    pub fn pack_ids(&self) -> impl Iterator<Item = &str> {
        self.packs.iter().map(|p| p.id.as_str())
    }

    /// The builder with `pack` added at position `at` of the load order (0 = first; past the end = last): the same
    /// builder as adding the packs in that order, `pack` among them (New pack in the homebrew creator puts its pack
    /// where a full load of the library folders would read it, without reading the others again).
    #[must_use]
    pub fn insert_parsed_pack(mut self, at: usize, pack: ParsedPack) -> Self {
        let at = at.min(self.packs.len());
        let file_at = self.files_before_pack(at);
        let earlier: Vec<String> = self.packs[..at]
            .iter()
            .map(|p| format!("{}/", p.id))
            .collect();
        let index_at = self
            .indexes
            .iter()
            .rposition(|i| earlier.iter().any(|e| i.file.starts_with(e.as_str())))
            .map_or(0, |i| i + 1);
        let mut files = Vec::new();
        let mut indexes = Vec::new();
        for entry in pack.entries {
            match entry {
                ParsedEntry::Content(name, file) => files.push(PendingFile {
                    name,
                    origin: pack.origin,
                    file: Arc::new(file),
                }),
                ParsedEntry::Index(name, index) => indexes.push(LoadedIndex { file: name, index }),
            }
        }
        self.files.splice(file_at..file_at, files);
        self.indexes.splice(index_at..index_at, indexes);
        self.packs.insert(at, pack.info);
        self
    }

    /// Where the files of the pack at `pack_at` go when it has none: after the files of every earlier pack.
    fn files_before_pack(&self, pack_at: usize) -> usize {
        let earlier: Vec<String> = self.packs[..pack_at]
            .iter()
            .map(|p| format!("{}/", p.id))
            .collect();
        self.files
            .iter()
            .rposition(|f| earlier.iter().any(|e| f.name.starts_with(e.as_str())))
            .map_or(0, |i| i + 1)
    }

    /// Load every content `*.json` below `dir` (skipping `pack.json` and `*.index.json`),
    /// ordered by the custom folder load order.
    ///
    /// # Errors
    /// I/O and JSON errors, tagged with the path.
    pub fn add_dir(mut self, dir: &Path, origin: PackOrigin) -> Result<Self, ContentError> {
        let mut relative = Vec::new();
        collect_json(dir, dir, &mut relative)?;
        for rel in load_order::sorted(&relative) {
            if load_order::is_ignored_path(&rel) {
                continue;
            }
            let path = dir.join(&rel);
            let text = read_text(&path)?;
            let file = ContentFile::from_json_str(&text).map_err(|e| e.in_file(&path))?;
            self = self.add_file(rel, origin, file);
        }
        Ok(self)
    }

    /// IM14 (GitHub #11): the user's edition per imported book, keyed by
    /// [`super::source_key`] of the book's name. [`build`](Self::build) re-tags the book's
    /// elements in custom (imported) files: its `Source` element, elements naming it (or one of
    /// its `alternative` names) as their source, and `Internal`/`Core` helpers in a file whose
    /// other elements all belong to such books. Edition-neutral (`any`) elements and bundled
    /// files are never re-tagged. The files on disk are not changed.
    #[must_use]
    pub fn with_source_editions(mut self, editions: BTreeMap<String, Ruleset>) -> Self {
        self.source_editions = editions;
        self
    }

    /// Apply the load pipeline: ordered files, duplicate/override rules, then appends.
    ///
    /// * `ignore: true` files are skipped.
    /// * Core files: a duplicate id keeps the first definition (error diagnostic).
    /// * Custom files: a duplicate id replaces the earlier element, which moves to the end.
    /// * HC20 `overrides` are applied after every file's elements, in file order: each takes the place of the loaded
    ///   element of its id (any origin), keeping its position; one whose element is not loaded is dropped (warning).
    /// * `<append>` records are applied last, in file order.
    #[must_use]
    pub fn build(self) -> ContentStore {
        self.build_reusing(None)
    }

    /// [`build`](Self::build), sharing with `previous` (the store this one replaces) every element that comes out
    /// equal to the element `previous` holds under its id: the result is the same store as `build` gives, but an
    /// unchanged element is the very allocation `previous` holds (`Arc::ptr_eq`), so the two stores together cost one
    /// store plus what changed (the homebrew creator's saves and every reload, item Isq-Y).
    #[must_use]
    pub fn build_reusing(self, previous: Option<&ContentStore>) -> ContentStore {
        let Self {
            files,
            packs,
            indexes,
            mut diagnostics,
            source_editions,
        } = self;
        let retagged = source_edition_tags(&files, &source_editions);
        // the edition an element of a file is built with, and the book whose user edition set it
        let edition_of = |file: &ContentFile, at: (usize, usize)| match retagged.get(&at) {
            Some((ruleset, book)) => (Some(*ruleset), Some(book.clone())),
            None => (file.elements[at.1].ruleset.or(file.ruleset), None),
        };
        // the previous store's element for each file element that is it as built (compared in parallel)
        let reused: Vec<Vec<Option<Arc<Element>>>> = match previous {
            None => Vec::new(),
            Some(p) => files
                .par_iter()
                .enumerate()
                .map(|(fi, pending)| {
                    let file = pending.file.as_ref();
                    (0..file.elements.len())
                        .map(|ei| {
                            let element = &file.elements[ei];
                            let old = p.get_exact(&element.id)?;
                            let (ruleset, _) = edition_of(file, (fi, ei));
                            old.element
                                .equals_with_ruleset(element, ruleset)
                                .then(|| Arc::clone(&old.element))
                        })
                        .collect()
                })
                .collect(),
        };
        let shared = |element: Element| -> Arc<Element> {
            match previous.and_then(|p| p.get_exact(&element.id)) {
                Some(old) if *old.element == element => Arc::clone(&old.element),
                _ => Arc::new(element),
            }
        };
        let mut slots: Vec<Option<StoredElement>> = Vec::new();
        let mut position: HashMap<String, usize> = HashMap::new();
        let mut appends: Vec<(Arc<str>, Append)> = Vec::new();
        let mut overrides: Vec<(Arc<str>, Option<Ruleset>, Arc<ContentFile>)> = Vec::new();
        let mut loaded_files = Vec::new();

        for (file_index, pending) in files.into_iter().enumerate() {
            let name: Arc<str> = Arc::from(pending.name.as_str());
            if pending.file.is_ignored() {
                diagnostics.push(
                    Issue::info(
                        IssueCode::FileSkipped,
                        "file has ignore=true and was skipped",
                    )
                    .in_file(pending.name),
                );
                continue;
            }
            loaded_files.push(pending.name.clone());
            let file = pending.file;
            for (element_index, element) in file.elements.iter().enumerate() {
                let (ruleset, user_edition_book) = edition_of(&file, (file_index, element_index));
                if let Some(&old) = position.get(&element.id) {
                    match pending.origin {
                        PackOrigin::Core => {
                            diagnostics.push(
                                Issue::error(
                                    IssueCode::DuplicateId,
                                    "duplicate element id in core content; first definition kept",
                                )
                                .in_file(name.to_string())
                                .at_element(element.id.clone()),
                            );
                            continue;
                        }
                        PackOrigin::Custom => {
                            let previous = slots[old].take();
                            let note = previous.as_ref().map_or_else(String::new, |p| {
                                if p.element.ty == element.ty {
                                    format!("replaced the definition from {}", p.file)
                                } else {
                                    format!(
                                        "replaced the definition from {} (type changed from {} to {})",
                                        p.file, p.element.ty, element.ty
                                    )
                                }
                            });
                            diagnostics.push(
                                Issue::info(IssueCode::ElementReplaced, note)
                                    .in_file(name.to_string())
                                    .at_element(element.id.clone()),
                            );
                        }
                    }
                }
                position.insert(element.id.clone(), slots.len());
                // the previous store's element when it is this one as built, else a copy with its edition
                let element = match reused
                    .get(file_index)
                    .and_then(|f| f[element_index].clone())
                {
                    Some(old) => old,
                    None => Arc::new(Element {
                        ruleset,
                        ..element.clone()
                    }),
                };
                slots.push(Some(StoredElement {
                    element,
                    file: Arc::clone(&name),
                    origin: pending.origin,
                    extended: false,
                    appended_supports: Vec::new(),
                    user_edition_book,
                    overridden: None,
                }));
            }
            appends.extend(file.appends.iter().map(|a| (Arc::clone(&name), a.clone())));
            if !file.overrides.is_empty() {
                overrides.push((Arc::clone(&name), file.ruleset, Arc::clone(&file)));
            }
        }

        for (file, file_ruleset, content) in overrides {
            for element in &content.overrides {
                apply_override(
                    &mut slots,
                    &position,
                    &mut diagnostics,
                    &file,
                    element.ruleset.or(file_ruleset),
                    element,
                );
            }
        }

        for (file, append) in appends {
            let Some(&idx) = position.get(&append.id) else {
                diagnostics.push(
                    Issue::warning(
                        IssueCode::AppendTargetMissing,
                        "append targets an element that does not exist",
                    )
                    .in_file(file.to_string())
                    .at_element(append.id.clone()),
                );
                continue;
            };
            if let Some(stored) = slots[idx].as_ref() {
                let (merged, changed) = apply_append(&stored.element, &append);
                if changed {
                    let added = merged
                        .supports
                        .iter()
                        .filter(|t| !stored.element.supports.contains(t))
                        .cloned();
                    let appended_supports = stored
                        .appended_supports
                        .iter()
                        .cloned()
                        .chain(added)
                        .collect();
                    slots[idx] = Some(StoredElement {
                        element: Arc::new(merged),
                        extended: true,
                        appended_supports,
                        ..stored.clone()
                    });
                }
            }
        }

        let elements = super::alias::resolve_phb24_aliases(
            slots.into_iter().flatten().collect(),
            |s: &StoredElement| s.element.as_ref(),
            |s, element| StoredElement {
                element: Arc::new(element),
                ..s
            },
        );
        // an element an `<append>` or an alias rewrite made anew may still equal the previous store's
        let elements = match previous {
            None => elements,
            Some(_) => elements
                .into_iter()
                .map(|s| {
                    if Arc::strong_count(&s.element) > 1 {
                        return s;
                    }
                    StoredElement {
                        element: shared(Arc::unwrap_or_clone(s.element)),
                        ..s
                    }
                })
                .collect(),
        };
        ContentStore::assemble(elements, loaded_files, packs, indexes, diagnostics)
    }

    /// The builder with each file that equals the file of the same name (and load semantics) in `previous` replaced by
    /// that file: the result builds the same store, sharing those files with `previous` (a reload that read every pack
    /// again from disk keeps one copy of what did not change).
    #[must_use]
    pub fn sharing_files_with(mut self, previous: &Self) -> Self {
        let old: HashMap<&str, &PendingFile> = previous
            .files
            .iter()
            .map(|f| (f.name.as_str(), f))
            .collect();
        for f in &mut self.files {
            if let Some(o) = old.get(f.name.as_str()) {
                if o.origin == f.origin && !Arc::ptr_eq(&o.file, &f.file) && o.file == f.file {
                    f.file = Arc::clone(&o.file);
                }
            }
        }
        self
    }
}

/// HC20 (homebrew-creator spec 22.3): `element` from `file` takes the place of the loaded element of its id, keeping
/// that element's position and recording it as the original. An original the user set a book edition for (IM14) gives
/// the override that edition. A second override of one id replaces the first (info); an override whose element is not
/// loaded is dropped (warning).
fn apply_override(
    slots: &mut [Option<StoredElement>],
    position: &HashMap<String, usize>,
    diagnostics: &mut Vec<Issue>,
    file: &Arc<str>,
    ruleset: Option<Ruleset>,
    element: &Element,
) {
    let Some(current) = position
        .get(&element.id)
        .and_then(|&at| slots[at].as_ref().map(|s| (at, s.clone())))
    else {
        diagnostics.push(
            Issue::warning(
                IssueCode::OverrideTargetMissing,
                "override of an element that is not loaded; the override is not loaded either",
            )
            .in_file(file.to_string())
            .at_element(element.id.clone()),
        );
        return;
    };
    let (at, stored) = current;
    if stored.overridden.is_some() {
        diagnostics.push(
            Issue::info(
                IssueCode::ElementReplaced,
                format!("replaced the override from {}", stored.file),
            )
            .in_file(file.to_string())
            .at_element(element.id.clone()),
        );
    }
    let original = stored.overridden.clone().unwrap_or_else(|| Overridden {
        element: Arc::clone(&stored.element),
        file: Arc::clone(&stored.file),
    });
    let ruleset = match &stored.user_edition_book {
        Some(_) => original.element.ruleset,
        None => ruleset,
    };
    slots[at] = Some(StoredElement {
        element: Arc::new(Element {
            ruleset,
            ..element.clone()
        }),
        file: Arc::clone(file),
        origin: PackOrigin::Custom,
        extended: false,
        appended_supports: Vec::new(),
        user_edition_book: stored.user_edition_book.clone(),
        overridden: Some(original),
    });
}

/// IM14: the new edition of the elements of books the user set an edition for (see
/// [`ContentStoreBuilder::with_source_editions`]), with the book key that applied, by (file, element) position.
fn source_edition_tags(
    files: &[PendingFile],
    editions: &BTreeMap<String, Ruleset>,
) -> HashMap<(usize, usize), (Ruleset, String)> {
    let mut changed = HashMap::new();
    if editions.is_empty() {
        return changed;
    }
    let custom = |p: &PendingFile| p.origin == PackOrigin::Custom;
    // every name of a set book (its own and its `alternative` names) -> (edition, book key)
    let mut by_key: BTreeMap<String, (Ruleset, String)> = editions
        .iter()
        .map(|(k, r)| (k.clone(), (*r, k.clone())))
        .collect();
    // IM17: books the importer tagged for Both editions (their `Source` element is `any`): their
    // `any` tags came from the guess, so a set edition re-tags them too
    let mut guessed_both: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for p in files.iter().filter(|p| custom(p)) {
        for el in p.file.elements.iter().filter(|e| e.ty == "Source") {
            if el.ruleset.or(p.file.ruleset) == Some(Ruleset::Any) {
                guessed_both.insert(super::source_key(&el.name));
            }
        }
    }
    for el in files
        .iter()
        .filter(|p| custom(p))
        .flat_map(|p| &p.file.elements)
    {
        if el.ty != "Source" {
            continue;
        }
        let key = super::source_key(&el.name);
        if let Some(&r) = editions.get(&key) {
            for alt in el.setters.iter().filter(|s| s.name == "alternative") {
                by_key
                    .entry(super::source_key(&alt.value))
                    .or_insert_with(|| (r, key.clone()));
            }
        }
    }
    let book_of = |el: &crate::Element| {
        let name = if el.ty == "Source" {
            &el.name
        } else {
            &el.source
        };
        by_key.get(&super::source_key(name)).cloned()
    };
    for (fi, pending) in files.iter().enumerate().filter(|(_, p)| custom(p)) {
        let file_ruleset = pending.file.ruleset;
        let books: Vec<Option<(Ruleset, String)>> = pending
            .file
            .elements
            .iter()
            .filter(|e| e.ty == "Source" || !super::is_unrestrictable_source(&e.source))
            .map(book_of)
            .collect();
        // helpers follow their file when every other element is of one set book
        let follow = books
            .first()
            .cloned()
            .flatten()
            .filter(|b| books.iter().all(|x| x.as_ref() == Some(b)));
        for (ei, el) in pending.file.elements.iter().enumerate() {
            let helper = el.ty != "Source" && super::is_unrestrictable_source(&el.source);
            let set = if helper { follow.clone() } else { book_of(el) };
            let neutral = el.ruleset.or(file_ruleset) == Some(Ruleset::Any);
            if neutral && !set.as_ref().is_some_and(|(_, b)| guessed_both.contains(b)) {
                continue;
            }
            if let Some((r, book)) = set {
                changed.insert((fi, ei), (r, book));
            }
        }
    }
    changed
}

fn collect_json(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), ContentError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| ContentError::io(dir, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| ContentError::io(dir, e))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| ContentError::io(&path, e))?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect_json(root, &path, out)?;
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        if lower.ends_with(".json") && lower != "pack.json" && !lower.ends_with(".index.json") {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(())
}

impl ContentStoreBuilder {
    /// True when both builders hold the same files, packs, indexes, diagnostics and editions, in the
    /// same order (a snapshot round trip must give `true`).
    #[must_use]
    pub fn same_content(&self, other: &Self) -> bool {
        self.files.len() == other.files.len()
            && self
                .files
                .iter()
                .zip(&other.files)
                .all(|(a, b)| a.name == b.name && a.origin == b.origin && a.file == b.file)
            && self.packs == other.packs
            && self.indexes == other.indexes
            && self.diagnostics == other.diagnostics
            && self.source_editions == other.source_editions
    }

    /// Append an existing diagnostic (for callers that load files themselves).
    #[must_use]
    pub fn with_diagnostic(mut self, issue: Issue) -> Self {
        self.diagnostics.push(issue);
        self
    }
}
