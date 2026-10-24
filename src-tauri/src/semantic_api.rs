//! Settings > Search semantic model actions on [`AppState`] (local-search-spec sec. 8, 9; S3.2).
//!
//! `start` is the consent: only the consent dialog's "Download and turn on" (or Resume of a download the user
//! started) calls it, and it turns `settings.search.semantic` on. Cancel and delete turn it off. Status, settings
//! changes and app start never make a request.

use crate::error::{ApiError, ApiResult};
use crate::semantic_service::{SemanticContext, SemanticStatus};
use crate::settings::SearchTier;
use crate::state::AppState;
use borealis_embed::tier::effective_tier;
use borealis_embed::{DeviceProfile, Lifecycle, Tier, TierOverride};
use serde::Serialize;
use serde_json::json;

/// `semantic_model_license` result: the model's licence and NOTICE (Settings > About, EM6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticLicense {
    pub model: String,
    pub license: String,
    pub notice: String,
}

/// The settings override as the crate's type.
#[must_use]
pub fn tier_override(t: SearchTier) -> TierOverride {
    match t {
        SearchTier::Auto => TierOverride::Auto,
        SearchTier::T1 => TierOverride::T1,
        SearchTier::T2 => TierOverride::T2,
        SearchTier::T3 => TierOverride::T3,
    }
}

impl AppState {
    /// The status context for a probed device profile and the current settings.
    ///
    /// # Errors
    /// `internal` if the settings lock is poisoned.
    pub fn semantic_context(
        &self,
        profile: &DeviceProfile,
        mobile: bool,
    ) -> ApiResult<SemanticContext> {
        let search = self.settings()?.search;
        let probed = profile.tier();
        Ok(SemanticContext {
            probed,
            tier: effective_tier(probed, tier_override(search.tier_override)),
            mobile,
            semantic: search.semantic,
            t0_reason: Tier::unavailable_reason(profile),
        })
    }

    fn with_semantic(&self, ctx: &SemanticContext, on: bool) -> ApiResult<SemanticStatus> {
        let saved = self.save_settings(&json!({ "search": { "semantic": on } }))?;
        Ok(self.semantic.status(&SemanticContext {
            semantic: saved.search.semantic,
            ..*ctx
        }))
    }

    /// Settings > Search status (no network).
    #[must_use]
    pub fn semantic_status(&self, ctx: &SemanticContext) -> SemanticStatus {
        self.semantic.status(ctx)
    }

    /// The user consented: start (or resume) the download for this device's tier and turn semantic search on.
    ///
    /// # Errors
    /// `requirement_failed` when the device has no file (T0) or no URL serves it yet; `io_error` if the
    /// settings cannot be saved.
    pub fn semantic_start(
        &self,
        ctx: &SemanticContext,
        lifecycle: Option<Lifecycle>,
    ) -> ApiResult<SemanticStatus> {
        self.semantic
            .start(ctx.tier, lifecycle)
            .map_err(ApiError::requirement)?;
        self.with_semantic(ctx, true)
    }

    /// Pause the running download (the partial is kept for Resume).
    #[must_use]
    pub fn semantic_pause(&self, ctx: &SemanticContext) -> SemanticStatus {
        self.semantic.pause();
        self.semantic.status(ctx)
    }

    /// Cancel the download, delete the partial and turn semantic search off.
    ///
    /// # Errors
    /// `io_error` if the partial or the settings cannot be written.
    pub fn semantic_cancel(&self, ctx: &SemanticContext) -> ApiResult<SemanticStatus> {
        self.semantic
            .cancel()
            .map_err(|e| ApiError::io("delete the partial download", e))?;
        self.with_semantic(ctx, false)
    }

    /// "Delete model and semantic indexes": the model files and every `vectors-*` file go, keyword indexes stay,
    /// semantic search turns off.
    ///
    /// # Errors
    /// `io_error` if a file or the settings cannot be written.
    pub fn semantic_delete(&self, ctx: &SemanticContext) -> ApiResult<SemanticStatus> {
        self.semantic
            .delete_model_and_vectors()
            .map_err(|e| ApiError::io("delete the semantic search model", e))?;
        self.with_semantic(ctx, false)
    }
}

/// The bundled model licence and NOTICE.
#[must_use]
pub fn model_license() -> SemanticLicense {
    SemanticLicense {
        model: "EmbeddingGemma 2".into(),
        license: borealis_embed::models::MODEL_LICENSE.into(),
        notice: borealis_embed::models::MODEL_NOTICE.into(),
    }
}
