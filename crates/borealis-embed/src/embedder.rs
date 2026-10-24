//! The [`Embedder`] trait and the guard that turns semantic mode off on degenerate vectors.
//!
//! A backend implements only [`Embedder::model`], [`Embedder::dims`] and [`Embedder::embed_prompts`] (raw,
//! native-length vectors for already formatted prompts). The provided [`Embedder::embed_query`] and
//! [`Embedder::embed_docs`] apply the spec's prompt templates, truncate to [`Embedder::dims`] (Matryoshka) and
//! L2-normalise after truncation, rejecting NaN / zero-norm output.
//!
//! [`GuardedEmbedder`] wraps a backend for the app: the first degenerate vector trips a shared
//! [`SemanticGuard`], which keeps semantic mode off until the model or settings change ([`SemanticGuard::reset`]),
//! and is reported locally (a log line without the input text, plus [`SemanticGuard::report`] for Settings).

use std::sync::{Arc, Mutex, PoisonError};

use crate::error::{EmbedError, Result};
use crate::prompt::{document_prompt, query_prompt, PROMPT_VERSION};
use crate::vector::{truncate_normalise, Degeneracy, MATRYOSHKA_DIMS};

/// Identity of the loaded model: what a vector store is keyed on (spec sec. 7, 9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    /// Model family id, e.g. `embeddinggemma-2`.
    pub id: String,
    /// Lower-case hex SHA-256 of the weights file.
    pub sha256: String,
    /// Quantisation label, e.g. `Q8_0` (at most 16 ASCII bytes, stored in the vector header).
    pub quant: String,
    /// Native output dimensions (768 for EmbeddingGemma 2).
    pub native_dims: usize,
}

impl ModelInfo {
    /// First 8 hex digits of the SHA-256 (vector file name, spec sec. 7).
    #[must_use]
    pub fn sha8(&self) -> &str {
        self.sha256.get(..8).unwrap_or(&self.sha256)
    }
}

/// One chunk to embed as a document.
#[derive(Debug, Clone, Copy)]
pub struct DocText<'a> {
    /// Chunk title (element name, heading); `None` becomes `none` in the prompt.
    pub title: Option<&'a str>,
    /// Chunk text.
    pub text: &'a str,
}

/// Check that `dims` is a Matryoshka size no larger than the model's.
///
/// # Errors
/// [`EmbedError::Dims`].
pub fn check_dims(dims: usize, native: usize) -> Result<()> {
    if MATRYOSHKA_DIMS.contains(&dims) && dims <= native {
        Ok(())
    } else {
        Err(EmbedError::Dims { dims, native })
    }
}

/// An embedding backend (llama.cpp on device, or [`crate::FakeEmbedder`] in tests).
pub trait Embedder: Send + Sync {
    /// The loaded model.
    fn model(&self) -> &ModelInfo;

    /// Output dimensions after Matryoshka truncation (the device tier's choice).
    fn dims(&self) -> usize;

    /// Version of the prompt templates this backend is fed with.
    fn prompt_version(&self) -> u32 {
        PROMPT_VERSION
    }

    /// Raw vectors, one per formatted prompt, at the model's native length (no truncation or normalisation).
    ///
    /// # Errors
    /// [`EmbedError::Backend`] when the forward pass fails.
    fn embed_prompts(&self, prompts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Embed a search query with `task: search result | query: {q}`.
    ///
    /// # Errors
    /// Backend errors, [`EmbedError::Degenerate`] on NaN / zero-norm output.
    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let mut out = finish(self.embed_prompts(&[query_prompt(query)])?, 1, self.dims())?;
        Ok(out.remove(0))
    }

    /// Embed document chunks with `title: {title|none} | text: {chunk}`.
    ///
    /// # Errors
    /// Backend errors, [`EmbedError::Degenerate`] when any vector is NaN / zero-norm.
    fn embed_docs(&self, docs: &[DocText<'_>]) -> Result<Vec<Vec<f32>>> {
        if docs.is_empty() {
            return Ok(Vec::new());
        }
        let prompts: Vec<String> = docs
            .iter()
            .map(|d| document_prompt(d.title, d.text))
            .collect();
        finish(self.embed_prompts(&prompts)?, docs.len(), self.dims())
    }
}

/// Truncate + normalise each raw vector, checking the count.
fn finish(raw: Vec<Vec<f32>>, want: usize, dims: usize) -> Result<Vec<Vec<f32>>> {
    if raw.len() != want {
        return Err(EmbedError::Count {
            got: raw.len(),
            want,
        });
    }
    raw.iter()
        .map(|v| truncate_normalise(v, dims).map_err(EmbedError::Degenerate))
        .collect()
}

/// Where a degenerate vector showed up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedStage {
    /// Embedding a search query.
    Query,
    /// Embedding document chunks while indexing.
    Index,
}

/// Why semantic mode was turned off (shown in Settings > Search; never sent anywhere).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisabledReport {
    /// What was wrong with the vector.
    pub reason: Degeneracy,
    /// Query or indexing.
    pub stage: EmbedStage,
    /// Model file (first 8 hex digits of its SHA-256) and quantisation.
    pub model_sha8: String,
    pub quant: String,
}

impl DisabledReport {
    /// One plain sentence for Settings.
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "Semantic search was turned off: the {} model ({}) produced an unusable vector ({}). Keyword search \
             still works.",
            self.quant, self.model_sha8, self.reason
        )
    }
}

/// Shared on/off switch for semantic mode on this device. Cloning shares the state.
#[derive(Debug, Clone, Default)]
pub struct SemanticGuard {
    tripped: Arc<Mutex<Option<DisabledReport>>>,
}

impl SemanticGuard {
    /// A guard with semantic mode allowed.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// False after a degenerate vector until [`Self::reset`].
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.report().is_none()
    }

    /// The reason semantic mode is off, if it is.
    #[must_use]
    pub fn report(&self) -> Option<DisabledReport> {
        self.tripped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Allow semantic mode again (new model file, dims or prompt version, or the user turned it back on).
    pub fn reset(&self) {
        *self.tripped.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Turn semantic mode off. The first report wins; it is logged locally without any input text.
    pub fn trip(&self, report: DisabledReport) {
        let mut slot = self.tripped.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
            tracing::warn!(
                reason = %report.reason,
                stage = ?report.stage,
                model = %report.model_sha8,
                quant = %report.quant,
                "semantic search disabled: degenerate embedding"
            );
            *slot = Some(report);
        }
    }
}

/// An [`Embedder`] behind a [`SemanticGuard`]: what search and indexing call.
#[derive(Clone)]
pub struct GuardedEmbedder {
    inner: Arc<dyn Embedder>,
    guard: SemanticGuard,
}

impl GuardedEmbedder {
    /// Wrap `inner`, sharing `guard`.
    #[must_use]
    pub fn new(inner: Arc<dyn Embedder>, guard: SemanticGuard) -> Self {
        Self { inner, guard }
    }

    /// The wrapped backend.
    #[must_use]
    pub fn embedder(&self) -> &dyn Embedder {
        self.inner.as_ref()
    }

    /// The shared guard.
    #[must_use]
    pub fn guard(&self) -> &SemanticGuard {
        &self.guard
    }

    /// [`Embedder::embed_query`], refusing while the guard is tripped and tripping it on a degenerate vector.
    ///
    /// # Errors
    /// [`EmbedError::Disabled`] while off; the backend's error otherwise.
    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        self.check()?;
        self.watch(EmbedStage::Query, self.inner.embed_query(query))
    }

    /// [`Embedder::embed_docs`], with the same guard behaviour.
    ///
    /// # Errors
    /// [`EmbedError::Disabled`] while off; the backend's error otherwise.
    pub fn embed_docs(&self, docs: &[DocText<'_>]) -> Result<Vec<Vec<f32>>> {
        self.check()?;
        self.watch(EmbedStage::Index, self.inner.embed_docs(docs))
    }

    fn check(&self) -> Result<()> {
        match self.guard.report() {
            Some(report) => Err(EmbedError::Disabled(report.message())),
            None => Ok(()),
        }
    }

    fn watch<T>(&self, stage: EmbedStage, result: Result<T>) -> Result<T> {
        if let Err(EmbedError::Degenerate(reason)) = &result {
            let model = self.inner.model();
            self.guard.trip(DisabledReport {
                reason: *reason,
                stage,
                model_sha8: model.sha8().to_string(),
                quant: model.quant.clone(),
            });
        }
        result
    }
}

impl std::fmt::Debug for GuardedEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuardedEmbedder")
            .field("model", self.inner.model())
            .field("dims", &self.inner.dims())
            .field("enabled", &self.guard.is_enabled())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeEmbedder;

    #[test]
    fn dims_must_be_matryoshka_sizes() {
        assert!(check_dims(256, 768).is_ok());
        assert!(check_dims(768, 768).is_ok());
        assert!(check_dims(300, 768).is_err());
        assert!(check_dims(768, 512).is_err());
    }

    #[test]
    fn query_and_docs_are_unit_vectors_of_the_tier_dims() {
        let e = FakeEmbedder::new(128);
        let q = e.embed_query("grapple a creature").expect("query");
        assert_eq!(q.len(), 128);
        assert!((crate::vector::dot(&q, &q) - 1.0).abs() < 1e-5);
        let docs = e
            .embed_docs(&[
                DocText {
                    title: Some("Grappled"),
                    text: "speed 0",
                },
                DocText {
                    title: None,
                    text: "fire damage",
                },
            ])
            .expect("docs");
        assert_eq!(docs.len(), 2);
        assert!(docs.iter().all(|d| d.len() == 128));
    }

    #[test]
    fn nan_output_disables_semantic_mode_and_reports_it() {
        let guard = SemanticGuard::new();
        let e = GuardedEmbedder::new(
            Arc::new(FakeEmbedder::new(256).with_poison("NaNtrigger")),
            guard.clone(),
        );
        assert!(e.embed_query("fireball").is_ok());
        let err = e
            .embed_docs(&[DocText {
                title: None,
                text: "a NaNtrigger chunk",
            }])
            .expect_err("degenerate");
        assert!(matches!(err, EmbedError::Degenerate(Degeneracy::NonFinite)));
        assert!(!guard.is_enabled());
        let report = guard.report().expect("report");
        assert_eq!(report.stage, EmbedStage::Index);
        assert_eq!(report.reason, Degeneracy::NonFinite);
        assert!(report.message().contains("Keyword search still works"));
        // Everything is refused while off, also healthy input.
        assert!(matches!(
            e.embed_query("fireball"),
            Err(EmbedError::Disabled(_))
        ));
        guard.reset();
        assert!(e.embed_query("fireball").is_ok());
    }

    #[test]
    fn a_wrong_vector_count_is_an_error() {
        struct Short(ModelInfo);
        impl Embedder for Short {
            fn model(&self) -> &ModelInfo {
                &self.0
            }
            fn dims(&self) -> usize {
                128
            }
            fn embed_prompts(&self, _: &[String]) -> Result<Vec<Vec<f32>>> {
                Ok(Vec::new())
            }
        }
        let e = Short(FakeEmbedder::new(128).model().clone());
        assert!(matches!(
            e.embed_query("x"),
            Err(EmbedError::Count { got: 0, want: 1 })
        ));
    }
}
