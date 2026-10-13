//! Tagged-PDF structure tree: [`StructBuilder`] implements `pdf::StructureWriter`.
//!
//! # Contract for layout code
//!
//! 1. While emitting ops, grow the tree with the builder: [`StructBuilder::begin`]/[`end`]
//!    for grouping elements (`Document`, `Sect`, `Table`, `TR`, `L`, `LI`, `Figure`...),
//!    [`StructBuilder::leaf`] for content items (`H1`, `P`, `TH`, `TD`, `Lbl`, `LBody`, `Span`)
//!    and [`StructBuilder::widget`] for form fields.
//! 2. `leaf` returns the page-local MCID: emit `PageOp::BeginTag { tag, mcid: Some(mcid) }`
//!    (same tag), the drawing ops, then `PageOp::EndMarked`.
//! 3. Every content op must sit inside such a `BeginTag`/`EndMarked` pair or inside a
//!    `PageOp::BeginArtifact`/`EndMarked` pair (ornaments, rules, backgrounds, frames).
//! 4. `widget` returns the key for `FormField::struct_parent`. The tooltip (`/TU`) comes from
//!    `FormField::with_tooltip`; `widget`'s `alt` is the structure element's `/Alt`.
//! 5. Set `PdfPage::struct_parents = builder.page_key(page)` for every page that has marked
//!    content (calling it for any page is harmless), and
//!    `PdfDocument::structure = Some(&builder)` once layout is finished.
//!
//! `pdf.rs` writes `/Lang` from `PdfDocument::lang`; this module adds `/StructTreeRoot` and
//! `/MarkInfo << /Marked true >>`.
//!
//! [`end`]: StructBuilder::end

use crate::pdf::{utf16_hex_string, DocRefs, ObjectSink, StructureWriter};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Standard structure types that need no `/RoleMap` entry.
const STANDARD_TAGS: &[&str] = &[
    "Document",
    "Part",
    "Sect",
    "Div",
    "Art",
    "BlockQuote",
    "Caption",
    "TOC",
    "TOCI",
    "Index",
    "NonStruct",
    "Private",
    "P",
    "H",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "L",
    "LI",
    "Lbl",
    "LBody",
    "Table",
    "TR",
    "TH",
    "TD",
    "THead",
    "TBody",
    "TFoot",
    "Span",
    "Quote",
    "Note",
    "Reference",
    "BibEntry",
    "Code",
    "Link",
    "Annot",
    "Figure",
    "Formula",
    "Form",
];

/// `/Scope` of a table header cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Row and Both complete the PDF scope set
pub(crate) enum Scope {
    Row,
    Column,
    Both,
}

impl Scope {
    fn name(self) -> &'static str {
        match self {
            Scope::Row => "Row",
            Scope::Column => "Column",
            Scope::Both => "Both",
        }
    }
}

/// Optional attributes of a leaf element.
#[derive(Debug, Clone, Default)]
pub(crate) struct LeafOpts {
    /// `/Alt` (figures, decorative groups with meaning).
    pub alt: Option<String>,
    /// `/ActualText` (replacement text, for example symbols or split words).
    pub actual_text: Option<String>,
    /// `/A << /O /Table /Scope .. >>` for `TH`.
    pub scope: Option<Scope>,
}

#[derive(Debug, Clone)]
enum Kind {
    Group,
    /// One marked-content item `mcid` on `page`.
    Leaf {
        page: usize,
        mcid: u32,
    },
    /// Widget annotation `index` (in `PageOp::Field` order) on `page`.
    Widget {
        page: usize,
        index: usize,
    },
}

#[derive(Debug, Clone)]
struct Node {
    tag: String,
    kind: Kind,
    kids: Vec<usize>,
    opts: LeafOpts,
}

/// Builds the structure tree while layout emits ops.
#[derive(Debug, Default)]
pub(crate) struct StructBuilder {
    nodes: Vec<Node>,
    /// Open group elements (innermost last).
    open: Vec<usize>,
    /// Top-level elements.
    roots: Vec<usize>,
    /// Next MCID per page.
    next_mcid: BTreeMap<usize, u32>,
    /// `/StructParents` key per page.
    page_keys: BTreeMap<usize, u32>,
    next_key: u32,
    /// MCID -> element (indexed by MCID) per page.
    page_items: BTreeMap<usize, Vec<usize>>,
    /// Widget `/StructParent` key -> element.
    widget_items: BTreeMap<u32, usize>,
    roles: BTreeMap<String, String>,
}

impl StructBuilder {
    pub fn new() -> StructBuilder {
        StructBuilder::default()
    }

    /// Map a custom tag to a standard structure type (`/RoleMap`).
    #[cfg_attr(not(test), allow(dead_code))] // exercised by tests; kept as API
    pub fn map_role(&mut self, custom: &str, standard: &str) {
        self.roles.insert(custom.to_string(), standard.to_string());
    }

    /// Open a grouping element under the current one.
    pub fn begin(&mut self, tag: &str) {
        let id = self.push(Node::new(tag, Kind::Group, LeafOpts::default()));
        self.open.push(id);
    }

    /// Close the innermost open grouping element. Unbalanced calls are ignored.
    pub fn end(&mut self) {
        self.open.pop();
    }

    /// Add a content item on `page` and return its fresh page-local MCID.
    #[cfg_attr(not(test), allow(dead_code))] // exercised by tests; kept as API
    pub fn leaf(&mut self, tag: &str, page: usize) -> u32 {
        self.leaf_with(tag, page, LeafOpts::default())
    }

    /// [`StructBuilder::leaf`] with `/Alt`, `/ActualText` or table scope.
    pub fn leaf_with(&mut self, tag: &str, page: usize, opts: LeafOpts) -> u32 {
        let counter = self.next_mcid.entry(page).or_insert(0);
        let mcid = *counter;
        *counter += 1;
        let id = self.push(Node::new(tag, Kind::Leaf { page, mcid }, opts));
        self.page_items.entry(page).or_default().push(id);
        mcid
    }

    /// Attach widget `index` (its position among the page's `PageOp::Field`s) as an object
    /// reference under a `Form` element. Returns the key for `FormField::struct_parent`.
    pub fn widget(&mut self, page: usize, index: usize, alt: Option<&str>) -> u32 {
        let opts = LeafOpts {
            alt: alt.map(str::to_string),
            ..LeafOpts::default()
        };
        let id = self.push(Node::new("Form", Kind::Widget { page, index }, opts));
        let key = self.alloc_key();
        self.widget_items.insert(key, id);
        key
    }

    /// The `/StructParents` key of `page`, allocated on first use.
    pub fn page_key(&mut self, page: usize) -> u32 {
        if let Some(&k) = self.page_keys.get(&page) {
            return k;
        }
        let k = self.alloc_key();
        self.page_keys.insert(page, k);
        k
    }

    fn alloc_key(&mut self) -> u32 {
        let k = self.next_key;
        self.next_key += 1;
        k
    }

    fn push(&mut self, node: Node) -> usize {
        let id = self.nodes.len();
        self.nodes.push(node);
        match self.open.last() {
            Some(&parent) => self.nodes[parent].kids.push(id),
            None => self.roots.push(id),
        }
        id
    }

    fn role_map(&self) -> BTreeMap<&str, &str> {
        let mut map: BTreeMap<&str, &str> = self
            .roles
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        for n in &self.nodes {
            if !STANDARD_TAGS.contains(&n.tag.as_str()) {
                map.entry(n.tag.as_str()).or_insert("Span");
            }
        }
        map
    }

    fn element(&self, id: usize, ids: &[usize], parent: usize, refs: &DocRefs) -> String {
        let node = &self.nodes[id];
        let mut d = format!("<< /Type /StructElem /S /{} /P {parent} 0 R", node.tag);
        let page_of = |p: usize| refs.pages.get(p).copied();
        match node.kind {
            Kind::Group => {
                let kids: String = node
                    .kids
                    .iter()
                    .map(|&k| format!("{} 0 R ", ids[k]))
                    .collect();
                let _ = write!(d, " /K [ {kids}]");
            }
            Kind::Leaf { page, mcid } => {
                if let Some(pg) = page_of(page) {
                    let _ = write!(d, " /Pg {pg} 0 R");
                }
                let _ = write!(d, " /K {mcid}");
            }
            Kind::Widget { page, index } => {
                let pg = page_of(page);
                let obj = refs.widgets.get(page).and_then(|w| w.get(index)).copied();
                if let (Some(pg), Some(obj)) = (pg, obj) {
                    let _ = write!(
                        d,
                        " /Pg {pg} 0 R /K << /Type /OBJR /Pg {pg} 0 R /Obj {obj} 0 R >>"
                    );
                }
            }
        }
        if let Some(alt) = &node.opts.alt {
            let _ = write!(d, " /Alt {}", utf16_hex_string(alt));
        }
        if let Some(t) = &node.opts.actual_text {
            let _ = write!(d, " /ActualText {}", utf16_hex_string(t));
        }
        if let Some(s) = node.opts.scope {
            let _ = write!(d, " /A << /O /Table /Scope /{} >>", s.name());
        }
        d.push_str(" >>");
        d
    }
}

impl Node {
    fn new(tag: &str, kind: Kind, opts: LeafOpts) -> Node {
        Node {
            tag: tag.to_string(),
            kind,
            kids: Vec::new(),
            opts,
        }
    }
}

impl StructureWriter for StructBuilder {
    fn write(&self, sink: &mut ObjectSink<'_>, refs: &DocRefs) -> String {
        let root = sink.alloc();
        let parent_tree = sink.alloc();
        let ids: Vec<usize> = self.nodes.iter().map(|_| sink.alloc()).collect();
        // Parent of every node (the tree root for top-level elements).
        let mut parent = vec![root; self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate() {
            for &k in &n.kids {
                parent[k] = ids[i];
            }
        }
        for i in 0..self.nodes.len() {
            sink.object(ids[i], &self.element(i, &ids, parent[i], refs));
        }

        // ParentTree: page key -> [elements by MCID]; widget key -> element.
        let mut nums: BTreeMap<u32, String> = BTreeMap::new();
        for (page, items) in &self.page_items {
            let Some(&key) = self.page_keys.get(page) else {
                continue;
            };
            let arr: String = items.iter().map(|&n| format!("{} 0 R ", ids[n])).collect();
            nums.insert(key, format!("[ {arr}]"));
        }
        for (&key, &n) in &self.widget_items {
            nums.insert(key, format!("{} 0 R", ids[n]));
        }
        let nums: String = nums.iter().map(|(k, v)| format!("{k} {v} ")).collect();
        sink.object(parent_tree, &format!("<< /Nums [ {nums}] >>"));

        let kids: String = self
            .roots
            .iter()
            .map(|&r| format!("{} 0 R ", ids[r]))
            .collect();
        let mut dict = format!(
            "<< /Type /StructTreeRoot /K [ {kids}] /ParentTree {parent_tree} 0 R \
             /ParentTreeNextKey {}",
            self.next_key
        );
        let roles = self.role_map();
        if !roles.is_empty() {
            let entries: String = roles.iter().map(|(k, v)| format!("/{k} /{v} ")).collect();
            let _ = write!(dict, " /RoleMap << {entries}>>");
        }
        dict.push_str(" >>");
        sink.object(root, &dict);
        format!("/StructTreeRoot {root} 0 R /MarkInfo << /Marked true >>")
    }
}

#[cfg(test)]
#[path = "pdf_tags_tests.rs"]
mod tests;
