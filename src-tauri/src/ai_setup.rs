//! First-run AI setup: detect claude/codex, check login, install a CLI after explicit
//! consent, and help the user sign in. Borealis never sees credentials: login happens in the
//! vendor's own CLI, in a terminal the user controls.

use crate::ai_profile::ProfileOutcome;
use crate::ai_service::AiService;
use crate::error::{ApiError, ApiResult, ErrorCode};
use crate::state::AppState;
use borealis_ai::{Backend, ProviderInfo};
use serde::Serialize;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

/// Event channel for install output.
pub const EVENT_AI_SETUP: &str = "borealis://ai-setup-event";
const AUTH_TIMEOUT: Duration = Duration::from_secs(8);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);
const MARKER_FILE: &str = "ai-setup.json";

/// One official install method.
struct Method {
    backend: Backend,
    name: &'static str,
    npm_package: &'static str,
    /// Exact version installed: never a floating tag. Bump after checking `npm view`.
    npm_version: &'static str,
    login: &'static str,
}

impl Method {
    /// `package@version`, what `npm install -g` receives.
    fn spec(&self) -> String {
        format!("{}@{}", self.npm_package, self.npm_version)
    }

    /// The exact command shown to the user and run.
    fn command(&self) -> String {
        format!("npm install -g {}", self.spec())
    }
}

const METHODS: [Method; 2] = [
    Method {
        backend: Backend::Claude,
        name: "claude",
        npm_package: "@anthropic-ai/claude-code",
        npm_version: "2.1.293",
        login: "claude auth login",
    },
    Method {
        backend: Backend::Codex,
        name: "codex",
        npm_package: "@openai/codex",
        npm_version: "0.161.0",
        login: "codex login",
    },
];

fn method(name: &str) -> ApiResult<&'static Method> {
    METHODS.iter().find(|m| m.name == name).ok_or_else(|| {
        ApiError::new(
            ErrorCode::InvalidArgument,
            format!("Unknown provider {name:?}."),
        )
    })
}

/// Per-provider setup row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetupDto {
    /// `claude` or `codex`.
    pub backend: String,
    /// CLI found on this machine.
    pub installed: bool,
    /// Executable path.
    pub path: Option<String>,
    /// Reported version.
    pub version: Option<String>,
    /// `Some(true/false)` once checked; `None` when not installed or the check was inconclusive.
    pub logged_in: Option<bool>,
    /// The command Borealis would run to install it (npm).
    pub install_command: String,
    /// The command the user runs to sign in.
    pub login_command: String,
}

/// Profile state shown in the setup screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileStatusDto {
    /// `installed`, `updated`, `verified`, `repaired`, `unavailable` (mobile) or `error`.
    pub state: String,
    /// Tampered paths when repaired, or the error text.
    pub detail: Vec<String>,
}

/// `ai_setup_status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSetupStatusDto {
    /// One row per supported CLI.
    pub providers: Vec<ProviderSetupDto>,
    /// Path of `npm` when found (required for in-app install).
    pub npm_path: Option<String>,
    /// Borealis AI profile status.
    pub profile: ProfileStatusDto,
    /// The user finished or skipped the setup before.
    pub completed: bool,
    /// No AI setup on this platform (phones pair with a desktop instead).
    pub mobile: bool,
}

/// `ai_setup_confirm_install`: a one-time consent token for one provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallConsent {
    /// Pass back to `ai_setup_install`.
    pub token: String,
    /// The exact command that will run, shown in the dialog.
    pub command: String,
}

/// `ai_setup_login`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginHelp {
    /// Command to run in a terminal.
    pub command: String,
    /// A terminal window was opened with the command.
    pub opened: bool,
}

/// Streamed install output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetupEvent {
    /// A line of npm output.
    Output { provider: String, line: String },
    /// Finished; `ok` false on failure (see `message`).
    Done {
        provider: String,
        ok: bool,
        message: String,
    },
}

/// Receives install output.
pub trait SetupSink: Send + Sync {
    /// Deliver one event.
    fn emit(&self, event: &SetupEvent);
}

/// How long an install consent stays valid.
pub const CONSENT_TTL: Duration = Duration::from_secs(60);
/// Most outstanding install consents; older ones are dropped.
pub const MAX_CONSENTS: usize = 4;

/// One outstanding install consent.
struct Consent {
    provider: &'static str,
    at: Instant,
}

/// Drop expired consents and make room for one more.
fn prune(map: &mut HashMap<String, Consent>) {
    map.retain(|_, c| c.at.elapsed() < CONSENT_TTL);
    while map.len() >= MAX_CONSENTS {
        let Some(oldest) = map.iter().min_by_key(|(_, c)| c.at).map(|(k, _)| k.clone()) else {
            break;
        };
        map.remove(&oldest);
    }
}

/// Setup state kept on the AI service: consent tokens and the startup profile outcome.
#[derive(Default)]
pub struct SetupState {
    consents: Mutex<HashMap<String, Consent>>,
    profile: Mutex<Option<ProfileStatusDto>>,
}

impl SetupState {
    /// Record what startup did to the profile.
    pub fn set_profile_outcome(&self, outcome: &Result<ProfileOutcome, String>) {
        let dto = match outcome {
            Ok(ProfileOutcome::Installed) => ("installed", vec![]),
            Ok(ProfileOutcome::Updated) => ("updated", vec![]),
            Ok(ProfileOutcome::Verified) => ("verified", vec![]),
            Ok(ProfileOutcome::Repaired(f)) => ("repaired", f.clone()),
            Err(e) => ("error", vec![e.clone()]),
        };
        if let Ok(mut slot) = self.profile.lock() {
            *slot = Some(ProfileStatusDto {
                state: dto.0.into(),
                detail: dto.1,
            });
        }
    }
}

fn find_on_path(name: &str, path_env: Option<&OsString>) -> Option<PathBuf> {
    let var = path_env.cloned().or_else(|| std::env::var_os("PATH"))?;
    // Windows: `npm.cmd`, never the extension-less `#!/bin/sh` shim npm installs beside it.
    let names = borealis_ai::spawn::host_launcher_names(name);
    std::env::split_paths(&var)
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| p.is_file())
}

/// Run `exe args` with a timeout; stdout and exit success.
async fn probe(exe: &Path, args: &[&str], path_env: Option<&OsString>) -> Option<(bool, String)> {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    borealis_ai::spawn::hide_console(&mut cmd);
    if let Some(p) = path_env {
        cmd.env("PATH", p);
    }
    let out = tokio::time::timeout(AUTH_TIMEOUT, cmd.output())
        .await
        .ok()?
        .ok()?;
    Some((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

/// Cheap login check using each CLI's documented status command.
async fn logged_in(p: &ProviderInfo, path_env: Option<&OsString>) -> Option<bool> {
    match p.backend {
        // `claude auth status` prints JSON with `loggedIn`.
        Backend::Claude => {
            let (ok, out) = probe(&p.path, &["auth", "status"], path_env).await?;
            serde_json::from_str::<serde_json::Value>(&out)
                .ok()
                .and_then(|v| v.get("loggedIn").and_then(serde_json::Value::as_bool))
                .or((!ok).then_some(false))
        }
        // `codex login status` exits 0 only when signed in.
        Backend::Codex => probe(&p.path, &["login", "status"], path_env)
            .await
            .map(|(ok, _)| ok),
    }
}

fn marker_path(state: &AppState) -> PathBuf {
    state.paths.data.join(MARKER_FILE)
}

fn row(m: &Method, found: Option<&ProviderInfo>, logged_in: Option<bool>) -> ProviderSetupDto {
    ProviderSetupDto {
        backend: m.name.into(),
        installed: found.is_some(),
        path: found.map(|p| p.path.to_string_lossy().into_owned()),
        version: found.and_then(|p| p.version.clone()),
        logged_in,
        install_command: m.command(),
        login_command: m.login.into(),
    }
}

impl AiService {
    /// `ai_setup_status`.
    pub async fn setup_status(&self, state: &AppState) -> ApiResult<AiSetupStatusDto> {
        let mobile = cfg!(any(target_os = "android", target_os = "ios"));
        let ai = state.settings()?.ai;
        let found = self.providers(&ai).await;
        let path_env = self.detect_path_env();
        let mut providers = Vec::new();
        for m in &METHODS {
            let p = found.iter().find(|p| p.backend == m.backend);
            let login = match p {
                Some(p) => logged_in(p, path_env.as_ref()).await,
                None => None,
            };
            providers.push(row(m, p, login));
        }
        let profile = self
            .setup
            .profile
            .lock()?
            .clone()
            .unwrap_or(ProfileStatusDto {
                state: "unavailable".into(),
                detail: vec![],
            });
        Ok(AiSetupStatusDto {
            providers,
            npm_path: find_on_path("npm", path_env.as_ref())
                .map(|p| p.to_string_lossy().into_owned()),
            profile,
            completed: marker_path(state).is_file(),
            mobile,
        })
    }

    /// `ai_setup_confirm_install`: the UI calls this when the user accepts the consent dialog.
    ///
    /// # Errors
    /// `invalid_argument` for an unknown provider; `requirement_failed` when npm is missing.
    pub fn setup_confirm_install(&self, provider: &str) -> ApiResult<InstallConsent> {
        let m = method(provider)?;
        if find_on_path("npm", self.detect_path_env().as_ref()).is_none() {
            return Err(ApiError::new(
                ErrorCode::RequirementFailed,
                "npm was not found. Install Node.js from nodejs.org, or follow the CLI's own install guide.",
            ));
        }
        let token = uuid::Uuid::new_v4().to_string();
        let mut consents = self.setup.consents.lock()?;
        prune(&mut consents);
        consents.insert(
            token.clone(),
            Consent {
                provider: m.name,
                at: Instant::now(),
            },
        );
        drop(consents);
        Ok(InstallConsent {
            token,
            command: m.command(),
        })
    }

    /// The exact install command a live consent token covers (for the backend's native confirm
    /// dialog). The token is not spent.
    ///
    /// # Errors
    /// `requirement_failed` for an unknown, expired or other-provider token.
    pub fn consent_command(&self, provider: &str, consent_token: &str) -> ApiResult<String> {
        let m = method(provider)?;
        let mut consents = self.setup.consents.lock()?;
        consents.retain(|_, c| c.at.elapsed() < CONSENT_TTL);
        match consents.get(consent_token) {
            Some(c) if c.provider == m.name => Ok(m.command()),
            _ => Err(ApiError::new(
                ErrorCode::RequirementFailed,
                "Installation needs your explicit confirmation first.",
            )),
        }
    }

    /// Spend a consent token without installing (the native confirm was declined).
    pub fn revoke_consent(&self, consent_token: &str) {
        if let Ok(mut c) = self.setup.consents.lock() {
            c.remove(consent_token);
        }
    }

    /// `ai_setup_install`: runs the official npm install, only with a valid one-time consent token.
    ///
    /// # Errors
    /// `requirement_failed` without a matching unused token, or when npm is missing / the install fails.
    pub async fn setup_install(
        &self,
        provider: &str,
        consent_token: &str,
        sink: Arc<dyn SetupSink>,
    ) -> ApiResult<()> {
        let m = method(provider)?;
        let granted = self.setup.consents.lock()?.remove(consent_token);
        if !granted.is_some_and(|c| c.provider == m.name && c.at.elapsed() < CONSENT_TTL) {
            return Err(ApiError::new(
                ErrorCode::RequirementFailed,
                "Installation needs your explicit confirmation first.",
            ));
        }
        let path_env = self.detect_path_env();
        let npm = find_on_path("npm", path_env.as_ref()).ok_or_else(|| {
            ApiError::new(
                ErrorCode::RequirementFailed,
                "npm was not found; install Node.js first.",
            )
        })?;
        let result = run_npm(&npm, m, path_env.as_ref(), &sink).await;
        let (ok, message) = match &result {
            Ok(()) => (true, "Installed.".to_string()),
            Err(e) => (false, e.clone()),
        };
        sink.emit(&SetupEvent::Done {
            provider: m.name.into(),
            ok,
            message: message.clone(),
        });
        result.map_err(|e| ApiError::new(ErrorCode::IoError, e))
    }

    /// `ai_setup_login`: the command to run, optionally opened in a terminal (macOS only).
    ///
    /// # Errors
    /// `invalid_argument` for an unknown provider.
    pub fn setup_login(&self, provider: &str, open_terminal: bool) -> ApiResult<LoginHelp> {
        let m = method(provider)?;
        let opened = open_terminal && open_in_terminal(m.login);
        Ok(LoginHelp {
            command: m.login.into(),
            opened,
        })
    }

    /// `ai_setup_complete`: remember that the first-run screen was done or skipped.
    ///
    /// # Errors
    /// `io_error` when the marker cannot be written.
    pub fn setup_complete(&self, state: &AppState, skipped: bool) -> ApiResult<()> {
        let body = serde_json::json!({ "completed": true, "skipped": skipped });
        std::fs::write(marker_path(state), body.to_string()).map_err(|e| {
            ApiError::new(
                ErrorCode::IoError,
                format!("Could not save setup state: {e}"),
            )
        })
    }
}

/// Spawn a Terminal window running `command` (macOS). Other platforms show the command only.
fn open_in_terminal(command: &str) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    // `command` comes from the constant table above, never from the UI.
    let script = format!("tell application \"Terminal\" to activate\ntell application \"Terminal\" to do script \"{command}\"");
    std::process::Command::new("osascript")
        .args(["-e", &script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

async fn pump(reader: impl AsyncRead + Unpin, provider: &'static str, sink: Arc<dyn SetupSink>) {
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        sink.emit(&SetupEvent::Output {
            provider: provider.into(),
            line,
        });
    }
}

/// Kill npm and everything it spawned.
async fn kill_tree(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    let (program, args) = (
        "/bin/kill",
        vec!["-s".into(), "KILL".into(), "--".into(), format!("-{pid}")],
    );
    #[cfg(windows)]
    let (program, args) = (
        "taskkill",
        vec![
            "/T".to_string(),
            "/F".into(),
            "/PID".into(),
            pid.to_string(),
        ],
    );
    let _ = borealis_ai::spawn::hide_console(
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .status()
    .await;
}

async fn run_npm(
    npm: &Path,
    m: &Method,
    path_env: Option<&OsString>,
    sink: &Arc<dyn SetupSink>,
) -> Result<(), String> {
    let mut cmd = Command::new(npm);
    cmd.args(["install", "-g", &m.spec()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Own process group, so a timeout can stop npm and the lifecycle scripts it started.
    #[cfg(unix)]
    cmd.process_group(0);
    borealis_ai::spawn::hide_console(&mut cmd);
    if let Some(p) = path_env {
        cmd.env("PATH", p);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start npm: {e}"))?;
    let out = child
        .stdout
        .take()
        .map(|r| tokio::spawn(pump(r, m.name, Arc::clone(sink))));
    let err = child
        .stderr
        .take()
        .map(|r| tokio::spawn(pump(r, m.name, Arc::clone(sink))));
    let pid = child.id();
    let status = match tokio::time::timeout(INSTALL_TIMEOUT, child.wait()).await {
        Ok(r) => r.map_err(|e| format!("npm failed: {e}"))?,
        Err(_) => {
            kill_tree(pid).await;
            let _ = child.kill().await;
            return Err("The install timed out.".to_string());
        }
    };
    for h in [out, err].into_iter().flatten() {
        let _ = h.await;
    }
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "npm exited with {}. If this is a permissions error, run the command in a terminal.",
            status.code().map_or("a signal".into(), |c| c.to_string())
        ))
    }
}
