//! The content-stream walker: positioned text runs (by the text and graphics matrices, through
//! Form XObjects) and the boxes where images are painted.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};

use super::pdf_fonts::{shown, FontCache, FontInfo, Spacing};

/// Most decompressed content read from one page or form (flate-bomb guard).
const MAX_CONTENT: usize = 32 * 1024 * 1024;
/// Deepest Form XObject nesting followed.
const MAX_FORM_DEPTH: usize = 8;

pub(crate) type Matrix = [f32; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn translate(tx: f32, ty: f32) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, tx, ty]
}

/// A shown string at its place on the page.
#[derive(Debug, Clone)]
pub(crate) struct Run {
    pub x: f32,
    pub y: f32,
    pub end: f32,
    pub size: f32,
    pub bold: bool,
    pub text: String,
    pub font: Vec<u8>,
}

/// Where an image is painted (page space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ImageBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl ImageBox {
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }
}

/// What a page paints.
#[derive(Debug, Default)]
pub(crate) struct Painted {
    pub runs: Vec<Run>,
    pub images: Vec<ImageBox>,
}

#[derive(Clone)]
struct State {
    ctm: Matrix,
    font: Vec<u8>,
    size: f32,
    leading: f32,
    char_space: f32,
    word_space: f32,
    scale: f32,
}

fn num(o: &Object) -> f32 {
    o.as_float().unwrap_or(0.0)
}

fn nums<const N: usize>(ops: &[Object]) -> Option<[f32; N]> {
    let mut out = [0.0; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = num(ops.get(i)?);
    }
    Some(out)
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    doc.dereference(o).map_or(o, |(_, x)| x)
}

struct Walker<'a> {
    doc: &'a Document,
    cache: &'a mut FontCache,
    budget: &'a mut usize,
    out: Painted,
    seen_forms: HashSet<ObjectId>,
}

impl<'a> Walker<'a> {
    fn xobject(
        &self,
        resources: &[&'a Dictionary],
        name: &[u8],
    ) -> Option<(ObjectId, &'a lopdf::Stream)> {
        resources.iter().find_map(|r| {
            let xs = deref(self.doc, r.get(b"XObject").ok()?).as_dict().ok()?;
            let id = xs.get(name).ok()?.as_reference().ok()?;
            Some((id, self.doc.get_object(id).ok()?.as_stream().ok()?))
        })
    }

    fn do_xobject(&mut self, resources: &[&'a Dictionary], name: &[u8], st: &State, depth: usize) {
        let Some((id, stream)) = self.xobject(resources, name) else {
            return;
        };
        match stream.dict.get(b"Subtype").and_then(Object::as_name).ok() {
            Some(b"Image") => {
                let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
                    .map(|(u, v)| mul(&translate(u, v), &st.ctm));
                let xs = corners.map(|m| m[4]);
                let ys = corners.map(|m| m[5]);
                self.out.images.push(ImageBox {
                    x0: xs.iter().copied().fold(f32::MAX, f32::min),
                    y0: ys.iter().copied().fold(f32::MAX, f32::min),
                    x1: xs.iter().copied().fold(f32::MIN, f32::max),
                    y1: ys.iter().copied().fold(f32::MIN, f32::max),
                });
            }
            Some(b"Form") if depth < MAX_FORM_DEPTH && self.seen_forms.insert(id) => {
                let Ok(data) = stream
                    .decompressed_content_with_limit(MAX_CONTENT)
                    .or_else(|_| {
                        if stream.dict.get(b"Filter").is_err() {
                            Ok(stream.content.clone())
                        } else {
                            Err(())
                        }
                    })
                else {
                    self.seen_forms.remove(&id);
                    return;
                };
                let m = stream
                    .dict
                    .get(b"Matrix")
                    .ok()
                    .and_then(|o| deref(self.doc, o).as_array().ok())
                    .and_then(|a| nums::<6>(a))
                    .unwrap_or(IDENTITY);
                let mut inner: Vec<&'a Dictionary> = Vec::new();
                if let Some(own) = stream
                    .dict
                    .get(b"Resources")
                    .ok()
                    .and_then(|o| deref(self.doc, o).as_dict().ok())
                {
                    inner.push(own);
                }
                inner.extend_from_slice(resources);
                let mut start = st.clone();
                start.ctm = mul(&m, &st.ctm);
                self.walk(&data, &inner, start, depth + 1);
                self.seen_forms.remove(&id);
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_lines)]
    fn walk(&mut self, data: &[u8], resources: &[&'a Dictionary], start: State, depth: usize) {
        let Ok(content) = Content::decode(data) else {
            return;
        };
        let fonts: HashMap<Vec<u8>, Rc<FontInfo>> = self.cache.fonts_of(self.doc, resources);
        let mut stack: Vec<State> = Vec::new();
        let mut st = start;
        let (mut tm, mut lm) = (IDENTITY, IDENTITY);
        for op in &content.operations {
            let o = &op.operands;
            match op.operator.as_str() {
                "q" => stack.push(st.clone()),
                "Q" => st = stack.pop().unwrap_or(st),
                "cm" => {
                    if let Some(m) = nums::<6>(o) {
                        st.ctm = mul(&m, &st.ctm);
                    }
                }
                "Do" => {
                    if let Some(name) = o.first().and_then(|n| n.as_name().ok()) {
                        let name = name.to_vec();
                        self.do_xobject(resources, &name, &st, depth);
                    }
                }
                "BT" => (tm, lm) = (IDENTITY, IDENTITY),
                "Tf" => {
                    st.font = o
                        .first()
                        .and_then(|f| f.as_name().ok())
                        .unwrap_or_default()
                        .to_vec();
                    st.size = o.get(1).map(num).unwrap_or(0.0);
                }
                "TL" => st.leading = o.first().map(num).unwrap_or(0.0),
                "Tc" => st.char_space = o.first().map(num).unwrap_or(0.0),
                "Tw" => st.word_space = o.first().map(num).unwrap_or(0.0),
                "Tz" => st.scale = o.first().map_or(100.0, num) / 100.0,
                "Td" | "TD" => {
                    if let Some([tx, ty]) = nums::<2>(o) {
                        if op.operator == "TD" {
                            st.leading = -ty;
                        }
                        lm = mul(&translate(tx, ty), &lm);
                        tm = lm;
                    }
                }
                "Tm" => {
                    if let Some(m) = nums::<6>(o) {
                        (tm, lm) = (m, m);
                    }
                }
                "T*" => {
                    lm = mul(&translate(0.0, -st.leading), &lm);
                    tm = lm;
                }
                "Tj" | "TJ" | "'" | "\"" => {
                    if op.operator != "Tj" && op.operator != "TJ" {
                        lm = mul(&translate(0.0, -st.leading), &lm);
                        tm = lm;
                    }
                    let strings = if op.operator == "\"" {
                        o.get(2..).unwrap_or_default()
                    } else {
                        o
                    };
                    let font = fonts.get(&st.font).map(Rc::as_ref);
                    let sp = Spacing {
                        size: st.size,
                        char_space: st.char_space,
                        word_space: st.word_space,
                        scale: st.scale,
                    };
                    let (text, advance) = shown(font, strings, &sp);
                    let m = mul(&tm, &st.ctm);
                    let scale = (m[2] * m[2] + m[3] * m[3]).sqrt();
                    let size = (st.size * scale).abs() * font.map_or(1.0, |f| f.size_scale);
                    let width = advance * (m[0] * m[0] + m[1] * m[1]).sqrt();
                    tm = mul(&translate(advance, 0.0), &tm);
                    if text.trim().is_empty() || *self.budget == 0 {
                        continue;
                    }
                    *self.budget = self.budget.saturating_sub(text.len());
                    self.out.runs.push(Run {
                        x: m[4],
                        y: m[5],
                        end: m[4] + width,
                        size,
                        bold: font.is_some_and(|f| f.bold),
                        text,
                        font: st.font.clone(),
                    });
                }
                _ => {}
            }
        }
    }
}

/// Everything page `page` paints (text runs and image boxes).
pub(crate) fn paint_page(
    doc: &Document,
    page: ObjectId,
    cache: &mut FontCache,
    budget: &mut usize,
) -> Painted {
    let Ok(data) = doc.get_page_content_with_limit(page, MAX_CONTENT) else {
        return Painted::default();
    };
    let mut resources: Vec<&Dictionary> = Vec::new();
    if let Ok((own, inherited)) = doc.get_page_resources(page) {
        resources.extend(own);
        resources.extend(
            inherited
                .iter()
                .filter_map(|id| doc.get_dictionary(*id).ok()),
        );
    }
    let mut w = Walker {
        doc,
        cache,
        budget,
        out: Painted::default(),
        seen_forms: HashSet::new(),
    };
    let start = State {
        ctm: IDENTITY,
        font: Vec::new(),
        size: 0.0,
        leading: 0.0,
        char_space: 0.0,
        word_space: 0.0,
        scale: 1.0,
    };
    w.walk(&data, &resources, start, 0);
    w.out
}
