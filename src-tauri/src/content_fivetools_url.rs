//! 5etools import from an address the user pastes (decision IM5), and its Update.
//!
//! The data files are downloaded ([`crate::content_fivetools_remote`]) into a scratch folder in
//! the data directory and granted as a `5etools-folder` pick, so scan, source choice and import
//! run exactly as for a chosen folder or a `.zip`. Where the download came from is written next
//! to it; when `import_5etools` finishes on that folder, the address, its revision (commit SHA,
//! or a digest of every file for a plain folder address) and the chosen books are remembered.
//! Update checks the revision first and re-imports the same books only when it changed; the
//! packs are replaced in place, so characters keep their imported ids. No address ships with
//! the app. Deterministic: no AI is involved.

use crate::content_dto::{FiveToolsImportReport, FiveToolsImportRequest};
use crate::content_fetch_retry::{RetryPolicy, Sleep};
use crate::content_fivetools_fetch::RetryingRemote;
use crate::content_fivetools_remote::{
    download_folder, files_revision, parse_location, raw_base, resolve_commit, FileTag,
    FiveToolsLocation, RemoteFetcher, NO_DATA_FOLDER,
};
use crate::content_packs::EventSink;
use crate::content_prefs::FiveToolsUrlRecord;
use crate::error::{ApiError, ApiResult};
use crate::ids::rfc3339;
use crate::picked_paths::{PickPurpose, PickedPathDto};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Scratch folder (in the data directory) the latest download goes into; the `.tmp-` prefix lets
/// the startup sweep remove a stale one.
pub const STAGE_DIR: &str = ".tmp-5etools-url";
/// Written next to the staged `data/` folder.
const ORIGIN_FILE: &str = "origin.json";
/// Characters of a revision shown in the list.
const SHORT_REVISION: usize = 12;

/// Where a staged download came from.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Origin {
    url: String,
    revision: String,
    files: BTreeMap<String, FileTag>,
}

/// A remembered address, for the 5etools tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FiveToolsUrl {
    pub url: String,
    /// Short form of the imported revision.
    pub revision: String,
    pub sources: Vec<String>,
    pub packs: Vec<String>,
    pub last_checked: Option<String>,
    pub last_imported: Option<String>,
    pub problem: Option<String>,
}

/// `update_5etools_url` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FiveToolsUrlUpdate {
    /// Nothing changed at the address; nothing was imported.
    pub up_to_date: bool,
    /// The re-import, when the address changed.
    pub report: Option<FiveToolsImportReport>,
    pub urls: Vec<FiveToolsUrl>,
}

fn dto(r: &FiveToolsUrlRecord) -> FiveToolsUrl {
    FiveToolsUrl {
        url: r.url.clone(),
        revision: r.revision.chars().take(SHORT_REVISION).collect(),
        sources: r.sources.clone(),
        packs: r.packs.clone(),
        last_checked: r.last_checked.clone(),
        last_imported: r.last_imported.clone(),
        problem: r.problem.clone(),
    }
}

/// The address as stored and compared.
fn key(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

/// A full sentence for the UI from a download problem.
fn sentence(why: &str) -> String {
    if why.starts_with(|c: char| c.is_ascii_lowercase()) {
        format!("Could not download the 5etools data: {why}.")
    } else {
        why.to_string()
    }
}

fn now() -> String {
    rfc3339(SystemTime::now())
}

/// Outcome of checking an address against what was imported from it.
enum Checked {
    UpToDate,
    Staged(Origin),
}

/// Download the folder behind `location` into `dest`, unless `previous` shows it is unchanged.
fn fetch_location(
    f: &dyn RemoteFetcher,
    url: &str,
    location: &FiveToolsLocation,
    dest: &Path,
    previous: Option<&FiveToolsUrlRecord>,
    sink: &dyn EventSink,
) -> Result<Checked, String> {
    let origin = |revision: String, files| Origin {
        url: key(url),
        revision,
        files,
    };
    match location {
        FiveToolsLocation::GitHub {
            owner,
            repo,
            git_ref,
            path,
        } => {
            let sha = resolve_commit(f, owner, repo, git_ref.as_deref())?;
            if previous.is_some_and(|p| p.revision == sha) {
                return Ok(Checked::UpToDate);
            }
            let download = match path {
                Some(p) => {
                    download_folder(f, &raw_base(owner, repo, &sha, Some(p)), dest, None, sink)?
                }
                None => match download_folder(
                    f,
                    &raw_base(owner, repo, &sha, Some("data")),
                    dest,
                    None,
                    sink,
                ) {
                    Err(e) if e == NO_DATA_FOLDER => {
                        download_folder(f, &raw_base(owner, repo, &sha, None), dest, None, sink)?
                    }
                    other => other?,
                },
            };
            Ok(Checked::Staged(origin(sha, download.files)))
        }
        FiveToolsLocation::Folder { base } => {
            let download = download_folder(f, base, dest, previous.map(|p| &p.files), sink)?;
            if !download.changed {
                return Ok(Checked::UpToDate);
            }
            Ok(Checked::Staged(origin(
                files_revision(&download.files),
                download.files,
            )))
        }
    }
}

impl AppState {
    fn url_stage(&self) -> ApiResult<PathBuf> {
        let stage = self.paths.data.join(STAGE_DIR);
        if stage.exists() {
            std::fs::remove_dir_all(&stage).map_err(|e| ApiError::io("prepare the download", e))?;
        }
        Ok(stage.join(uuid::Uuid::new_v4().simple().to_string()))
    }

    /// Check `url` (against `previous`) and, when it changed or is new, download it into a fresh
    /// stage folder and grant it as a `5etools-folder` pick. Every request goes through one
    /// [`RetryingRemote`]; a file that still fails stops the download and nothing is staged.
    fn check_and_stage(
        &self,
        f: &dyn RemoteFetcher,
        url: &str,
        previous: Option<&FiveToolsUrlRecord>,
        sink: &dyn EventSink,
        retry: (RetryPolicy, Sleep<'_>),
    ) -> ApiResult<Option<PickedPathDto>> {
        let location = parse_location(url).map_err(ApiError::invalid)?;
        let retrying = RetryingRemote::new(f, retry.0, retry.1);
        let f: &dyn RemoteFetcher = &retrying;
        let _guard = self.content_lock()?;
        let dir = self.url_stage()?;
        let data = dir.join("data");
        let checked = fetch_location(f, url, &location, &data, previous, sink);
        let origin = match checked {
            Ok(Checked::Staged(origin)) => origin,
            Ok(Checked::UpToDate) => {
                let _ = std::fs::remove_dir_all(self.paths.data.join(STAGE_DIR));
                return Ok(None);
            }
            Err(why) => {
                let _ = std::fs::remove_dir_all(self.paths.data.join(STAGE_DIR));
                tracing::warn!("5etools download failed: {why}");
                return Err(ApiError::invalid(sentence(&why)));
            }
        };
        let text = serde_json::to_string(&origin)?;
        std::fs::write(dir.join(ORIGIN_FILE), text)
            .map_err(|e| ApiError::io("save the download", e))?;
        tracing::info!(files = origin.files.len(), "5etools address downloaded");
        self.picked
            .grant(data, PickPurpose::FivetoolsFolder)
            .map(Some)
    }

    /// Download the 5etools data folder at `url` and return a `5etools-folder` token for
    /// `scan_5etools_folder` and `import_5etools`. The previous download is removed.
    ///
    /// # Errors
    /// `invalid_argument` for an unusable address or download.
    pub fn stage_5etools_url(
        &self,
        f: &dyn RemoteFetcher,
        url: &str,
        sink: &dyn EventSink,
    ) -> ApiResult<PickedPathDto> {
        self.stage_5etools_url_with(f, url, sink, RetryPolicy::default(), &std::thread::sleep)
    }

    /// [`AppState::stage_5etools_url`] with an explicit retry policy and pause function (tests).
    ///
    /// # Errors
    /// As [`AppState::stage_5etools_url`].
    pub fn stage_5etools_url_with(
        &self,
        f: &dyn RemoteFetcher,
        url: &str,
        sink: &dyn EventSink,
        policy: RetryPolicy,
        sleep: Sleep<'_>,
    ) -> ApiResult<PickedPathDto> {
        let picked = self
            .check_and_stage(f, url, None, sink, (policy, sleep))?
            .ok_or_else(|| ApiError::invalid(NO_DATA_FOLDER))?;
        Ok(PickedPathDto {
            name: key(url),
            path: key(url),
            ..picked
        })
    }

    /// The remembered addresses.
    #[must_use]
    pub fn list_5etools_urls(&self) -> Vec<FiveToolsUrl> {
        self.content_prefs()
            .fivetools_urls
            .iter()
            .map(dto)
            .collect()
    }

    /// Forget an address (its packs stay installed).
    ///
    /// # Errors
    /// `io_error`.
    pub fn remove_5etools_url(&self, url: &str) -> ApiResult<Vec<FiveToolsUrl>> {
        let _guard = self.content_lock()?;
        let mut prefs = self.content_prefs();
        prefs.fivetools_urls.retain(|r| r.url != key(url));
        prefs.save(&self.paths.prefs_file())?;
        Ok(prefs.fivetools_urls.iter().map(dto).collect())
    }

    fn save_url_record(&self, record: FiveToolsUrlRecord) -> ApiResult<()> {
        let mut prefs = self.content_prefs();
        match prefs
            .fivetools_urls
            .iter_mut()
            .find(|r| r.url == record.url)
        {
            Some(slot) => *slot = record,
            None => prefs.fivetools_urls.push(record),
        }
        prefs.save(&self.paths.prefs_file())
    }

    /// Check a remembered address; re-import its books only when it changed.
    ///
    /// # Errors
    /// `invalid_argument` for an unknown address or a failed download (saved as its problem).
    pub fn update_5etools_url(
        &self,
        f: &dyn RemoteFetcher,
        url: &str,
        sink: &dyn EventSink,
    ) -> ApiResult<FiveToolsUrlUpdate> {
        self.update_5etools_url_with(f, url, sink, RetryPolicy::default(), &std::thread::sleep)
    }

    /// [`AppState::update_5etools_url`] with an explicit retry policy and pause function (tests).
    /// A failed download keeps the remembered revision, files and packs (the next Update tries
    /// again) and saves the failure as the address's problem.
    ///
    /// # Errors
    /// As [`AppState::update_5etools_url`].
    pub fn update_5etools_url_with(
        &self,
        f: &dyn RemoteFetcher,
        url: &str,
        sink: &dyn EventSink,
        policy: RetryPolicy,
        sleep: Sleep<'_>,
    ) -> ApiResult<FiveToolsUrlUpdate> {
        let record = self
            .content_prefs()
            .fivetools_urls
            .into_iter()
            .find(|r| r.url == key(url))
            .ok_or_else(|| ApiError::invalid("That address has not been imported yet."))?;
        tracing::info!("5etools address update started");
        let staged =
            match self.check_and_stage(f, &record.url, Some(&record), sink, (policy, sleep)) {
                Ok(s) => s,
                Err(e) => {
                    let _guard = self.content_lock()?;
                    let failed = FiveToolsUrlRecord {
                        last_checked: Some(now()),
                        problem: Some(e.message.clone()),
                        ..record
                    };
                    self.save_url_record(failed)?;
                    return Err(e);
                }
            };
        let Some(picked) = staged else {
            let _guard = self.content_lock()?;
            let checked = FiveToolsUrlRecord {
                last_checked: Some(now()),
                problem: None,
                ..record
            };
            self.save_url_record(checked)?;
            tracing::info!("5etools address unchanged");
            return Ok(FiveToolsUrlUpdate {
                up_to_date: true,
                report: None,
                urls: self.list_5etools_urls(),
            });
        };
        let report = self.reimport(&picked.token, &record, sink)?;
        Ok(FiveToolsUrlUpdate {
            up_to_date: false,
            report: Some(report),
            urls: self.list_5etools_urls(),
        })
    }

    /// Import the previously chosen books that the new download still has.
    fn reimport(
        &self,
        token: &str,
        record: &FiveToolsUrlRecord,
        sink: &dyn EventSink,
    ) -> ApiResult<FiveToolsImportReport> {
        let scan = self.scan_5etools_folder(token)?;
        let sources: Vec<String> = record
            .sources
            .iter()
            .filter(|s| scan.sources.iter().any(|x| &x.abbr == *s))
            .cloned()
            .collect();
        if sources.is_empty() {
            return Err(ApiError::invalid(
                "None of the books imported before are at this address any more.",
            ));
        }
        let request = FiveToolsImportRequest {
            folder: token.to_string(),
            sources,
            include_fluff: record.include_fluff,
        };
        self.import_5etools(&request, sink)
    }

    /// Called by `import_5etools` after a successful import from `dir`: when `dir` is a staged
    /// download, remember its address, revision and the chosen books (the content lock is held).
    pub(crate) fn remember_5etools_url(
        &self,
        dir: &Path,
        request: &FiveToolsImportRequest,
        packs: &[String],
    ) -> ApiResult<()> {
        let Some(parent) = dir
            .parent()
            .filter(|p| p.starts_with(self.paths.data.join(STAGE_DIR)))
        else {
            return Ok(());
        };
        let Ok(text) = std::fs::read_to_string(parent.join(ORIGIN_FILE)) else {
            return Ok(());
        };
        let origin: Origin = serde_json::from_str(&text)?;
        let stamp = now();
        self.save_url_record(FiveToolsUrlRecord {
            url: origin.url,
            revision: origin.revision,
            files: origin.files,
            sources: request.sources.clone(),
            include_fluff: request.include_fluff,
            packs: packs.to_vec(),
            last_checked: Some(stamp.clone()),
            last_imported: Some(stamp),
            problem: None,
        })
    }
}
