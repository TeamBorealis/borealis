//! Property tests for the rich-text tree (homebrew creator spec 7.3, 18.2 "Random").
//!
//! Seeded and deterministic (a small xorshift generator, no extra dependency), so a failure names its seed:
//! * every normalized random document `d`: `parse(render(d)) == d`, `d.normalized() == d`, JSON round trip;
//! * every random HTML soup `s` (tags, text, entities, whitespace): `render(parse(s)) == s`;
//! * `fix` leaves no escape artefact and is idempotent.

use borealis_content::rich::{
    fix, lint, parse, render, Block, BlockAttrs, BoldTag, Cell, Doc, HeadingAttrs, Inline,
    ItalicTag, KeptReason, ListItem, Mark, ParagraphAttrs, RowAttrs, TableAttrs, TableRow,
};

const DOC_CASES: u64 = 4000;
const SOUP_CASES: u64 = 4000;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap()
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const WORDS: [&str; 16] = [
    "Fireball",
    " ",
    "a & b",
    "x < y > z",
    "\u{201c}quoted\u{201d}",
    "it\u{2019}s",
    "\u{2014}",
    "caf\u{e9}",
    "\u{5d0}\u{5d1}",
    "\u{65e5}\u{672c}",
    "1d6 + 2",
    "&amp; literal",
    "tab\there",
    "line\nfeed",
    "  spaced  ",
    "\u{1f409}",
];

const KEPT: [&str; 5] = [
    "<pre> Level   Spells\n  3       Bless</pre>",
    "<h5 class=\"caption\">Table</h5>",
    "<h6>Small</h6>",
    "<p class=\"table-text\">cell</p>",
    "<table class=\"class-features\"><tr><td>1</td></tr></table>",
];

fn marks(rng: &mut Rng) -> Vec<Mark> {
    let mut out = Vec::new();
    if rng.chance(30) {
        out.push(Mark::bold(*rng.pick(&[BoldTag::B, BoldTag::Strong])));
    }
    if rng.chance(30) {
        out.push(Mark::italic(*rng.pick(&[ItalicTag::I, ItalicTag::Em])));
    }
    out
}

fn inline(rng: &mut Rng) -> Vec<Inline> {
    (0..rng.below(6))
        .map(|_| {
            if rng.chance(12) {
                Inline::HardBreak { marks: marks(rng) }
            } else {
                Inline::Text {
                    text: (*rng.pick(&WORDS)).to_owned(),
                    marks: marks(rng),
                }
            }
        })
        .collect()
}

fn items(rng: &mut Rng) -> Vec<ListItem> {
    (0..=rng.below(4))
        .map(|_| ListItem {
            content: inline(rng),
        })
        .collect()
}

fn table(rng: &mut Rng) -> Block {
    let sections = rng.chance(60);
    let head_rows = if sections { rng.below(3) } else { 0 };
    let rows = (0..=rng.below(4))
        .map(|r| TableRow {
            attrs: RowAttrs {
                head: r < head_rows,
            },
            content: (0..rng.below(4))
                .map(|_| {
                    let content = inline(rng);
                    if rng.chance(30) {
                        Cell::Header { content }
                    } else {
                        Cell::Data { content }
                    }
                })
                .collect(),
        })
        .collect();
    Block::Table {
        attrs: TableAttrs {
            joined: false,
            sections,
        },
        content: rows,
    }
}

fn simple_block(rng: &mut Rng) -> Block {
    match rng.below(4) {
        0 => Block::Heading {
            attrs: HeadingAttrs {
                joined: false,
                level: *rng.pick(&[3, 4]),
            },
            content: inline(rng),
        },
        1 => Block::BulletList {
            attrs: BlockAttrs::default(),
            content: items(rng),
        },
        2 => Block::OrderedList {
            attrs: BlockAttrs::default(),
            content: items(rng),
        },
        _ => Block::paragraph(inline(rng)),
    }
}

fn block(rng: &mut Rng) -> Block {
    let mut b = match rng.below(9) {
        0 => table(rng),
        1 => Block::Blockquote {
            attrs: BlockAttrs::default(),
            content: (0..=rng.below(3))
                .map(|_| {
                    let mut child = simple_block(rng);
                    child.set_joined(rng.chance(50));
                    child
                })
                .collect(),
        },
        2 => Block::kept(*rng.pick(&KEPT), KeptReason::OutsidePalette),
        _ => simple_block(rng),
    };
    b.set_joined(rng.chance(40));
    b
}

fn doc(rng: &mut Rng) -> Doc {
    if rng.chance(10) {
        return Doc::new(vec![Block::Paragraph {
            attrs: ParagraphAttrs {
                joined: false,
                bare: true,
            },
            content: inline(rng),
        }]);
    }
    Doc::new((0..rng.below(7)).map(|_| block(rng)).collect())
}

/// What a document exercises, so the generator's coverage is asserted too.
fn features(html: &str) -> [bool; 7] {
    [
        html.contains("<table><thead>"),
        html.contains("<table><tr>"),
        html.contains("<blockquote>"),
        html.contains("<pre>"),
        html.contains("<br />"),
        html.contains("</p><"),
        !html.is_empty() && !html.starts_with('<'),
    ]
}

#[test]
fn random_documents_survive_render_and_parse() {
    let mut seen = [0usize; 7];
    for seed in 0..DOC_CASES {
        let d = doc(&mut Rng::new(seed)).normalized();
        for (count, hit) in seen.iter_mut().zip(features(&render(&d))) {
            *count += usize::from(hit);
        }
        assert_eq!(
            d.normalized(),
            d,
            "seed {seed}: normalization is idempotent"
        );
        let html = render(&d);
        assert_eq!(
            parse(&html),
            d,
            "seed {seed}: parse(render(d)) for {html:?}"
        );
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(
            serde_json::from_value::<Doc>(json).unwrap(),
            d,
            "seed {seed}: JSON"
        );
    }
    assert!(seen.iter().all(|n| *n >= 50), "generator coverage {seen:?}");
}

const SOUP: [&str; 34] = [
    "<p>",
    "</p>",
    "<b>",
    "</b>",
    "<strong>",
    "</strong>",
    "<i>",
    "</i>",
    "<em>",
    "</em>",
    "<br />",
    "<br>",
    "<ul>",
    "</ul>",
    "<li>",
    "</li>",
    "<table>",
    "<tr>",
    "<td>",
    "</td>",
    "</tr>",
    "</table>",
    "<h5>",
    "</h5>",
    "<pre>",
    "</pre>",
    "<p class=\"x\">",
    "<!-- c -->",
    "\n",
    " ",
    "text",
    "&amp;",
    "&#8217;",
    "<",
];

#[test]
fn any_html_is_written_back_byte_for_byte() {
    for seed in 0..SOUP_CASES {
        let mut rng = Rng::new(seed + 1_000_000);
        let soup: String = (0..rng.below(14)).map(|_| *rng.pick(&SOUP)).collect();
        let d = parse(&soup);
        assert_eq!(render(&d), soup, "seed {seed}");
        assert_eq!(parse(&render(&d)), d, "seed {seed}: stable");
    }
}

#[test]
fn fix_cleans_random_artefacts() {
    const ARTEFACTS: [&str; 8] = [
        "\\n",
        "\\\"",
        "&amp;",
        "&#8217;",
        "\\",
        "\u{e2}\u{20ac}\u{2122}",
        "&nbsp;",
        "\\t",
    ];
    for seed in 0..1000 {
        let mut rng = Rng::new(seed + 2_000_000);
        let mut d = doc(&mut rng).normalized();
        for b in &mut d.content {
            if let Block::Paragraph { content, .. } = b {
                content.push(Inline::text(*rng.pick(&ARTEFACTS)));
            }
        }
        let fixed = fix(&d);
        assert!(lint(&fixed).is_empty(), "seed {seed}: {:?}", lint(&fixed));
        assert_eq!(fix(&fixed), fixed, "seed {seed}: idempotent");
        assert_eq!(parse(&render(&fixed)), fixed, "seed {seed}: canonical");
    }
}
