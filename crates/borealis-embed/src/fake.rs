//! A deterministic stand-in for the llama.cpp backend (tests and CI: no model file needed).
//!
//! Feature hashing of lower-cased words: texts sharing words get a high dot product, so fusion and indexing tests
//! behave like a (very weak) semantic model. Each word adds weight to one component inside the first 128 (so the
//! smallest Matryoshka prefix keeps the signal) and a smaller weight anywhere in the 768. The prompt prefixes are
//! stripped first, so a query and a document with the same words match.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::embedder::{check_dims, Embedder, ModelInfo};
use crate::error::Result;

/// Native dimensions, as EmbeddingGemma 2.
pub const FAKE_NATIVE_DIMS: usize = 768;
/// Model id reported by the fake.
pub const FAKE_MODEL_ID: &str = "fake-embedder";
/// A fixed, obviously fake SHA-256.
pub const FAKE_SHA256: &str = "fa4efa4efa4efa4efa4efa4efa4efa4efa4efa4efa4efa4efa4efa4efa4efa4e";

const SMALL_PREFIX: u64 = 128;
const BIAS: f32 = 0.01;

/// Deterministic hashing embedder.
#[derive(Debug)]
pub struct FakeEmbedder {
    model: ModelInfo,
    dims: usize,
    poison: Option<String>,
    calls: AtomicUsize,
}

impl FakeEmbedder {
    /// A fake producing `dims`-dimensional vectors.
    ///
    /// # Panics
    /// When `dims` is not a Matryoshka size (a test bug).
    #[must_use]
    pub fn new(dims: usize) -> Self {
        check_dims(dims, FAKE_NATIVE_DIMS).expect("fake embedder dims");
        Self {
            model: ModelInfo {
                id: FAKE_MODEL_ID.into(),
                sha256: FAKE_SHA256.into(),
                quant: "FAKE".into(),
                native_dims: FAKE_NATIVE_DIMS,
            },
            dims,
            poison: None,
            calls: AtomicUsize::new(0),
        }
    }

    /// Any prompt containing `marker` comes back as all-NaN (simulates the f16 failure of sec. 7).
    #[must_use]
    pub fn with_poison(mut self, marker: &str) -> Self {
        self.poison = Some(marker.to_string());
        self
    }

    /// Use a different model SHA-256 (tests of store invalidation).
    #[must_use]
    pub fn with_sha256(mut self, sha256: &str) -> Self {
        self.model.sha256 = sha256.to_string();
        self
    }

    /// Number of prompts embedded so far.
    #[must_use]
    pub fn prompts_embedded(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    fn raw(&self, prompt: &str) -> Vec<f32> {
        if self.poison.as_deref().is_some_and(|p| prompt.contains(p)) {
            return vec![f32::NAN; FAKE_NATIVE_DIMS];
        }
        let mut v = vec![0.0_f32; FAKE_NATIVE_DIMS];
        v[0] = BIAS;
        for word in words(strip_prompt(prompt)) {
            let h = fnv1a(word.as_bytes());
            let sign = if h & (1 << 63) == 0 { 1.0 } else { -1.0 };
            v[index(h % SMALL_PREFIX)] += sign;
            v[index((h >> 16) % FAKE_NATIVE_DIMS as u64)] += 0.5 * sign;
        }
        v
    }
}

impl Embedder for FakeEmbedder {
    fn model(&self) -> &ModelInfo {
        &self.model
    }

    fn dims(&self) -> usize {
        self.dims
    }

    fn embed_prompts(&self, prompts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.calls.fetch_add(prompts.len(), Ordering::Relaxed);
        Ok(prompts.iter().map(|p| self.raw(p)).collect())
    }
}

#[allow(clippy::cast_possible_truncation)] // values are < 768
fn index(h: u64) -> usize {
    h as usize
}

/// Drop `task: search result | query: ` / `title: ` and ` | text: ` so queries and documents share words.
fn strip_prompt(prompt: &str) -> String {
    if let Some(q) = prompt.strip_prefix("task: search result | query: ") {
        return q.to_string();
    }
    let body = prompt.strip_prefix("title: ").unwrap_or(prompt);
    match body.split_once(" | text: ") {
        Some(("none", text)) => text.to_string(),
        Some((title, text)) => format!("{title} {text}"),
        None => body.to_string(),
    }
}

fn words(text: String) -> impl Iterator<Item = String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedder::DocText;
    use crate::vector::dot;

    #[test]
    fn is_deterministic() {
        let a = FakeEmbedder::new(256).embed_query("fire bolt").expect("a");
        let b = FakeEmbedder::new(256).embed_query("fire bolt").expect("b");
        assert_eq!(a, b);
    }

    #[test]
    fn shared_words_score_higher() {
        let e = FakeEmbedder::new(128);
        let q = e.embed_query("grappled creature speed").expect("q");
        let docs = e
            .embed_docs(&[
                DocText {
                    title: Some("Grappled"),
                    text: "A grappled creature has speed 0.",
                },
                DocText {
                    title: Some("Fireball"),
                    text: "A bright streak flashes to a point you choose.",
                },
            ])
            .expect("docs");
        assert!(dot(&q, &docs[0]) > dot(&q, &docs[1]));
    }

    #[test]
    fn empty_text_is_still_a_valid_vector() {
        let e = FakeEmbedder::new(128);
        assert!(e
            .embed_docs(&[DocText {
                title: None,
                text: ""
            }])
            .is_ok());
        assert_eq!(e.prompts_embedded(), 1);
    }
}
