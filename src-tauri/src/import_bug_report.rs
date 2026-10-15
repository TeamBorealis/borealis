//! Opt-in import bug reports (BR1-BR3): build the exact payload for the user to preview, keep it
//! in an outbox, and send only that payload when the user presses Send. Nothing is sent
//! otherwise; there is no telemetry.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use borealis_assist::import::bug_report::{
    bug_report_from_draft, bug_report_from_pack, self_checks_failed, AppInfo, BugReport,
    ReportOptions, MAX_SOURCE_BYTES,
};
use borealis_assist::import::{ImportReport, REPORT_FILE};
use borealis_content::file::ContentFile;
use serde::Serialize;
use serde_json::Value;

use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::import_service::assist_err;
use crate::state::AppState;

/// Where reports go (the Borealis website; private storage, BR3).
pub const REPORT_ENDPOINT: &str = "https://borealisbuilder.com/api/import-report";
/// Largest payload the endpoint accepts.
pub const MAX_PAYLOAD_BYTES: usize = 1_400_000;
/// Previews kept for sending.
const OUTBOX_CAP: usize = 8;
const SEND_TIMEOUT_SECS: u64 = 30;

/// What to report on: a draft under review or an installed import pack.
#[derive(Debug, Clone, Default)]
pub struct ReportRequest {
    /// Draft id (review step).
    pub draft_id: Option<String>,
    /// Installed pack id (after adding to the library).
    pub pack_id: Option<String>,
    /// The user's note.
    pub note: String,
    /// Element ids the user marked as wrong.
    pub flagged: Vec<String>,
    /// The user ticked "include the source".
    pub include_source: bool,
}

/// The exact payload and what the dialog needs to show about it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReportPreview {
    /// Pass to `import_report_send` to send exactly this payload.
    pub preview_id: String,
    /// The payload, as it will be sent.
    pub payload: Value,
    /// Size of the payload in bytes.
    pub bytes: usize,
    /// Import errors, unlocated sections, unsupported content or engine findings exist.
    pub self_checks_failed: bool,
    /// The source text can be attached (drafts only).
    pub source_available: bool,
    /// Size of the source text, when available.
    pub source_bytes: Option<usize>,
    /// Largest source that can be attached.
    pub source_limit: usize,
    /// Where the report would go.
    pub endpoint: String,
}

/// Answer of a successful send.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReportSent {
    /// Report id given by the server.
    pub id: String,
}

/// Posts a payload; the real one is HTTPS, tests use a fake.
pub trait ReportPost: Send + Sync {
    /// POST `body` as JSON to `url`; the server's report id on success.
    ///
    /// # Errors
    /// A short reason.
    fn post(&self, url: &str, body: &str) -> Result<String, String>;
}

/// Previews built in this session, newest last, and the endpoint they go to.
pub struct ReportOutbox {
    endpoint: String,
    poster: Arc<dyn ReportPost>,
    items: Mutex<VecDeque<(String, String)>>,
}

impl Default for ReportOutbox {
    fn default() -> Self {
        Self::new(REPORT_ENDPOINT, Arc::new(HttpsPost))
    }
}

impl ReportOutbox {
    /// An outbox posting to `endpoint` with `poster` (tests override both).
    #[must_use]
    pub fn new(endpoint: &str, poster: Arc<dyn ReportPost>) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            poster,
            items: Mutex::new(VecDeque::new()),
        }
    }

    fn keep(&self, body: String) -> ApiResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut items = self.items.lock()?;
        if items.len() >= OUTBOX_CAP {
            items.pop_front();
        }
        items.push_back((id.clone(), body));
        Ok(id)
    }

    /// Send the previewed payload `preview_id` (removed from the outbox once sent).
    ///
    /// # Errors
    /// `not_found` for an unknown preview, `io_error` when the server refuses or is unreachable.
    pub fn send(&self, preview_id: &str) -> ApiResult<ImportReportSent> {
        let body = self
            .items
            .lock()?
            .iter()
            .find(|(id, _)| id == preview_id)
            .map(|(_, b)| b.clone())
            .ok_or_else(|| {
                ApiError::not_found("This report preview has expired; open it again.")
            })?;
        let id = self.poster.post(&self.endpoint, &body).map_err(|why| {
            ApiError::new(
                ErrorCode::IoError,
                format!("The report could not be sent: {why}"),
            )
        })?;
        self.items.lock()?.retain(|(i, _)| i != preview_id);
        Ok(ImportReportSent { id })
    }
}

impl AppState {
    /// Build the report for `req` and keep it in `outbox` for sending.
    ///
    /// # Errors
    /// `invalid_argument` without exactly one of draft/pack or for a too-large payload,
    /// `not_found` for an unknown draft or pack.
    pub fn import_report_preview(
        &self,
        outbox: &ReportOutbox,
        req: ReportRequest,
    ) -> ApiResult<ImportReportPreview> {
        let app = AppInfo::current(env!("CARGO_PKG_VERSION"));
        let mut options = ReportOptions {
            note: req.note,
            flagged: req.flagged,
            ..ReportOptions::default()
        };
        let (report, source_bytes) = match (req.draft_id.as_deref(), req.pack_id.as_deref()) {
            (Some(id), None) => {
                let stored = self.drafts.get(&self.paths, id)?;
                options.review = stored.review;
                let draft = stored
                    .draft
                    .ok_or_else(|| ApiError::requirement("This import has no draft yet."))?;
                let size = draft.source_text.len();
                if req.include_source && size > 0 {
                    let name = draft.source_name.clone();
                    let kind = draft.source_kind.clone();
                    options.source = Some((name, kind, draft.source_text.clone()));
                }
                let hub = self.hub()?;
                let r = bug_report_from_draft(app, &hub.builder, &draft, options)
                    .map_err(|e| assist_err(&e))?;
                (r, (size > 0).then_some(size))
            }
            (None, Some(id)) => (self.pack_report(id, app, options)?, None),
            _ => {
                return Err(ApiError::invalid(
                    "Report on either an import draft or an imported pack.",
                ))
            }
        };
        let body = serde_json::to_string(&report).map_err(ApiError::internal)?;
        if body.len() > MAX_PAYLOAD_BYTES {
            return Err(ApiError::invalid(
                "This report is too large to send. Leave out the source file.",
            ));
        }
        let failed = self_checks_failed(&report);
        let payload = serde_json::to_value(&report).map_err(ApiError::internal)?;
        Ok(ImportReportPreview {
            bytes: body.len(),
            preview_id: outbox.keep(body)?,
            payload,
            self_checks_failed: failed,
            source_available: source_bytes.is_some(),
            source_bytes,
            source_limit: MAX_SOURCE_BYTES,
            endpoint: outbox.endpoint.clone(),
        })
    }

    fn pack_report(&self, id: &str, app: AppInfo, options: ReportOptions) -> ApiResult<BugReport> {
        let pack = self
            .installed_packs()
            .into_iter()
            .find(|p| !p.bundled && p.manifest.id == id)
            .ok_or_else(|| ApiError::not_found("No imported pack with this id."))?;
        let provenance = std::fs::read_to_string(pack.dir.join("import.json")).unwrap_or_default();
        if !provenance.contains("user-import") {
            return Err(ApiError::invalid("Reports are only for imported packs."));
        }
        let report: ImportReport = std::fs::read_to_string(pack.dir.join(REPORT_FILE))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let content = pack
            .manifest
            .files
            .iter()
            .filter_map(|f| std::fs::read_to_string(pack.dir.join(&f.path)).ok())
            .filter_map(|t| ContentFile::from_json_str(&t).ok())
            .fold(ContentFile::default(), |mut all, f| {
                all.elements.extend(f.elements);
                all
            });
        let edition = pack
            .manifest
            .rulesets
            .first()
            .map(|r| r.as_str().to_string());
        bug_report_from_pack(app, edition, &report, &content, Vec::new(), options)
            .map_err(|e| assist_err(&e))
    }
}

/// HTTPS POST without redirects (desktop).
#[derive(Debug, Clone, Copy, Default)]
pub struct HttpsPost;

impl ReportPost for HttpsPost {
    fn post(&self, url: &str, body: &str) -> Result<String, String> {
        if !url.starts_with("https://") {
            return Err("reports are only sent over HTTPS".into());
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(SEND_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        let response = client
            .post(url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .map_err(|_| "the server could not be reached".to_string())?;
        let status = response.status();
        let answer: Value = response
            .text()
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        if !status.is_success() {
            let why = answer.get("error").and_then(Value::as_str).unwrap_or("");
            return Err(format!("the server answered {} {why}", status.as_u16())
                .trim()
                .to_string());
        }
        Ok(answer
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(80)
            .collect())
    }
}
