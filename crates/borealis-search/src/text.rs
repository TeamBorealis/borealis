//! HTML to plain text and paragraph-aware splitting (spec section 3 chunking rules).

/// Rules sections longer than this many words are split at paragraphs.
pub const RULES_MAX_WORDS: usize = 350;
/// Lore descriptions longer than this many words are split at headings/paragraphs.
pub const LORE_MAX_WORDS: usize = 400;

const BLOCK_TAGS: &[&str] = &[
    "p",
    "li",
    "tr",
    "ul",
    "ol",
    "table",
    "div",
    "br",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
];
const CELL_TAGS: &[&str] = &["td", "th"];

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&amp;", "&")
}

fn collapse(text: &str) -> String {
    decode_entities(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tag_name(raw: &str) -> String {
    raw.trim_start_matches('/')
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Split an HTML fragment into plain-text blocks (paragraphs, list items, table rows, headings). Empty blocks are
/// dropped, entities decoded, whitespace collapsed. Table cells of a row are joined with a space.
#[must_use]
pub fn html_blocks(html: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let flush = |current: &mut String, blocks: &mut Vec<String>| {
        let text = collapse(current);
        if !text.is_empty() {
            blocks.push(text);
        }
        current.clear();
    };
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        current.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else {
            rest = "";
            break;
        };
        let name = tag_name(&after[..close]);
        if BLOCK_TAGS.contains(&name.as_str()) {
            flush(&mut current, &mut blocks);
        } else if CELL_TAGS.contains(&name.as_str()) {
            current.push(' ');
        }
        rest = &after[close + 1..];
    }
    current.push_str(rest);
    flush(&mut current, &mut blocks);
    blocks
}

/// Plain text of an HTML fragment (blocks joined with a space).
#[must_use]
pub fn html_to_text(html: &str) -> String {
    html_blocks(html).join(" ")
}

/// Word count (whitespace separated).
#[must_use]
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Group `blocks` into chunks of at most `max_words` words (a single longer block stays whole). When a section is
/// split, each chunk after the first starts with the last block of the previous chunk (one-paragraph overlap).
/// Returns one string per chunk, blocks joined with a space.
#[must_use]
pub fn split_blocks(blocks: &[String], max_words: usize) -> Vec<String> {
    let total: usize = blocks.iter().map(|b| word_count(b)).sum();
    if total <= max_words {
        return if blocks.is_empty() {
            Vec::new()
        } else {
            vec![blocks.join(" ")]
        };
    }
    let mut chunks: Vec<Vec<&String>> = Vec::new();
    let mut current: Vec<&String> = Vec::new();
    let mut words = 0usize;
    // Index of the first block of `current` that is genuinely new (not the overlap).
    let mut fresh_from = 0usize;
    for block in blocks {
        let w = word_count(block);
        if words + w > max_words && current.len() > fresh_from {
            let overlap = current.last().copied();
            chunks.push(std::mem::take(&mut current));
            words = 0;
            if let Some(o) = overlap {
                words += word_count(o);
                current.push(o);
            }
            fresh_from = current.len();
        }
        current.push(block);
        words += w;
    }
    if current.len() > fresh_from {
        chunks.push(current);
    }
    chunks
        .into_iter()
        .map(|c| c.into_iter().cloned().collect::<Vec<_>>().join(" "))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_split_at_paragraphs_rows_and_list_items() {
        let html = "<p>One <b>bold</b> &amp; two.</p><ul><li>a</li><li>b</li></ul>\
                    <table><tr><th>X</th><td>1</td></tr></table>";
        assert_eq!(
            html_blocks(html),
            ["One bold & two.", "a", "b", "X 1"].map(String::from)
        );
        assert_eq!(html_to_text("<p>x</p><p></p>"), "x");
        assert_eq!(html_to_text("plain&nbsp;text"), "plain text");
    }

    #[test]
    fn short_sections_stay_whole() {
        let blocks = vec!["a b c".to_string(), "d e".to_string()];
        assert_eq!(split_blocks(&blocks, 10), ["a b c d e"]);
        assert!(split_blocks(&[], 10).is_empty());
    }

    #[test]
    fn long_sections_split_with_one_paragraph_overlap() {
        let blocks: Vec<String> = (0..6)
            .map(|i| format!("p{i} {}", "w ".repeat(49).trim_end()))
            .collect();
        let chunks = split_blocks(&blocks, 120);
        assert!(chunks.len() >= 3, "{}", chunks.len());
        for pair in chunks.windows(2) {
            let last_of_first = pair[0].rsplit(" p").next().unwrap();
            let marker = format!("p{}", &last_of_first[..1]);
            assert!(pair[1].starts_with(&marker), "{marker} overlaps");
        }
        assert!(chunks.last().unwrap().contains("p5"));
        for c in &chunks {
            assert!(word_count(c) <= 200);
        }
    }

    #[test]
    fn one_huge_block_is_not_dropped() {
        let blocks = vec!["x ".repeat(500)];
        assert_eq!(split_blocks(&blocks, 100).len(), 1);
    }
}
