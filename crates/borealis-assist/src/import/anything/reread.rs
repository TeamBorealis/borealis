//! A second look at the pages of creatures detection names whose stat block the source text
//! lacks: a stat block pasted as a picture that the first look took for decoration is read
//! there, once, within a small page budget.

use borealis_ai::task::RunOptions;
use borealis_ai::{Attachment, AttachmentKind};

use super::{pictures, stat_lines, Ingested, VisionRunner};
use crate::import::chunk::norm_words;
use crate::import::read_pdf_layout;

/// Most pages read again as pictures for creatures whose stat block is missing (the second
/// look's budget: one call per [`super::VISUAL_PAGES_PER_CALL`] of them).
const MAX_REREAD_PAGES: usize = 8;

/// The section of `text` under the heading (or bold line) naming `name`: up to the next
/// heading. `None` when no heading names it.
fn named_section<'a>(text: &'a str, name: &str) -> Option<String> {
    let wanted = norm_words(name);
    let lines: Vec<&'a str> = text.lines().collect();
    let label = |l: &str| norm_words(l.trim().trim_start_matches('#').trim_matches('*'));
    let start = lines.iter().position(|l| {
        let t = l.trim();
        (t.starts_with('#') || t.starts_with("**")) && label(t) == wanted
    })?;
    let end = lines
        .iter()
        .skip(start + 1)
        .position(|l| l.trim_start().starts_with('#'))
        .map_or(lines.len(), |k| start + 1 + k);
    Some(lines[start..end].join("\n"))
}

/// The creatures of `creatures` the source names under a heading of their own whose section
/// prints no stat block.
fn missing_stat_blocks(text: &str, creatures: &[String]) -> Vec<String> {
    creatures
        .iter()
        .filter(|c| named_section(text, c).is_some_and(|s| !stat_lines::has_stat_block(&s)))
        .cloned()
        .collect()
}

/// Look once more, as pictures, at the pages of creatures detection names whose stat block is
/// not in `text` (a pasted stat block the first look took for decoration): each such page that
/// shows an image goes to the pictures call, its blocks are spliced into `text`. At most
/// [`MAX_REREAD_PAGES`] pages; the first failed call ends the second look (the import's call
/// budget may be spent). Returns the new text (unchanged when nothing was read) with its notes
/// and calls.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn reread_missing_stat_blocks(
    runner: &dyn VisionRunner,
    attachments: &[Attachment],
    text: &str,
    creatures: &[String],
    opts: &RunOptions<'_>,
) -> Ingested {
    let mut out = Ingested {
        text: text.to_string(),
        ..Ingested::default()
    };
    let missing: Vec<String> = missing_stat_blocks(text, creatures)
        .iter()
        .map(|c| norm_words(c))
        .collect();
    let mut left = MAX_REREAD_PAGES;
    for a in attachments
        .iter()
        .filter(|a| matches!(a.kind, AttachmentKind::Pdf))
    {
        if missing.is_empty() || left == 0 {
            break;
        }
        let Ok(layout) = read_pdf_layout(&a.bytes) else {
            continue;
        };
        let scale = layout.scale();
        let named: std::collections::BTreeSet<u32> = layout
            .pages
            .iter()
            .enumerate()
            .filter(|(_, p)| p.has_text())
            .filter(|(k, _)| {
                let words = format!(" {} ", norm_words(&layout.text_of(&[*k], &scale)));
                missing.iter().any(|m| words.contains(&format!(" {m} ")))
            })
            .map(|(_, p)| p.number)
            .collect();
        let pages: Vec<usize> = pictures::imaged_pages(&a.bytes, &named)
            .into_iter()
            .take(left)
            .filter_map(|n| layout.pages.iter().position(|p| p.number == n))
            .collect();
        if pages.is_empty() {
            continue;
        }
        left -= pages.len();
        let Ok(parts) = pictures::picture_parts(a, &layout, &pages, &mut out.notes) else {
            continue;
        };
        // Each page with the next one (its end) in page order, so a block stays on its page.
        let mut bounds: Vec<usize> = pages
            .iter()
            .flat_map(|&k| [k, k + 1])
            .filter(|&k| k < layout.pages.len())
            .collect();
        bounds.sort_unstable();
        bounds.dedup();
        let own: Vec<(u32, String)> = bounds
            .iter()
            .map(|&k| (layout.pages[k].number, layout.text_of(&[k], &scale)))
            .collect();
        let mut insets = Vec::new();
        for part in &parts {
            let Some(read) = pictures::transcribe_pictures(runner, part, opts, &mut out).await
            else {
                out.text = pictures::splice(&out.text, &own, &insets);
                return out;
            };
            insets.extend(read);
        }
        out.text = pictures::splice(&out.text, &own, &insets);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use borealis_ai::task::{AiRunner, BoxFuture};
    use borealis_ai::{AiError, AiEvent, AiOutcome, AiRequest};
    use serde_json::json;

    use super::super::pictures::tests::{lore, pdf};
    use super::*;

    /// Answers every pictures call with `blocks` (a refusal when `None`), counting calls.
    struct Fake {
        blocks: Option<serde_json::Value>,
        calls: AtomicUsize,
    }

    impl AiRunner for Fake {
        fn run<'a>(
            &'a self,
            _req: &'a AiRequest,
            _sink: Option<&'a tokio::sync::mpsc::UnboundedSender<AiEvent>>,
        ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let blocks = self.blocks.clone();
            Box::pin(async move {
                match blocks {
                    Some(b) => Ok(AiOutcome {
                        text: json!({ "blocks": b }).to_string(),
                        ..Default::default()
                    }),
                    None => Err(AiError::Invalid("the import's call budget is spent".into())),
                }
            })
        }
    }

    impl VisionRunner for Fake {}

    fn fake(blocks: Option<serde_json::Value>) -> Fake {
        Fake {
            blocks,
            calls: 0.into(),
        }
    }

    const BLOCK: &str =
        "Small beast, unaligned\nArmor Class 13\nHit Points 9 (2d6 + 2)\nSpeed 10 ft., fly 40 ft.";

    /// The creature's page with a picture, as one PDF attachment, and its text without the
    /// picture's stat block.
    fn source() -> (Vec<Attachment>, String) {
        let bytes = pdf(&[(lore(), Some([72.0, 300.0, 230.0, 300.0])), (lore(), None)]);
        let text = "# Gloomwing\n\nGloomwings roost under old bridges and hum at dusk.\nVillagers leave them sweet water in clay bowls.\n\n# Other Matters\n\nRules text.".to_string();
        let a = Attachment {
            kind: AttachmentKind::Pdf,
            bytes,
            name: "creatures.pdf".into(),
        };
        (vec![a], text)
    }

    #[tokio::test]
    async fn a_named_creature_without_its_stat_block_gets_its_page_read_again() {
        let (attachments, text) = source();
        let runner = fake(Some(
            json!([{"page": 1, "after": "Villagers leave them sweet water in clay bowls.", "text": BLOCK}]),
        ));
        let out = reread_missing_stat_blocks(
            &runner,
            &attachments,
            &text,
            &["Gloomwing".into()],
            &RunOptions::default(),
        )
        .await;
        assert_eq!(out.calls, 1);
        assert!(stat_lines::has_stat_block(
            &named_section(&out.text, "Gloomwing").unwrap()
        ));
        // The block stays in the creature's section, before the next heading.
        assert!(
            out.text.find("Armor Class 13").unwrap() < out.text.find("# Other Matters").unwrap()
        );
    }

    #[tokio::test]
    async fn printed_stat_blocks_unnamed_creatures_and_a_spent_budget_cost_nothing_more() {
        let (attachments, text) = source();
        let printed = format!("{text}\n\n# Reedling\n\n{BLOCK}");
        let runner = fake(Some(json!([])));
        let out = reread_missing_stat_blocks(
            &runner,
            &attachments,
            &printed,
            &["Reedling".into(), "Nobody Here".into()],
            &RunOptions::default(),
        )
        .await;
        assert_eq!((out.calls, out.text.as_str()), (0, printed.as_str()));
        // A failed call (the budget is spent) is reported; the text stands.
        let refused = fake(None);
        let out = reread_missing_stat_blocks(
            &refused,
            &attachments,
            &text,
            &["Gloomwing".into()],
            &RunOptions::default(),
        )
        .await;
        assert_eq!((out.calls, out.text.as_str()), (1, text.as_str()));
        assert_eq!(out.notes.len(), 1);
    }
}
