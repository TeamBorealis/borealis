//! The Obsidian syntax the backend needs for backlinks, the tag list and link updates: YAML front matter, `[[wikilinks]]`
//! and `![[embeds]]`, Markdown links to vault files, and `#tags`. CommonMark itself is parsed by pulldown-cmark, only to
//! find the inline code and code blocks, where none of this syntax counts (Obsidian ignores it there too). Front matter
//! is read with yaml-rust2.
//!
//! The UI's renderer (`ui/src/pages/notes/markdown/`) follows the same rules.

use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, Options, Parser, Tag};
use regex::Regex;
use std::ops::Range;
use std::sync::LazyLock;

/// `[[target#sub|display]]`, optionally `!`-prefixed. No brackets or line breaks inside.
static WIKILINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(!?)\[\[([^\[\]\r\n]+)\]\]").expect("wikilink regex"));
/// `[text](target "title")` / `[text](<target with spaces>)`, optionally `!`-prefixed.
static MD_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(!?)\[[^\[\]\r\n]*\]\(\s*(<[^<>\r\n]*>|[^\s()<>]+)(?:\s+"[^"\r\n]*")?\s*\)"#)
        .expect("markdown link regex")
});
/// `#tag` after whitespace or at a line start: letters, digits, `_`, `-`, `/`, with at least one non-digit.
static TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)(?:^|\s)#([\p{L}\p{N}\p{M}_/\-]*[\p{L}\p{M}_/\-][\p{L}\p{N}\p{M}_/\-]*)")
        .expect("tag regex")
});
/// A URL scheme (`https:`, `mailto:`, `obsidian:`): such a Markdown link is not a vault file.
static SCHEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").expect("scheme regex"));

/// The front matter block at the top of a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    /// The whole block with its `---` lines and the line break after the closing one.
    pub range: Range<usize>,
    /// The YAML between the `---` lines.
    pub yaml: Range<usize>,
}

/// A link to a vault file found in a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// `![[...]]` or `![...](...)`.
    pub embed: bool,
    /// A Markdown link (`[text](target)`) rather than a wikilink.
    pub markdown: bool,
    /// The whole link in the note's text.
    pub range: Range<usize>,
    /// The file part as written (`Folder/Note`), without `#heading` or `|alias`; for a Markdown link, still
    /// percent-encoded and without the angle brackets.
    pub target_range: Range<usize>,
    /// The file part, decoded (`Folder/Note`, `My note.md`).
    pub target: String,
    /// `heading` or `^block` after the `#`, if any.
    pub subpath: Option<String>,
}

/// What [`scan`] finds in a note.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    pub front_matter: Option<FrontMatter>,
    /// In text order.
    pub links: Vec<Link>,
    /// Tags as written (without `#`), body tags first, then front matter tags.
    pub tags: Vec<String>,
}

/// The front matter of `text` (BOM already removed): a first line `---` and a later line `---`.
#[must_use]
pub fn front_matter(text: &str) -> Option<FrontMatter> {
    let first_end = text.find('\n')?;
    if text[..first_end].trim_end() != "---" {
        return None;
    }
    let yaml_start = first_end + 1;
    let mut at = yaml_start;
    while at <= text.len() {
        let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
        if text[at..end].trim_end() == "---" {
            let after = if end < text.len() { end + 1 } else { end };
            return Some(FrontMatter {
                range: 0..after,
                yaml: yaml_start..at,
            });
        }
        if end >= text.len() {
            break;
        }
        at = end + 1;
    }
    None
}

/// Byte ranges of inline code, code blocks and raw HTML in `body` (offset by `base`).
fn code_ranges(body: &str, base: usize) -> Vec<Range<usize>> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    Parser::new_ext(body, options)
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Code(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock) => Some(range),
            _ => None,
        })
        .map(|r| r.start + base..r.end + base)
        .collect()
}

fn inside(ranges: &[Range<usize>], at: usize) -> bool {
    ranges.iter().any(|r| r.contains(&at))
}

/// Trim `range` of `text` (whitespace both sides).
fn trimmed(text: &str, range: Range<usize>) -> Range<usize> {
    let s = &text[range.clone()];
    let start = range.start + (s.len() - s.trim_start().len());
    let end = range.end - (s.len() - s.trim_end().len());
    start..end.max(start)
}

fn wikilink(text: &str, caps: &regex::Captures<'_>) -> Link {
    let whole = caps.get(0).expect("match");
    let inner = caps.get(2).expect("inner");
    let raw = inner.as_str();
    // `[[a\|b]]` (inside a table) is `[[a|b]]`.
    let link_len = raw.find('|').unwrap_or(raw.len());
    let link_len = if raw[..link_len].ends_with('\\') {
        link_len - 1
    } else {
        link_len
    };
    let link = &raw[..link_len];
    let (path_len, subpath) = match link.find('#') {
        Some(i) => (i, Some(link[i + 1..].trim().to_string())),
        None => (link.len(), None),
    };
    let target_range = trimmed(text, inner.start()..inner.start() + path_len);
    Link {
        embed: !caps[1].is_empty(),
        markdown: false,
        range: whole.range(),
        target: text[target_range.clone()].to_string(),
        target_range,
        subpath: subpath.filter(|s| !s.is_empty()),
    }
}

fn markdown_link(text: &str, caps: &regex::Captures<'_>) -> Option<Link> {
    let whole = caps.get(0).expect("match");
    let dest = caps.get(2).expect("destination");
    let (raw_start, raw) = match dest.as_str().strip_prefix('<') {
        Some(inner) => (dest.start() + 1, inner.strip_suffix('>').unwrap_or(inner)),
        None => (dest.start(), dest.as_str()),
    };
    if raw.is_empty() || raw.starts_with('#') || SCHEME.is_match(raw) {
        return None;
    }
    let (path_len, subpath) = match raw.find('#') {
        Some(i) => (i, Some(raw[i + 1..].to_string())),
        None => (raw.len(), None),
    };
    let target_range = raw_start..raw_start + path_len;
    let target = percent_decode_str(&text[target_range.clone()])
        .decode_utf8_lossy()
        .into_owned();
    Some(Link {
        embed: !caps[1].is_empty(),
        markdown: true,
        range: whole.range(),
        target,
        target_range,
        subpath: subpath
            .map(|s| percent_decode_str(&s).decode_utf8_lossy().into_owned())
            .filter(|s| !s.is_empty()),
    })
}

/// Tags listed in front matter (`tags:` or `tag:`, a list or a comma / space separated string).
#[must_use]
pub fn front_matter_tags(yaml: &str) -> Vec<String> {
    use yaml_rust2::{Yaml, YamlLoader};
    let Ok(docs) = YamlLoader::load_from_str(yaml) else {
        return Vec::new();
    };
    let Some(Yaml::Hash(map)) = docs.into_iter().next() else {
        return Vec::new();
    };
    let scalar = |y: &Yaml| match y {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        _ => None,
    };
    let mut out = Vec::new();
    for key in ["tags", "tag"] {
        let words: Vec<String> = match map.get(&Yaml::String(key.into())) {
            Some(Yaml::Array(items)) => items.iter().filter_map(scalar).collect(),
            Some(other) => scalar(other)
                .map(|s| s.split([',', ' ']).map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_default(),
            None => Vec::new(),
        };
        out.extend(
            words
                .iter()
                .map(|w| w.trim().trim_start_matches('#').to_string())
                .filter(|w| !w.is_empty()),
        );
    }
    out
}

/// Scan a note's text (BOM removed).
#[must_use]
pub fn scan(text: &str) -> Scan {
    let fm = front_matter(text);
    let body_start = fm.as_ref().map_or(0, |f| f.range.end);
    let code = code_ranges(&text[body_start..], body_start);
    let mut links: Vec<Link> = WIKILINK
        .captures_iter(text)
        .filter(|c| !inside(&code, c.get(0).expect("match").start()))
        .map(|c| wikilink(text, &c))
        .collect();
    links.extend(
        MD_LINK
            .captures_iter(&text[body_start..])
            .filter_map(|c| {
                let m = c.get(0).expect("match");
                (!inside(&code, body_start + m.start())).then_some(c)
            })
            .filter_map(|c| {
                // Offsets are relative to the body: rebuild the link against the whole text.
                let shifted = markdown_link(&text[body_start..], &c)?;
                Some(Link {
                    range: shifted.range.start + body_start..shifted.range.end + body_start,
                    target_range: shifted.target_range.start + body_start
                        ..shifted.target_range.end + body_start,
                    ..shifted
                })
            }),
    );
    links.sort_by_key(|l| l.range.start);
    let in_link = |at: usize| links.iter().any(|l| l.range.contains(&at));
    let mut tags: Vec<String> = TAG
        .captures_iter(&text[body_start..])
        .filter_map(|c| c.get(1))
        .filter(|m| {
            let at = body_start + m.start();
            !inside(&code, at) && !in_link(at)
        })
        .map(|m| m.as_str().trim_end_matches('/').to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if let Some(f) = &fm {
        tags.extend(front_matter_tags(&text[f.yaml.clone()]));
    }
    Scan {
        front_matter: fm,
        links,
        tags,
    }
}

#[cfg(test)]
#[path = "syntax_tests.rs"]
mod tests;
