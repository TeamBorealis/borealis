//! The content store: many packs loaded in order, indexed for lookup.

mod alias;
mod builder;
pub mod load_order;
mod merge;
mod restriction;

pub use builder::{ContentStoreBuilder, PackInfo};
pub use merge::apply_append;
pub use restriction::SourceRestriction;

use crate::element::Element;
use crate::file::{IndexFile, PackOrigin};
use crate::issue::Issue;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

/// An element together with where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredElement {
    /// The element after `<append>` merging.
    pub element: Element,
    /// The file it was loaded from.
    pub file: Arc<str>,
    /// Core or custom load semantics of that file.
    pub origin: PackOrigin,
    /// True when an `<append>` changed it.
    pub extended: bool,
    /// The supports tags that `<append>`s added (not in the element's own definition), in the
    /// order they were added.
    pub appended_supports: Vec<String>,
}

/// An index file loaded alongside the packs (metadata only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedIndex {
    /// The file it was loaded from.
    pub file: String,
    /// The parsed index, including remote URLs.
    pub index: IndexFile,
}

/// All loaded content, with lookup indexes.
#[derive(Debug, Clone, Default)]
pub struct ContentStore {
    elements: Vec<StoredElement>,
    by_id: HashMap<String, usize>,
    /// `renamed_from` (previous or legacy id) -> element position; consulted after `by_id`.
    aliases: HashMap<String, usize>,
    by_type: BTreeMap<String, Vec<usize>>,
    by_source: BTreeMap<String, Vec<usize>>,
    by_tag: HashMap<String, Vec<usize>>,
    multiclass_ids: BTreeSet<String>,
    files: Vec<String>,
    packs: Vec<PackInfo>,
    indexes: Vec<LoadedIndex>,
    diagnostics: Vec<Issue>,
}

impl ContentStore {
    pub(crate) fn assemble(
        elements: Vec<StoredElement>,
        files: Vec<String>,
        packs: Vec<PackInfo>,
        indexes: Vec<LoadedIndex>,
        diagnostics: Vec<Issue>,
    ) -> Self {
        let mut by_id = HashMap::new();
        let mut aliases: HashMap<String, usize> = HashMap::new();
        let mut by_type: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_source: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_tag: HashMap<String, Vec<usize>> = HashMap::new();
        let mut multiclass_ids = BTreeSet::new();
        for (i, stored) in elements.iter().enumerate() {
            let e = &stored.element;
            by_id.insert(e.id.clone(), i);
            if let Some(old) = &e.renamed_from {
                aliases.entry(old.clone()).or_insert(i);
            }
            by_type.entry(e.ty.clone()).or_default().push(i);
            by_source.entry(e.source.clone()).or_default().push(i);
            for tag in &e.supports {
                by_tag.entry(tag.clone()).or_default().push(i);
            }
            if let Some(m) = &e.multiclass {
                multiclass_ids.insert(m.id.clone());
            }
        }
        Self {
            elements,
            by_id,
            aliases,
            by_type,
            by_source,
            by_tag,
            multiclass_ids,
            files,
            packs,
            indexes,
            diagnostics,
        }
    }

    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// True if no elements are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Look an element up by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&StoredElement> {
        self.by_id
            .get(id)
            .or_else(|| self.aliases.get(id))
            .map(|&i| &self.elements[i])
    }

    /// Look an element up by id and return just the element.
    #[must_use]
    pub fn element(&self, id: &str) -> Option<&Element> {
        self.get(id).map(|s| &s.element)
    }

    /// True if `id` is an element id or the id of a synthesized multiclass sibling.
    #[must_use]
    pub fn contains_id(&self, id: &str) -> bool {
        self.by_id.contains_key(id)
            || self.aliases.contains_key(id)
            || self.multiclass_ids.contains(id)
    }

    /// Iterate every element in load order.
    pub fn iter(&self) -> impl Iterator<Item = &StoredElement> {
        self.elements.iter()
    }

    fn pick<'a>(
        &'a self,
        positions: Option<&'a Vec<usize>>,
    ) -> impl Iterator<Item = &'a StoredElement> {
        positions
            .into_iter()
            .flat_map(|v| v.iter())
            .map(|&i| &self.elements[i])
    }

    /// Elements of one type (exact, case-sensitive), in load order.
    pub fn by_type(&self, ty: &str) -> impl Iterator<Item = &StoredElement> {
        self.pick(self.by_type.get(ty))
    }

    /// Elements from one source name, in load order.
    pub fn by_source(&self, source: &str) -> impl Iterator<Item = &StoredElement> {
        self.pick(self.by_source.get(source))
    }

    /// Elements whose explicit `supports` list contains `tag` (derived tags are not stored).
    pub fn with_tag(&self, tag: &str) -> impl Iterator<Item = &StoredElement> {
        self.pick(self.by_tag.get(tag))
    }

    /// Every distinct type string with its element count.
    #[must_use]
    pub fn type_counts(&self) -> BTreeMap<&str, usize> {
        self.by_type
            .iter()
            .map(|(k, v)| (k.as_str(), v.len()))
            .collect()
    }

    /// Every distinct source name used by an element.
    #[must_use]
    pub fn source_names(&self) -> Vec<&str> {
        self.by_source.keys().map(String::as_str).collect()
    }

    /// Candidates of `ty` that survive the source restriction (select rule semantics).
    pub fn candidates<'a>(
        &'a self,
        ty: &str,
        restriction: &'a SourceRestriction,
    ) -> impl Iterator<Item = &'a StoredElement> {
        self.by_type(ty)
            .filter(move |s| !restriction.is_restricted(&s.element.source))
    }

    /// Ids of every element whose source is unchecked.
    #[must_use]
    pub fn restricted_ids(&self, restriction: &SourceRestriction) -> HashSet<&str> {
        self.elements
            .iter()
            .filter(|s| restriction.is_restricted(&s.element.source))
            .map(|s| s.element.id.as_str())
            .collect()
    }

    /// Source names used by elements that no `Source` element defines (other than `Core`
    /// and `Internal`), so the UI can still list them as "undefined" sources.
    #[must_use]
    pub fn undefined_sources(&self) -> BTreeSet<&str> {
        const EXCLUDED_TYPES: [&str; 8] = [
            "Source",
            "Internal",
            "Core",
            "Ability Score Improvement",
            "Level",
            "Multiclass",
            "Skill",
            "Support",
        ];
        let defined: HashSet<&str> = self
            .by_type("Source")
            .map(|s| s.element.name.as_str())
            .collect();
        self.elements
            .iter()
            .filter(|s| !EXCLUDED_TYPES.contains(&s.element.ty.as_str()))
            .map(|s| s.element.source.as_str())
            .filter(|name| !matches!(*name, "Core" | "Internal") && !defined.contains(name))
            .collect()
    }

    /// Names of the files loaded, in load order.
    #[must_use]
    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// Manifests of the packs that were loaded.
    #[must_use]
    pub fn packs(&self) -> &[PackInfo] {
        &self.packs
    }

    /// Index files that were loaded (remote URLs are stored, never fetched).
    #[must_use]
    pub fn indexes(&self) -> &[LoadedIndex] {
        &self.indexes
    }

    /// Diagnostics produced while loading (duplicates, failed appends, ignored files).
    #[must_use]
    pub fn diagnostics(&self) -> &[Issue] {
        &self.diagnostics
    }

    /// Run the content validator over the loaded universe.
    #[must_use]
    pub fn validate(&self) -> Vec<Issue> {
        crate::validate::validate_store(self)
    }
}
