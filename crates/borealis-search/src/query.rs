//! Query construction with the spec's ranking tweaks 1-7, each behind a flag (spec section 5).
//!
//! The query is a BooleanQuery of SHOULD clauses scored by Tantivy's BM25 (k1 = 1.2, b = 0.75, fixed by Tantivy),
//! wrapped with zero-score filters (edition, type, pack, campaign, source exclusions). Tweak 7's boost for the
//! preferred edition is a score multiplier applied to the candidate pool in `index.rs`, because Tantivy boosts are
//! additive per clause, not multiplicative on the document score.

use crate::analyzer::{
    analyze, normalize_name, plain_words, EN_STEM, MAX_PREFIX, MIN_PREFIX, NAME_PREFIX,
};
use crate::chunk::EditionTag;
use crate::schema::Fields;
use crate::synonyms::Synonyms;
use std::collections::HashSet;
use tantivy::query::{
    BooleanQuery, BoostQuery, ConstScoreQuery, FuzzyTermQuery, Occur, PhraseQuery, Query, TermQuery,
};
use tantivy::schema::{Field, IndexRecordOption};
use tantivy::tokenizer::TokenizerManager;
use tantivy::Term;

/// Which of the ranking tweaks are active. `Tweaks::default()` is every tweak on (the candidate configuration);
/// `Tweaks::none()` is plain multi-field BM25 (variant V1 of the evaluation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tweaks {
    /// 1. Field weights: `name` x4, `headings` x2, `body` x1.
    pub field_weights: bool,
    /// 2. Exact-name boost x10 on `name_exact`.
    pub exact_name: bool,
    /// 3. Prefix as you type: last word on `name_prefix`, x1.5.
    pub prefix: bool,
    /// 4. Typo tolerance: fuzzy terms on `name` (x0.6) and `body` (x0.3) for words of 4+ characters.
    pub typo: bool,
    /// 5. Phrase/proximity (slop 2) on `headings` and `body`, x1.5.
    pub phrase: bool,
    /// 6. Query-time synonym expansion at x0.8.
    pub synonyms: bool,
    /// 7. Edition filter and preferred-edition boost.
    pub edition: bool,
}

impl Tweaks {
    /// Every tweak on.
    pub const ALL: Self = Self {
        field_weights: true,
        exact_name: true,
        prefix: true,
        typo: true,
        phrase: true,
        synonyms: true,
        edition: true,
    };

    /// Every tweak off.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            field_weights: false,
            exact_name: false,
            prefix: false,
            typo: false,
            phrase: false,
            synonyms: false,
            edition: false,
        }
    }
}

impl Default for Tweaks {
    fn default() -> Self {
        Self::ALL
    }
}

/// Numeric knobs of the tweaks (the evaluation sweeps the field weights).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    /// `name` field weight (tweak 1).
    pub name: f32,
    /// `headings` field weight (tweak 1).
    pub headings: f32,
    /// `body` field weight (tweak 1).
    pub body: f32,
    /// Exact-name boost (tweak 2).
    pub exact_name: f32,
    /// Prefix boost (tweak 3).
    pub prefix: f32,
    /// Fuzzy boost on `name` (tweak 4).
    pub fuzzy_name: f32,
    /// Fuzzy boost on `body` (tweak 4).
    pub fuzzy_body: f32,
    /// Fuzzy boost of the whole normalised query against `name_exact` (tweak 4 extension: a misspelled full
    /// name ranks the element of that name above elements that merely contain similar words).
    pub fuzzy_exact_name: f32,
    /// Shortest word that is fuzzed (tweak 4).
    pub fuzzy_min_len: usize,
    /// Word length from which edit distance 2 is allowed (tweak 4).
    pub fuzzy_distance2_len: usize,
    /// Phrase boost (tweak 5).
    pub phrase: f32,
    /// Phrase slop (tweak 5).
    pub phrase_slop: u32,
    /// Synonym expansion boost (tweak 6).
    pub synonym: f32,
    /// Score multiplier of the open character's edition when searching both (tweak 7).
    pub preferred_edition: f32,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            name: 4.0,
            headings: 2.0,
            body: 1.0,
            exact_name: 10.0,
            prefix: 1.5,
            fuzzy_name: 0.6,
            fuzzy_body: 0.3,
            fuzzy_exact_name: 40.0,
            fuzzy_min_len: 4,
            fuzzy_distance2_len: 8,
            phrase: 1.5,
            phrase_slop: 2,
            synonym: 0.8,
            preferred_edition: 1.2,
        }
    }
}

/// Tweak flags plus their weights.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SearchConfig {
    /// Which tweaks are on.
    pub tweaks: Tweaks,
    /// Their weights.
    pub weights: Weights,
}

/// One edition of the rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    /// 2014 rules.
    Y2014,
    /// 2024 rules.
    Y2024,
}

impl Edition {
    /// The index tag of this edition.
    #[must_use]
    pub fn tag(self) -> EditionTag {
        match self {
            Self::Y2014 => EditionTag::Y2014,
            Self::Y2024 => EditionTag::Y2024,
        }
    }
}

/// Edition scope of a search (tweak 7). Content tagged `none` is always included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditionScope {
    /// Only this edition (and `none`).
    Only(Edition),
    /// Both editions; the open character's edition, when known, gets the preferred-edition boost.
    Both {
        /// The edition to prefer.
        preferred: Option<Edition>,
    },
    /// No restriction, no boost.
    #[default]
    Any,
}

/// A search request. Shaped for the Tauri `search` command and the MCP tools.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchRequest {
    /// The user's text.
    pub query: String,
    /// Edition scope (ignored when tweak 7 is off).
    pub edition: EditionScope,
    /// Only these element types (empty = all).
    pub types: Vec<String>,
    /// Only these packs (`None` = all); the enabled-pack set of the content hub.
    pub packs: Option<Vec<String>>,
    /// Exclude hits from these sources (restricted sources).
    pub exclude_sources: Vec<String>,
    /// Only this campaign (sessions corpus).
    pub campaign: Option<String>,
    /// Page size.
    pub limit: usize,
    /// Page offset.
    pub offset: usize,
    /// Collapse the chunks of one document (`#1`, `#2`, ...) into its best chunk.
    pub collapse: bool,
}

impl SearchRequest {
    /// A request for `query` with defaults: any edition, no filters, 10 hits, chunks collapsed.
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            edition: EditionScope::Any,
            types: Vec::new(),
            packs: None,
            exclude_sources: Vec::new(),
            campaign: None,
            limit: 10,
            offset: 0,
            collapse: true,
        }
    }
}

type Clause = (Occur, Box<dyn Query>);

fn boosted(query: Box<dyn Query>, boost: f32) -> Box<dyn Query> {
    if (boost - 1.0).abs() < f32::EPSILON {
        query
    } else {
        Box::new(BoostQuery::new(query, boost))
    }
}

fn term_query(field: Field, text: &str, boost: f32) -> Box<dyn Query> {
    boosted(
        Box::new(TermQuery::new(
            Term::from_field_text(field, text),
            IndexRecordOption::WithFreqs,
        )),
        boost,
    )
}

/// A phrase (or a single term) over one field; `tokens` are `(position, text)`.
fn phrase_or_term(
    field: Field,
    tokens: &[(usize, String)],
    slop: u32,
    boost: f32,
) -> Option<Box<dyn Query>> {
    match tokens {
        [] => None,
        [(_, t)] => Some(term_query(field, t, boost)),
        many => {
            let terms: Vec<(usize, Term)> = many
                .iter()
                .map(|(p, t)| (*p, Term::from_field_text(field, t)))
                .collect();
            Some(boosted(
                Box::new(PhraseQuery::new_with_offset_and_slop(terms, slop)),
                boost,
            ))
        }
    }
}

fn any_of(values: impl IntoIterator<Item = Box<dyn Query>>) -> Box<dyn Query> {
    Box::new(BooleanQuery::new(
        values.into_iter().map(|q| (Occur::Should, q)).collect(),
    ))
}

fn filter(query: Box<dyn Query>) -> Clause {
    (Occur::Must, Box::new(ConstScoreQuery::new(query, 0.0)))
}

fn field_weights(cfg: &SearchConfig) -> [(f32, usize); 3] {
    // (weight, field index into [name, headings, body])
    if cfg.tweaks.field_weights {
        [
            (cfg.weights.name, 0),
            (cfg.weights.headings, 1),
            (cfg.weights.body, 2),
        ]
    } else {
        [(1.0, 0), (1.0, 1), (1.0, 2)]
    }
}

struct Ctx<'a> {
    f: &'a Fields,
    tokenizers: &'a TokenizerManager,
    cfg: &'a SearchConfig,
}

impl Ctx<'_> {
    fn text_fields(&self) -> [Field; 3] {
        [self.f.name, self.f.headings, self.f.body]
    }

    fn base_clauses(&self, tokens: &[(usize, String)], out: &mut Vec<Clause>) {
        let fields = self.text_fields();
        let mut seen = HashSet::new();
        for (_, token) in tokens {
            if !seen.insert(token.clone()) {
                continue;
            }
            for (weight, i) in field_weights(self.cfg) {
                out.push((Occur::Should, term_query(fields[i], token, weight)));
            }
        }
    }

    fn exact_clause(&self, query: &str, out: &mut Vec<Clause>) {
        let normalised = normalize_name(query);
        if !normalised.is_empty() {
            out.push((
                Occur::Should,
                term_query(self.f.name_exact, &normalised, self.cfg.weights.exact_name),
            ));
        }
    }

    fn prefix_clause(&self, query: &str, out: &mut Vec<Clause>) {
        let ends_open = query.chars().last().is_some_and(char::is_alphanumeric);
        if !ends_open {
            return;
        }
        let Some(word) = plain_words(query).pop() else {
            return;
        };
        let Some((_, token)) = analyze(self.tokenizers, NAME_PREFIX, &word).pop() else {
            return;
        };
        if token.chars().count() < MIN_PREFIX {
            return;
        }
        let token: String = token.chars().take(MAX_PREFIX).collect();
        out.push((
            Occur::Should,
            term_query(self.f.name_prefix, &token, self.cfg.weights.prefix),
        ));
    }

    fn typo_clauses(&self, query: &str, tokens: &[(usize, String)], out: &mut Vec<Clause>) {
        let w = &self.cfg.weights;
        let whole = normalize_name(query);
        let whole_len = whole.chars().count();
        if whole_len >= w.fuzzy_min_len {
            let distance = if whole_len >= w.fuzzy_distance2_len {
                2
            } else {
                1
            };
            let fuzzy = FuzzyTermQuery::new(
                Term::from_field_text(self.f.name_exact, &whole),
                distance,
                true,
            );
            out.push((Occur::Should, boosted(Box::new(fuzzy), w.fuzzy_exact_name)));
        }
        let mut seen = HashSet::new();
        for (_, token) in tokens {
            let len = token.chars().count();
            if len < w.fuzzy_min_len || !seen.insert(token.clone()) {
                continue;
            }
            let distance = if len >= w.fuzzy_distance2_len { 2 } else { 1 };
            for (field, boost) in [(self.f.name, w.fuzzy_name), (self.f.body, w.fuzzy_body)] {
                let fuzzy =
                    FuzzyTermQuery::new(Term::from_field_text(field, token), distance, true);
                out.push((Occur::Should, boosted(Box::new(fuzzy), boost)));
            }
        }
    }

    fn phrase_clauses(&self, tokens: &[(usize, String)], out: &mut Vec<Clause>) {
        if tokens.len() < 2 {
            return;
        }
        let w = &self.cfg.weights;
        for field in [self.f.headings, self.f.body] {
            if let Some(q) = phrase_or_term(field, tokens, w.phrase_slop, w.phrase) {
                out.push((Occur::Should, q));
            }
        }
    }

    fn synonym_clauses(&self, query: &str, synonyms: &Synonyms, out: &mut Vec<Clause>) {
        let fields = self.text_fields();
        for expansion in synonyms.expand(query) {
            let tokens = analyze(self.tokenizers, EN_STEM, &expansion);
            for (weight, i) in field_weights(self.cfg) {
                let boost = weight * self.cfg.weights.synonym;
                if let Some(q) = phrase_or_term(fields[i], &tokens, 0, boost) {
                    out.push((Occur::Should, q));
                }
            }
        }
    }

    fn filters(&self, req: &SearchRequest) -> Vec<Clause> {
        let mut out = Vec::new();
        if self.cfg.tweaks.edition {
            if let EditionScope::Only(edition) = req.edition {
                out.push(filter(any_of([
                    term_query(self.f.ruleset, edition.tag().as_str(), 1.0),
                    term_query(self.f.ruleset, EditionTag::None.as_str(), 1.0),
                ])));
            }
        }
        if !req.types.is_empty() {
            out.push(filter(any_of(
                req.types.iter().map(|t| term_query(self.f.ty, t, 1.0)),
            )));
        }
        if let Some(packs) = &req.packs {
            out.push(filter(any_of(
                packs.iter().map(|p| term_query(self.f.pack, p, 1.0)),
            )));
        }
        if let Some(campaign) = &req.campaign {
            out.push(filter(term_query(self.f.campaign, campaign, 1.0)));
        }
        for source in &req.exclude_sources {
            out.push((Occur::MustNot, term_query(self.f.source, source, 1.0)));
        }
        out
    }
}

/// Build the Tantivy query for `req`. `None` when the text yields nothing to search for (empty, or only
/// stopwords with no usable prefix).
#[must_use]
pub fn build_query(
    fields: &Fields,
    tokenizers: &TokenizerManager,
    req: &SearchRequest,
    cfg: &SearchConfig,
    synonyms: &Synonyms,
) -> Option<Box<dyn Query>> {
    let ctx = Ctx {
        f: fields,
        tokenizers,
        cfg,
    };
    let tokens = analyze(tokenizers, EN_STEM, &req.query);
    let t = &cfg.tweaks;
    let mut should: Vec<Clause> = Vec::new();
    ctx.base_clauses(&tokens, &mut should);
    if t.exact_name {
        ctx.exact_clause(&req.query, &mut should);
    }
    if t.prefix {
        ctx.prefix_clause(&req.query, &mut should);
    }
    if t.typo {
        ctx.typo_clauses(&req.query, &tokens, &mut should);
    }
    if t.phrase {
        ctx.phrase_clauses(&tokens, &mut should);
    }
    if t.synonyms {
        ctx.synonym_clauses(&req.query, synonyms, &mut should);
    }
    if should.is_empty() {
        return None;
    }
    let filters = ctx.filters(req);
    let main: Box<dyn Query> = Box::new(BooleanQuery::new(should));
    if filters.is_empty() {
        return Some(main);
    }
    let mut all = vec![(Occur::Must, main)];
    all.extend(filters);
    Some(Box::new(BooleanQuery::new(all)))
}
