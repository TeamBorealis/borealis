//! Office documents (`.docx`, `.odt`) to text: paragraphs, headings (declared outline levels,
//! heading styles, else font size and bold) and tables (one `a | b | c` line per row).

use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use quick_xml::XmlVersion;

use super::layout::{to_markdown, without_continuations, TextLine};
use super::zip::read_entry;
use crate::error::AssistError;

/// Most XML nodes read from one part (a novel-length document has far fewer).
const MAX_NODES: usize = 4_000_000;
/// Deepest nesting accepted.
const MAX_DEPTH: usize = 256;

/// Office formats the importer reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeFormat {
    /// Word (`.docx`).
    Docx,
    /// OpenDocument text (`.odt`).
    Odt,
}

impl OfficeFormat {
    /// The format of a file name, by extension.
    pub fn of_name(name: &str) -> Option<Self> {
        let n = name.to_lowercase();
        if n.ends_with(".docx") {
            Some(Self::Docx)
        } else if n.ends_with(".odt") {
            Some(Self::Odt)
        } else {
            None
        }
    }
}

/// The text of an office document, headings as `#` lines.
///
/// # Errors
/// [`AssistError::Extraction`] when the file is not a readable document of that format or has no
/// text.
pub fn office_text(format: OfficeFormat, bytes: &[u8]) -> Result<String, AssistError> {
    let lines = match format {
        OfficeFormat::Docx => docx_lines(bytes)?,
        OfficeFormat::Odt => odt_lines(bytes)?,
    };
    let text = without_continuations(&to_markdown(&lines));
    if text.trim().is_empty() {
        return Err(AssistError::Extraction("the document has no text".into()));
    }
    Ok(text)
}

// ---- a small DOM -------------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Child>,
}

#[derive(Debug)]
enum Child {
    El(Node),
    Text(String),
}

impl Node {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
    fn elements(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter_map(|c| match c {
            Child::El(n) => Some(n),
            Child::Text(_) => None,
        })
    }
    fn child(&self, name: &str) -> Option<&Node> {
        self.elements().find(|n| n.name == name)
    }
    fn find(&self, name: &str) -> Option<&Node> {
        if self.name == name {
            return Some(self);
        }
        self.elements().find_map(|c| c.find(name))
    }
}

fn bad_xml(e: impl std::fmt::Display) -> AssistError {
    AssistError::Extraction(format!("the document XML could not be read: {e}"))
}

fn start(s: &BytesStart<'_>) -> Node {
    let attrs = s
        .attributes()
        .flatten()
        .map(|a| {
            let v = a
                .normalized_value(XmlVersion::Implicit1_0)
                .map(|c| c.into_owned())
                .unwrap_or_default();
            (a.key.as_ref().to_string(), v)
        })
        .collect();
    Node {
        name: s.name().as_ref().to_string(),
        attrs,
        children: Vec::new(),
    }
}

fn push(stack: &mut [Node], child: Child) {
    if let Some(top) = stack.last_mut() {
        top.children.push(child);
    }
}

fn parse(xml: &[u8]) -> Result<Node, AssistError> {
    let text = String::from_utf8_lossy(xml);
    let mut reader = Reader::from_str(&text);
    reader.config_mut().trim_text(false);
    let mut stack = vec![Node::default()];
    let mut nodes = 0usize;
    loop {
        let ev = reader.read_event().map_err(bad_xml)?;
        nodes += 1;
        if nodes > MAX_NODES || stack.len() > MAX_DEPTH {
            return Err(bad_xml("too large"));
        }
        match ev {
            Event::Start(s) => stack.push(start(&s)),
            Event::Empty(s) => push(&mut stack, Child::El(start(&s))),
            Event::End(_) if stack.len() > 1 => {
                let done = stack.pop().unwrap_or_default();
                push(&mut stack, Child::El(done));
            }
            Event::Text(t) => {
                push(&mut stack, Child::Text(t.xml10_content().into_owned()));
            }
            Event::CData(c) => {
                push(&mut stack, Child::Text(c.into_inner().into_owned()));
            }
            Event::GeneralRef(r) => {
                let s = match r.resolve_char_ref().ok().flatten() {
                    Some(c) => c.to_string(),
                    None => {
                        let name = r.into_inner().into_owned();
                        quick_xml::escape::resolve_predefined_entity(&name)
                            .unwrap_or_default()
                            .to_string()
                    }
                };
                push(&mut stack, Child::Text(s));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    while stack.len() > 1 {
        let done = stack.pop().unwrap_or_default();
        push(&mut stack, Child::El(done));
    }
    Ok(stack.pop().unwrap_or_default())
}

fn part(bytes: &[u8], name: &str) -> Result<Option<Node>, AssistError> {
    read_entry(bytes, name)?.map(|x| parse(&x)).transpose()
}

/// Text runs of one paragraph: (text, size, bold).
#[derive(Default)]
struct Para {
    runs: Vec<(String, f32, bool)>,
}

impl Para {
    fn line(&self, level: Option<u8>, base_size: f32, base_bold: bool) -> TextLine {
        let text: String = self.runs.iter().map(|r| r.0.as_str()).collect();
        let visible = |s: &str| s.chars().filter(|c| !c.is_whitespace()).count();
        let main = self.runs.iter().max_by_key(|r| visible(&r.0));
        let size = main.map(|r| r.1).filter(|s| *s > 0.0).unwrap_or(base_size);
        let bold = base_bold
            || (!self.runs.is_empty()
                && self.runs.iter().filter(|r| visible(&r.0) > 0).all(|r| r.2));
        TextLine {
            text: text.split_whitespace().collect::<Vec<_>>().join(" "),
            size,
            bold,
            level,
            gap_before: true,
        }
    }
}

fn row_line(cells: Vec<String>, first: bool) -> TextLine {
    TextLine {
        text: cells.join(" | "),
        gap_before: first,
        ..TextLine::default()
    }
}

// ---- DOCX --------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct DocxStyle {
    based_on: Option<String>,
    level: Option<u8>,
    size: Option<f32>,
    bold: Option<bool>,
}

fn on_off(n: &Node) -> bool {
    !matches!(n.attr("w:val"), Some("0" | "false" | "off" | "none"))
}

fn rpr_size(rpr: &Node) -> Option<f32> {
    rpr.child("w:sz")
        .and_then(|s| s.attr("w:val"))
        .and_then(|v| v.parse::<f32>().ok())
        .map(|half| half / 2.0)
}

fn heading_level_of_name(name: &str) -> Option<u8> {
    let n = name.to_lowercase();
    if n == "title" {
        return Some(1);
    }
    let rest = n
        .strip_prefix("heading")?
        .trim()
        .trim_start_matches(['_', ' ']);
    rest.parse::<u8>().ok().filter(|l| *l >= 1)
}

fn docx_styles(root: Option<&Node>) -> (HashMap<String, DocxStyle>, f32) {
    let mut map = HashMap::new();
    let Some(root) = root else {
        return (map, 0.0);
    };
    let default_size = root
        .find("w:docDefaults")
        .and_then(|d| d.find("w:rPr"))
        .and_then(rpr_size)
        .unwrap_or(0.0);
    for s in root
        .elements()
        .flat_map(|r| r.elements())
        .filter(|n| n.name == "w:style")
    {
        let Some(id) = s.attr("w:styleId") else {
            continue;
        };
        let name = s
            .child("w:name")
            .and_then(|n| n.attr("w:val"))
            .unwrap_or(id);
        let outline = s
            .child("w:pPr")
            .and_then(|p| p.child("w:outlineLvl"))
            .and_then(|o| o.attr("w:val"))
            .and_then(|v| v.parse::<u8>().ok())
            .map(|l| l + 1);
        let rpr = s.child("w:rPr");
        map.insert(
            id.to_string(),
            DocxStyle {
                based_on: s
                    .child("w:basedOn")
                    .and_then(|b| b.attr("w:val"))
                    .map(str::to_string),
                level: outline.or_else(|| heading_level_of_name(name)),
                size: rpr.and_then(rpr_size),
                bold: rpr.and_then(|r| r.child("w:b")).map(on_off),
            },
        );
    }
    (map, default_size)
}

fn resolve_docx(styles: &HashMap<String, DocxStyle>, id: &str) -> DocxStyle {
    let mut out = DocxStyle::default();
    let mut cur = Some(id.to_string());
    for _ in 0..16 {
        let Some(s) = cur.as_deref().and_then(|i| styles.get(i)) else {
            break;
        };
        out.level = out.level.or(s.level);
        out.size = out.size.or(s.size);
        out.bold = out.bold.or(s.bold);
        cur = s.based_on.clone();
    }
    out
}

fn docx_runs(n: &Node, para: &mut Para) {
    for c in n.elements() {
        match c.name.as_str() {
            "w:r" => {
                let rpr = c.child("w:rPr");
                let size = rpr.and_then(rpr_size).unwrap_or(0.0);
                let bold = rpr.and_then(|r| r.child("w:b")).is_some_and(on_off);
                let mut text = String::new();
                for t in c.elements() {
                    match t.name.as_str() {
                        "w:t" => text.extend(t.children.iter().filter_map(|x| match x {
                            Child::Text(s) => Some(s.as_str()),
                            Child::El(_) => None,
                        })),
                        "w:tab" => text.push('\t'),
                        "w:br" | "w:cr" => text.push(' '),
                        _ => {}
                    }
                }
                para.runs.push((text, size, bold));
            }
            "w:del" | "w:instrText" | "w:pPr" => {}
            _ => docx_runs(c, para),
        }
    }
}

struct DocxCtx {
    styles: HashMap<String, DocxStyle>,
    default_size: f32,
}

fn docx_para(p: &Node, ctx: &DocxCtx) -> TextLine {
    let ppr = p.child("w:pPr");
    let style = ppr
        .and_then(|x| x.child("w:pStyle"))
        .and_then(|s| s.attr("w:val"))
        .map(|id| resolve_docx(&ctx.styles, id))
        .unwrap_or_default();
    let own_level = ppr
        .and_then(|x| x.child("w:outlineLvl"))
        .and_then(|o| o.attr("w:val"))
        .and_then(|v| v.parse::<u8>().ok())
        .map(|l| l + 1);
    let mut para = Para::default();
    docx_runs(p, &mut para);
    let mut line = para.line(
        own_level.or(style.level),
        style.size.unwrap_or(ctx.default_size),
        style.bold.unwrap_or(false),
    );
    if ppr.and_then(|x| x.child("w:numPr")).is_some() && line.level.is_none() {
        line.text = format!("- {}", line.text);
    }
    line
}

fn docx_cell_text(n: &Node, ctx: &DocxCtx) -> String {
    n.elements()
        .filter(|p| p.name == "w:p")
        .map(|p| docx_para(p, ctx).text)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn docx_block(n: &Node, ctx: &DocxCtx, out: &mut Vec<TextLine>) {
    for c in n.elements() {
        match c.name.as_str() {
            "w:p" => out.push(docx_para(c, ctx)),
            "w:tbl" => {
                for (i, row) in c.elements().filter(|r| r.name == "w:tr").enumerate() {
                    let cells = row
                        .elements()
                        .filter(|x| x.name == "w:tc")
                        .map(|cell| docx_cell_text(cell, ctx))
                        .collect();
                    out.push(row_line(cells, i == 0));
                }
            }
            "w:sdt" | "w:sdtContent" | "w:customXml" => docx_block(c, ctx, out),
            _ => {}
        }
    }
}

fn docx_lines(bytes: &[u8]) -> Result<Vec<TextLine>, AssistError> {
    let doc = part(bytes, "word/document.xml")?
        .ok_or_else(|| AssistError::Extraction("not a Word (.docx) document".into()))?;
    let styles = part(bytes, "word/styles.xml")?;
    let (styles, default_size) = docx_styles(styles.as_ref());
    let ctx = DocxCtx {
        styles,
        default_size,
    };
    let body = doc
        .find("w:body")
        .ok_or_else(|| AssistError::Extraction("the document has no body".into()))?;
    let mut out = Vec::new();
    docx_block(body, &ctx, &mut out);
    Ok(out)
}

// ---- ODT ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct OdtStyle {
    parent: Option<String>,
    level: Option<u8>,
    size: Option<f32>,
    bold: Option<bool>,
}

fn points(v: &str) -> Option<f32> {
    let v = v.trim();
    let (num, unit) = v.split_at(v.find(|c: char| c.is_alphabetic() || c == '%')?);
    let n: f32 = num.parse().ok()?;
    match unit {
        "pt" => Some(n),
        "px" => Some(n * 0.75),
        "in" => Some(n * 72.0),
        "cm" => Some(n * 28.35),
        "mm" => Some(n * 2.835),
        _ => None,
    }
}

fn odt_styles(roots: &[&Node], map: &mut HashMap<String, OdtStyle>) {
    fn walk(n: &Node, map: &mut HashMap<String, OdtStyle>) {
        if n.name == "style:style" {
            if let Some(name) = n.attr("style:name") {
                let display = n.attr("style:display-name").unwrap_or(name);
                let tp = n.child("style:text-properties");
                map.insert(
                    name.to_string(),
                    OdtStyle {
                        parent: n.attr("style:parent-style-name").map(str::to_string),
                        level: n
                            .attr("style:default-outline-level")
                            .and_then(|v| v.parse().ok())
                            .or_else(|| heading_level_of_name(&display.replace("_20_", " "))),
                        size: tp.and_then(|t| t.attr("fo:font-size")).and_then(points),
                        bold: tp
                            .and_then(|t| t.attr("fo:font-weight"))
                            .map(|w| w == "bold" || w.parse::<u32>().is_ok_and(|x| x >= 600)),
                    },
                );
            }
            return;
        }
        for c in n.elements() {
            walk(c, map);
        }
    }
    for r in roots {
        walk(r, map);
    }
}

fn resolve_odt(styles: &HashMap<String, OdtStyle>, name: Option<&str>) -> OdtStyle {
    let mut out = OdtStyle::default();
    let mut cur = name.map(str::to_string);
    for _ in 0..16 {
        let Some(s) = cur.as_deref().and_then(|i| styles.get(i)) else {
            break;
        };
        out.level = out.level.or(s.level);
        out.size = out.size.or(s.size);
        out.bold = out.bold.or(s.bold);
        cur = s.parent.clone();
    }
    out
}

fn odt_runs(n: &Node, styles: &HashMap<String, OdtStyle>, inherited: (f32, bool), para: &mut Para) {
    for c in &n.children {
        let el = match c {
            Child::Text(t) => {
                para.runs.push((t.clone(), inherited.0, inherited.1));
                continue;
            }
            Child::El(el) => el,
        };
        match el.name.as_str() {
            "text:s" => {
                let count = el.attr("text:c").and_then(|v| v.parse().ok()).unwrap_or(1);
                para.runs
                    .push((" ".repeat(count), inherited.0, inherited.1));
            }
            "text:tab" => para.runs.push(("\t".into(), inherited.0, inherited.1)),
            "text:line-break" => para.runs.push((" ".into(), inherited.0, inherited.1)),
            "text:note" | "office:annotation" | "text:bookmark-ref" => {}
            _ => {
                let s = resolve_odt(styles, el.attr("text:style-name"));
                let here = (s.size.unwrap_or(inherited.0), s.bold.unwrap_or(inherited.1));
                odt_runs(el, styles, here, para);
            }
        }
    }
}

fn odt_para(n: &Node, styles: &HashMap<String, OdtStyle>) -> TextLine {
    let s = resolve_odt(styles, n.attr("text:style-name"));
    let level = if n.name == "text:h" {
        Some(
            n.attr("text:outline-level")
                .and_then(|v| v.parse().ok())
                .or(s.level)
                .unwrap_or(1),
        )
    } else {
        s.level
    };
    let base = (s.size.unwrap_or(0.0), s.bold.unwrap_or(false));
    let mut para = Para::default();
    odt_runs(n, styles, base, &mut para);
    para.line(level, base.0, base.1)
}

fn odt_cell_text(n: &Node, styles: &HashMap<String, OdtStyle>) -> String {
    let mut lines = Vec::new();
    odt_block(n, styles, &mut lines, false);
    lines
        .into_iter()
        .map(|l| l.text)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn odt_block(n: &Node, styles: &HashMap<String, OdtStyle>, out: &mut Vec<TextLine>, list: bool) {
    for c in n.elements() {
        match c.name.as_str() {
            "text:h" | "text:p" => {
                let mut line = odt_para(c, styles);
                if list && line.level.is_none() && !line.text.is_empty() {
                    line.text = format!("- {}", line.text);
                }
                out.push(line);
            }
            "table:table" => {
                let rows: Vec<&Node> = table_rows(c);
                for (i, row) in rows.into_iter().enumerate() {
                    let cells = row
                        .elements()
                        .filter(|x| x.name == "table:table-cell")
                        .map(|cell| odt_cell_text(cell, styles))
                        .collect();
                    out.push(row_line(cells, i == 0));
                }
            }
            "text:list" | "text:list-item" | "text:list-header" => odt_block(c, styles, out, true),
            "text:section" | "text:index-body" | "draw:frame" | "draw:text-box" => {
                odt_block(c, styles, out, list);
            }
            _ => {}
        }
    }
}

fn table_rows(t: &Node) -> Vec<&Node> {
    let mut rows = Vec::new();
    for c in t.elements() {
        match c.name.as_str() {
            "table:table-row" => rows.push(c),
            "table:table-header-rows" | "table:table-rows" | "table:table-row-group" => {
                rows.extend(table_rows(c));
            }
            _ => {}
        }
    }
    rows
}

fn odt_lines(bytes: &[u8]) -> Result<Vec<TextLine>, AssistError> {
    let content = part(bytes, "content.xml")?
        .ok_or_else(|| AssistError::Extraction("not an OpenDocument (.odt) document".into()))?;
    let styles_part = part(bytes, "styles.xml")?;
    let mut styles = HashMap::new();
    let mut roots: Vec<&Node> = vec![&content];
    if let Some(s) = styles_part.as_ref() {
        roots.insert(0, s);
    }
    odt_styles(&roots, &mut styles);
    let text = content
        .find("office:text")
        .ok_or_else(|| AssistError::Extraction("the document has no text body".into()))?;
    let mut out = Vec::new();
    odt_block(text, &styles, &mut out, false);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::testing::synthetic_zip as write_zip;

    #[test]
    fn docx_headings_styles_and_tables() {
        let styles = r#"<w:styles xmlns:w="w"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="20"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:styleId="H1"><w:name w:val="heading 1"/><w:rPr><w:sz w:val="36"/></w:rPr></w:style>
<w:style w:styleId="Big"><w:name w:val="Fancy"/><w:rPr><w:sz w:val="32"/><w:b/></w:rPr></w:style></w:styles>"#;
        let doc = r#"<w:document xmlns:w="w"><w:body>
<w:p><w:pPr><w:pStyle w:val="H1"/></w:pPr><w:r><w:t>Glimmer Folk</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">They hum &amp; glow. </w:t></w:r><w:r><w:t>Softly.</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Big"/></w:pPr><w:r><w:t>Lantern Sense</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>d4</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Hue</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Amber</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
</w:body></w:document>"#;
        let z = write_zip(&[
            ("word/document.xml", doc.as_bytes()),
            ("word/styles.xml", styles.as_bytes()),
        ]);
        let t = office_text(OfficeFormat::Docx, &z).unwrap();
        assert!(t.starts_with("# Glimmer Folk\n"), "{t}");
        assert!(t.contains("They hum & glow. Softly."), "{t}");
        assert!(t.contains("## Lantern Sense"), "{t}");
        assert!(t.contains("d4 | Hue\n1 | Amber"), "{t}");
    }

    #[test]
    fn odt_outline_levels_spans_and_tables() {
        let styles = r#"<office:document-styles><office:styles>
<style:style style:name="Heading_20_2" style:display-name="Heading 2" style:default-outline-level="2"/>
<style:style style:name="Body"><style:text-properties fo:font-size="10pt"/></style:style>
</office:styles></office:document-styles>"#;
        let content = r#"<office:document-content><office:automatic-styles>
<style:style style:name="P9" style:parent-style-name="Body"><style:text-properties fo:font-size="18pt" fo:font-weight="bold"/></style:style>
</office:automatic-styles><office:body><office:text>
<text:h text:outline-level="1">Moss Wardens</text:h>
<text:p text:style-name="Body">Keepers<text:s text:c="2"/>of the deep grove.</text:p>
<text:p text:style-name="P9">Bark Skin</text:p>
<text:p text:style-name="Heading_20_2">Grove Step</text:p>
<text:list><text:list-item><text:p text:style-name="Body">first point</text:p></text:list-item></text:list>
<table:table><table:table-row><table:table-cell><text:p>Level</text:p></table:table-cell><table:table-cell><text:p>Gift</text:p></table:table-cell></table:table-row></table:table>
</office:text></office:body></office:document-content>"#;
        let z = write_zip(&[
            ("content.xml", content.as_bytes()),
            ("styles.xml", styles.as_bytes()),
        ]);
        let t = office_text(OfficeFormat::Odt, &z).unwrap();
        assert!(t.starts_with("# Moss Wardens\n"), "{t}");
        assert!(t.contains("Keepers of the deep grove."), "{t}");
        assert!(
            t.contains("# Bark Skin") && !t.contains("# Bark Skin\n\n# Grove"),
            "{t}"
        );
        assert!(t.contains("## Grove Step"), "{t}");
        assert!(t.contains("- first point"), "{t}");
        assert!(t.contains("Level | Gift"), "{t}");
        assert!(office_text(OfficeFormat::Odt, &write_zip(&[("x", b"y")])).is_err());
    }
}
