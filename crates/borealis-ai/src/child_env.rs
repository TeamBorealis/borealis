//! Environment allowlist for the CLI child process.
//!
//! The child starts from an empty environment and receives only the variables in
//! [`ALLOWED_ENV`] (plus the caller's explicit [`crate::SessionOptions::env`] extras). Anything
//! else in the parent environment, such as tokens for unrelated services, never reaches the CLI
//! or the shell tool it may run. Keep this table the single place to audit.

use std::ffi::{OsStr, OsString};

use crate::Backend;

/// Which backends receive a variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvScope {
    /// Every backend.
    All,
    /// `claude` only.
    Claude,
    /// `codex` only.
    Codex,
}

impl EnvScope {
    fn applies_to(self, backend: Backend) -> bool {
        match self {
            EnvScope::All => true,
            EnvScope::Claude => backend == Backend::Claude,
            EnvScope::Codex => backend == Backend::Codex,
        }
    }
}

/// Variables passed through from the parent. A trailing `*` is a prefix match. Names compare
/// case-insensitively on Windows (where `Path` and `PATH` are the same variable).
pub const ALLOWED_ENV: &[(&str, EnvScope)] = &[
    // Process basics.
    ("PATH", EnvScope::All),
    ("HOME", EnvScope::All),
    ("USER", EnvScope::All),
    ("LOGNAME", EnvScope::All),
    ("LANG", EnvScope::All),
    ("LC_*", EnvScope::All),
    ("TMPDIR", EnvScope::All),
    ("TEMP", EnvScope::All),
    ("TMP", EnvScope::All),
    // Linux config and credential-store lookup (Secret Service needs the session bus).
    ("XDG_CONFIG_HOME", EnvScope::All),
    ("XDG_DATA_HOME", EnvScope::All),
    ("XDG_CACHE_HOME", EnvScope::All),
    ("XDG_RUNTIME_DIR", EnvScope::All),
    ("DBUS_SESSION_BUS_ADDRESS", EnvScope::All),
    // Windows basics (the CLIs are often `.cmd` shims, which need PATHEXT and ComSpec).
    ("SystemRoot", EnvScope::All),
    ("SystemDrive", EnvScope::All),
    ("windir", EnvScope::All),
    ("ComSpec", EnvScope::All),
    ("PATHEXT", EnvScope::All),
    ("APPDATA", EnvScope::All),
    ("LOCALAPPDATA", EnvScope::All),
    ("USERPROFILE", EnvScope::All),
    // Network: proxies and extra CA roots, or the CLI cannot reach its API behind a proxy.
    ("HTTPS_PROXY", EnvScope::All),
    ("HTTP_PROXY", EnvScope::All),
    ("NO_PROXY", EnvScope::All),
    ("https_proxy", EnvScope::All),
    ("http_proxy", EnvScope::All),
    ("no_proxy", EnvScope::All),
    ("SSL_CERT_FILE", EnvScope::All),
    ("NODE_EXTRA_CA_CERTS", EnvScope::All),
    // Claude Code auth and config.
    ("ANTHROPIC_API_KEY", EnvScope::Claude),
    ("ANTHROPIC_AUTH_TOKEN", EnvScope::Claude),
    ("ANTHROPIC_BASE_URL", EnvScope::Claude),
    ("CLAUDE_CODE_OAUTH_TOKEN", EnvScope::Claude),
    ("CLAUDE_CONFIG_DIR", EnvScope::Claude),
    ("CLAUDE_CODE_USE_BEDROCK", EnvScope::Claude),
    ("CLAUDE_CODE_USE_VERTEX", EnvScope::Claude),
    ("AWS_REGION", EnvScope::Claude),
    ("AWS_PROFILE", EnvScope::Claude),
    ("AWS_ACCESS_KEY_ID", EnvScope::Claude),
    ("AWS_SECRET_ACCESS_KEY", EnvScope::Claude),
    ("AWS_SESSION_TOKEN", EnvScope::Claude),
    ("AWS_BEARER_TOKEN_BEDROCK", EnvScope::Claude),
    ("ANTHROPIC_VERTEX_PROJECT_ID", EnvScope::Claude),
    ("CLOUD_ML_REGION", EnvScope::Claude),
    ("GOOGLE_APPLICATION_CREDENTIALS", EnvScope::Claude),
    // Codex auth and config.
    ("OPENAI_API_KEY", EnvScope::Codex),
    ("OPENAI_BASE_URL", EnvScope::Codex),
    ("CODEX_HOME", EnvScope::Codex),
];

/// Variables always set for the child, after the allowlist.
pub const FIXED_ENV: &[(&str, &str)] = &[("TERM", "dumb"), ("NO_COLOR", "1")];

fn name_matches(pattern: &str, name: &str) -> bool {
    let eq = |a: &str, b: &str| {
        if cfg!(windows) {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    match pattern.strip_suffix('*') {
        Some(prefix) => name
            .get(..prefix.len())
            .is_some_and(|head| eq(head, prefix)),
        None => eq(pattern, name),
    }
}

/// Whether `name` may pass from the parent to a `backend` child.
pub fn is_allowed(backend: Backend, name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    ALLOWED_ENV
        .iter()
        .any(|(pattern, scope)| scope.applies_to(backend) && name_matches(pattern, name))
}

/// The filtered environment for a `backend` child, from `parent` (normally
/// `std::env::vars_os()`), followed by [`FIXED_ENV`]. The caller still adds its own extras.
pub fn child_env(
    backend: Backend,
    parent: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, OsString)> {
    let mut out: Vec<(OsString, OsString)> = parent
        .into_iter()
        .filter(|(k, _)| is_allowed(backend, k))
        .filter(|(k, _)| !FIXED_ENV.iter().any(|(f, _)| OsStr::new(f) == k))
        .collect();
    out.extend(
        FIXED_ENV
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v))),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent(vars: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        vars.iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v)))
            .collect()
    }

    fn names(env: &[(OsString, OsString)]) -> Vec<String> {
        env.iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn unknown_variables_are_dropped() {
        let env = child_env(
            Backend::Claude,
            parent(&[
                ("PATH", "/bin"),
                ("BOREALIS_TEST_SECRET", "leak"),
                ("GITHUB_TOKEN", "leak"),
                ("LC_ALL", "C"),
            ]),
        );
        let n = names(&env);
        assert!(n.contains(&"PATH".into()) && n.contains(&"LC_ALL".into()));
        assert!(!n
            .iter()
            .any(|k| k == "BOREALIS_TEST_SECRET" || k == "GITHUB_TOKEN"));
    }

    #[test]
    fn auth_variables_go_only_to_their_own_cli() {
        let vars = parent(&[("ANTHROPIC_API_KEY", "a"), ("OPENAI_API_KEY", "o")]);
        let claude = names(&child_env(Backend::Claude, vars.clone()));
        let codex = names(&child_env(Backend::Codex, vars));
        assert!(claude.contains(&"ANTHROPIC_API_KEY".into()));
        assert!(!claude.contains(&"OPENAI_API_KEY".into()));
        assert!(codex.contains(&"OPENAI_API_KEY".into()));
        assert!(!codex.contains(&"ANTHROPIC_API_KEY".into()));
    }

    #[test]
    fn term_is_forced_to_dumb() {
        let env = child_env(Backend::Codex, parent(&[("TERM", "xterm-256color")]));
        let terms: Vec<_> = env.iter().filter(|(k, _)| k == "TERM").collect();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].1, "dumb");
    }

    #[test]
    fn prefix_pattern_needs_the_prefix() {
        assert!(name_matches("LC_*", "LC_CTYPE"));
        assert!(!name_matches("LC_*", "LC"));
        assert!(!name_matches("PATH", "PATH_EXTRA"));
    }
}
