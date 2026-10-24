//! Tagged-PDF pass: turns the page op streams of the layout into a structure tree.
//!
//! Layout code only states intent through markers ([`PageOp::Group`], [`PageOp::Leaf`], the
//! `Canvas::heading`/`label` helpers and `Canvas::artifact`). This pass runs once over all pages:
//! it numbers the marked content (MCIDs), builds `Document > Sect (per page) > ...` with
//! [`StructBuilder`], and gives everything that carries no marker a default role so that nothing
//! stays outside the tree: text runs become `P`, vector drawing becomes an `/Artifact`, images
//! `Figure`, form fields `Form` elements with their widget (fillable) or a `P` (flattened).

use crate::pdf::{FormMode, PageOp, PdfPage};
use crate::pdf_form::{is_widget, FormField};
use crate::pdf_tags::{LeafOpts, Scope, StructBuilder};

/// Group role of the character name; only the first one in the document stays a heading.
const NAME_ROLE: &str = "H1";

#[derive(PartialEq, Clone, Copy)]
enum Open {
    None,
    Text,
    Artifact,
}

struct PageTagger<'a> {
    b: &'a mut StructBuilder,
    page: usize,
    mode: FormMode,
    out: Vec<PageOp>,
    open: Open,
    /// Open explicit marked content, innermost last; `false` marks a dropped (nested) leaf.
    marks: Vec<bool>,
    /// Tags of the open groups, innermost last.
    groups: Vec<&'static str>,
    /// An implicit `TD` wraps the current content item (content directly inside a `TR`).
    cell: bool,
    fields: usize,
}

impl PageTagger<'_> {
    fn close_run(&mut self) {
        if self.open != Open::None {
            self.out.push(PageOp::EndMarked);
            self.open = Open::None;
        }
        self.end_cell();
    }

    /// Content directly inside a `TR` gets its own `TD`.
    fn begin_cell(&mut self) {
        if !self.cell && self.groups.last() == Some(&"TR") {
            self.b.begin("TD");
            self.groups.push("TD");
            self.cell = true;
        }
    }

    fn end_cell(&mut self) {
        if self.cell {
            self.b.end();
            self.groups.pop();
            self.cell = false;
        }
    }

    fn begin_leaf(&mut self, tag: &str, opts: LeafOpts) {
        let mcid = self.b.leaf_with(tag, self.page, opts);
        self.out.push(PageOp::BeginTag {
            tag: tag.to_string(),
            mcid: Some(mcid),
        });
    }

    fn start_text(&mut self) {
        if self.open != Open::Text {
            self.close_run();
            self.begin_cell();
            self.begin_leaf("P", LeafOpts::default());
            self.open = Open::Text;
        }
    }

    fn start_artifact(&mut self) {
        if self.open != Open::Artifact {
            self.close_run();
            self.out.push(PageOp::BeginArtifact);
            self.open = Open::Artifact;
        }
    }

    fn field(&mut self, f: FormField) {
        self.close_run();
        self.begin_cell();
        match self.mode {
            FormMode::Fillable if is_widget(&f) => {
                let tip = f.tooltip.clone();
                let key = self.b.widget(self.page, self.fields, tip.as_deref());
                self.fields += 1;
                self.out.push(PageOp::BeginArtifact);
                self.out.push(PageOp::Field(FormField {
                    struct_parent: Some(key),
                    ..f
                }));
                self.out.push(PageOp::EndMarked);
            }
            FormMode::Fillable | FormMode::Flatten => {
                self.begin_leaf("P", LeafOpts::default());
                self.out.push(PageOp::Field(f));
                self.out.push(PageOp::EndMarked);
            }
        }
        self.end_cell();
    }

    fn explicit_leaf(&mut self, tag: &str, actual: Option<String>) {
        self.close_run();
        if tag != "TH" {
            self.begin_cell();
        }
        let scope = (tag == "TH").then_some(Scope::Column);
        self.begin_leaf(
            tag,
            LeafOpts {
                actual_text: actual,
                scope,
                ..LeafOpts::default()
            },
        );
        self.marks.push(true);
    }

    fn op(&mut self, op: PageOp, name_seen: &mut bool) {
        match op {
            PageOp::Group(tag) => {
                self.close_run();
                let tag = if tag == NAME_ROLE && std::mem::replace(name_seen, true) {
                    "P"
                } else {
                    tag
                };
                self.b.begin(tag);
                self.groups.push(tag);
            }
            PageOp::GroupEnd => {
                self.close_run();
                if self.groups.pop().is_some() {
                    self.b.end();
                }
            }
            PageOp::Leaf { tag, actual } if self.marks.is_empty() => {
                self.explicit_leaf(tag, actual);
            }
            PageOp::Leaf { .. } => self.marks.push(false),
            PageOp::BeginArtifact | PageOp::BeginTag { .. } => {
                self.close_run();
                self.marks.push(true);
                self.out.push(op);
            }
            PageOp::EndMarked => {
                if self.marks.pop().unwrap_or(true) {
                    self.out.push(op);
                }
                if self.marks.is_empty() {
                    self.end_cell();
                }
            }
            _ if !self.marks.is_empty() => self.out.push(op),
            PageOp::Text { .. } => {
                self.start_text();
                self.out.push(op);
            }
            PageOp::Rect { .. }
            | PageOp::Line { .. }
            | PageOp::Path { .. }
            | PageOp::Art { .. } => {
                self.start_artifact();
                self.out.push(op);
            }
            PageOp::Image { alt, .. } => {
                self.close_run();
                self.begin_leaf(
                    "Figure",
                    LeafOpts {
                        alt: Some(alt.to_string()),
                        ..LeafOpts::default()
                    },
                );
                self.out.push(op);
                self.out.push(PageOp::EndMarked);
            }
            PageOp::Field(f) => self.field(f),
            // graphics-state operators: never inside a marked run that spans them
            PageOp::Save | PageOp::Restore | PageOp::Clip { .. } | PageOp::Opacity(_) => {
                self.close_run();
                self.out.push(op);
            }
        }
    }
}

/// Tag every page: rewrite the ops in place and return the structure tree. Call after the last
/// page (footers included) exists.
pub(crate) fn tag_pages(pages: &mut [PdfPage], mode: FormMode) -> StructBuilder {
    let mut b = StructBuilder::new();
    b.begin("Document");
    let mut name_seen = false;
    for (page_no, page) in pages.iter_mut().enumerate() {
        b.begin("Sect");
        let mut t = PageTagger {
            b: &mut b,
            page: page_no,
            mode,
            out: Vec::with_capacity(page.ops.len() + page.ops.len() / 4),
            open: Open::None,
            marks: Vec::new(),
            groups: Vec::new(),
            cell: false,
            fields: 0,
        };
        for op in std::mem::take(&mut page.ops) {
            t.op(op, &mut name_seen);
        }
        t.close_run();
        while t.groups.pop().is_some() {
            t.b.end();
        }
        page.ops = t.out;
        b.end();
        page.struct_parents = Some(b.page_key(page_no));
    }
    b.end();
    b
}
