//! The index schema shared by every corpus (spec section 4).

use crate::analyzer::{EN_STEM, NAME_PREFIX};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST, STORED, STRING,
};

/// Bumped when the schema changes (forces a rebuild).
pub const SCHEMA_VERSION: u32 = 1;

/// Field handles of the shared schema.
#[derive(Debug, Clone, Copy)]
pub struct Fields {
    /// Stable citation id (raw, stored).
    pub doc_key: Field,
    /// Title or element name (analysed, stored).
    pub name: Field,
    /// Heading path / element type (analysed, stored).
    pub headings: Field,
    /// Plain text (analysed, stored: it is the snippet source).
    pub body: Field,
    /// Normalised whole name (raw).
    pub name_exact: Field,
    /// Edge n-grams of the name words.
    pub name_prefix: Field,
    /// `2014|2024|none` (raw, fast, stored).
    pub ruleset: Field,
    /// Element type (raw, fast, stored).
    pub ty: Field,
    /// Pack id (raw, stored): the unit of incremental updates.
    pub pack: Field,
    /// Source name (raw, stored): for source restrictions.
    pub source: Field,
    /// Campaign id (raw, stored): sessions only.
    pub campaign: Field,
    /// SHA-256 of the chunk content (raw, stored).
    pub content_hash: Field,
    /// `1` on the first chunk of a split section (raw, stored).
    pub primary: Field,
}

fn text_options(tokenizer: &str, positions: bool, stored: bool) -> TextOptions {
    let record = if positions {
        IndexRecordOption::WithFreqsAndPositions
    } else {
        IndexRecordOption::WithFreqs
    };
    let opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(tokenizer)
            .set_index_option(record),
    );
    if stored {
        opts.set_stored()
    } else {
        opts
    }
}

/// Build the schema and its field handles.
#[must_use]
pub fn build() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let fields = Fields {
        doc_key: b.add_text_field("doc_key", STRING | STORED),
        name: b.add_text_field("name", text_options(EN_STEM, true, true)),
        headings: b.add_text_field("headings", text_options(EN_STEM, true, true)),
        body: b.add_text_field("body", text_options(EN_STEM, true, true)),
        name_exact: b.add_text_field("name_exact", STRING),
        name_prefix: b.add_text_field("name_prefix", text_options(NAME_PREFIX, false, false)),
        ruleset: b.add_text_field("ruleset", STRING | FAST | STORED),
        ty: b.add_text_field("type", STRING | FAST | STORED),
        pack: b.add_text_field("pack", STRING | STORED),
        source: b.add_text_field("source", STRING | STORED),
        campaign: b.add_text_field("campaign", STRING | STORED),
        content_hash: b.add_text_field("content_hash", STRING | STORED),
        primary: b.add_text_field("primary", STRING | STORED),
    };
    (b.build(), fields)
}
