//! Analyzers (spec section 4) and name normalisation.
//!
//! `en_stem`: simple tokenizer, drop tokens over 40 chars, lowercase, ASCII folding, D&D stopword list, Snowball
//! English stemmer. The simple tokenizer splits on every non-alphanumeric character, so `can't` and `can’t` both
//! become `can` + `t` (the curly quote needs no separate fold). The same analyzer runs at query time.

use std::sync::OnceLock;
use tantivy::tokenizer::{
    AsciiFoldingFilter, Language, LowerCaser, RawTokenizer, RemoveLongFilter, SimpleTokenizer,
    Stemmer, StopWordFilter, TextAnalyzer, TokenizerManager, WhitespaceTokenizer,
};

/// Bumped whenever the analyzer chain or the stopword list changes (forces a rebuild).
pub const ANALYZER_VERSION: u32 = 1;
/// Registered name of the main text analyzer.
pub const EN_STEM: &str = "en_stem";
/// Registered name of the analyzer for pre-computed name prefixes.
pub const NAME_PREFIX: &str = "name_prefix";
/// Longest token kept.
pub const MAX_TOKEN_LEN: usize = 40;
/// Shortest edge n-gram indexed in `name_prefix`.
pub const MIN_PREFIX: usize = 2;
/// Longest edge n-gram indexed in `name_prefix`.
pub const MAX_PREFIX: usize = 15;

/// English stopwords minus the words that carry rules meaning (`can`, `will`, `no`, `not`, `only`, `each`, ...).
pub const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "do", "does", "for", "how", "i", "if",
    "in", "into", "is", "it", "my", "of", "on", "or", "such", "that", "the", "their", "then",
    "there", "these", "they", "this", "to", "was", "what", "when", "which", "who", "with", "you",
    "your",
];

fn en_stem() -> TextAnalyzer {
    TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(MAX_TOKEN_LEN + 1))
        .filter(LowerCaser)
        .filter(AsciiFoldingFilter)
        .filter(StopWordFilter::remove(
            STOPWORDS.iter().map(|w| (*w).to_string()),
        ))
        .filter(Stemmer::new(Language::English))
        .build()
}

fn name_prefix() -> TextAnalyzer {
    TextAnalyzer::builder(WhitespaceTokenizer::default())
        .filter(LowerCaser)
        .filter(AsciiFoldingFilter)
        .build()
}

/// Register every analyzer of this crate on a tokenizer manager.
pub fn register(manager: &TokenizerManager) {
    manager.register(EN_STEM, en_stem());
    manager.register(NAME_PREFIX, name_prefix());
}

/// Run a named analyzer over `text`, returning `(position, token)` pairs.
#[must_use]
pub fn analyze(manager: &TokenizerManager, analyzer: &str, text: &str) -> Vec<(usize, String)> {
    let Some(mut a) = manager.get(analyzer) else {
        return Vec::new();
    };
    let mut stream = a.token_stream(text);
    let mut out = Vec::new();
    while stream.advance() {
        let t = stream.token();
        out.push((t.position, t.text.clone()));
    }
    out
}

fn fold_analyzer() -> &'static TextAnalyzer {
    static FOLD: OnceLock<TextAnalyzer> = OnceLock::new();
    FOLD.get_or_init(|| {
        TextAnalyzer::builder(RawTokenizer::default())
            .filter(LowerCaser)
            .filter(AsciiFoldingFilter)
            .build()
    })
}

/// Lowercase and ASCII-fold (`Ünderdark` -> `underdark`).
#[must_use]
pub fn fold(text: &str) -> String {
    let mut a = fold_analyzer().clone();
    let mut stream = a.token_stream(text);
    let mut out = String::new();
    while stream.advance() {
        out.push_str(&stream.token().text);
    }
    out
}

/// The `name_exact` form of a name or query: lowercase, ASCII-folded, apostrophes dropped (curly ones too), every
/// other punctuation mark a space, whitespace collapsed.
#[must_use]
pub fn normalize_name(text: &str) -> String {
    let folded = fold(text);
    let mut out = String::with_capacity(folded.len());
    for c in folded.chars() {
        if matches!(c, '\'' | '\u{2019}' | '\u{2018}' | '\u{02bc}') {
            continue;
        }
        out.push(if c.is_alphanumeric() { c } else { ' ' });
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The words of a query as lowercase ASCII-folded alphanumeric runs (no stemming, no stopword removal).
#[must_use]
pub fn plain_words(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Edge n-grams (lengths [`MIN_PREFIX`]..=[`MAX_PREFIX`]) of every word of `name`, space separated, for the
/// `name_prefix` field.
#[must_use]
pub fn prefix_grams(name: &str) -> String {
    let mut grams: Vec<String> = Vec::new();
    for word in plain_words(name) {
        let chars: Vec<char> = word.chars().collect();
        for len in MIN_PREFIX..=chars.len().min(MAX_PREFIX) {
            grams.push(chars[..len].iter().collect());
        }
    }
    grams.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> TokenizerManager {
        let m = TokenizerManager::default();
        register(&m);
        m
    }

    fn tokens(text: &str) -> Vec<String> {
        analyze(&manager(), EN_STEM, text)
            .into_iter()
            .map(|(_, t)| t)
            .collect()
    }

    #[test]
    fn stems_grapple_forms_together() {
        let all = ["grappled", "grapple", "grappling", "grapples"].map(tokens);
        assert!(all.iter().all(|t| t == &all[0] && t.len() == 1), "{all:?}");
    }

    #[test]
    fn keeps_game_words_and_drops_function_words() {
        assert_eq!(tokens("the can will"), ["can", "will"]);
        assert!(tokens("What is the").is_empty());
    }

    #[test]
    fn folds_accents_and_curly_apostrophes() {
        assert_eq!(tokens("Ünderdark"), tokens("underdark"));
        assert_eq!(tokens("can\u{2019}t"), tokens("can't"));
        assert_eq!(tokens("can\u{2019}t"), ["can", "t"]);
    }

    #[test]
    fn drops_over_long_tokens() {
        assert!(tokens(&"x".repeat(41)).is_empty());
        assert_eq!(tokens(&"x".repeat(40)).len(), 1);
    }

    #[test]
    fn name_normalisation_folds_quotes_and_punctuation() {
        assert_eq!(
            normalize_name("Tasha\u{2019}s  Caustic-Brew!"),
            "tashas caustic brew"
        );
        assert_eq!(normalize_name("Fireball"), normalize_name("  FIREBALL "));
    }

    #[test]
    fn prefix_grams_cover_each_word() {
        let g = prefix_grams("Fire Bolt");
        for want in ["fi", "fir", "fire", "bo", "bol", "bolt"] {
            assert!(g.split(' ').any(|x| x == want), "{want} in {g}");
        }
        assert!(!g.split(' ').any(|x| x.len() == 1));
    }
}
