//! Pure argv/stdin construction for each backend. Nothing here spawns a process, so the exact
//! flags are unit-testable. Prompts always travel over stdin, never argv and never a shell.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::Backend;

/// What the agent may touch. Default is nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolPolicy {
    /// No tools at all (Claude: `--tools ""`). Codex cannot disable its shell, so it is kept in
    /// a read-only sandbox inside an empty temp dir instead.
    #[default]
    None,
    /// Read-only file inspection in the working directory (Claude: Read/Glob/Grep).
    ReadOnly,
}

/// Inputs to argv construction.
#[derive(Debug, Clone)]
pub struct CommandInputs<'a> {
    /// Which CLI.
    pub backend: Backend,
    /// Absolute path of the executable.
    pub program: &'a Path,
    /// Working directory the agent sees.
    pub workdir: &'a Path,
    /// Prompt text (goes to stdin).
    pub prompt: &'a str,
    /// Optional system instructions.
    pub system: Option<&'a str>,
    /// Claude only: a private file holding `system` (created by the caller). When set, Claude gets
    /// `--system-prompt-file` instead of the text in argv, which other local users could read
    /// with `ps`.
    pub system_file: Option<&'a Path>,
    /// Optional model alias/name.
    pub model: Option<&'a str>,
    /// Optional reasoning effort (`low`..`max` for Claude; `minimal`..`high` for Codex).
    pub effort: Option<&'a str>,
    /// Tool access.
    pub tools: ToolPolicy,
    /// JSON Schema for the final answer, if any.
    pub schema: Option<&'a Value>,
    /// Pass the schema natively (`--json-schema` / `--output-schema`) in addition to the prompt.
    pub native_schema: bool,
    /// Where to put the Codex `--output-schema` file (created by the caller).
    pub schema_file: Option<&'a Path>,
    /// Let Codex read the user's `config.toml` (default off: avoids user MCP servers/profiles).
    pub use_user_config: bool,
    /// Extra args from [`crate::profile::profile_args`]; non-empty means a Borealis profile is
    /// active (Claude then keeps the `Skill` tool and does not disable skills).
    pub profile_args: &'a [OsString],
}

/// A fully built invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPlan {
    /// Executable.
    pub program: PathBuf,
    /// Arguments (no shell involved).
    pub args: Vec<OsString>,
    /// Bytes to write to stdin.
    pub stdin: Vec<u8>,
}

/// Build the invocation for `inputs`.
pub fn build(inputs: &CommandInputs<'_>) -> CommandPlan {
    match inputs.backend {
        Backend::Claude => build_claude(inputs),
        Backend::Codex => build_codex(inputs),
    }
}

fn os(s: &str) -> OsString {
    OsString::from(s)
}

fn build_claude(i: &CommandInputs<'_>) -> CommandPlan {
    let mut a: Vec<OsString> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
    ]
    .into_iter()
    .map(os)
    .collect();
    // Isolation: no session files, no user/project settings or hooks, no slash commands, and no
    // MCP servers (claude.ai connectors are loaded unless --strict-mcp-config is given).
    let profile = !i.profile_args.is_empty();
    a.push(os("--no-session-persistence"));
    if !profile {
        // The profile ships skills, which `--disable-slash-commands` would also switch off.
        a.push(os("--disable-slash-commands"));
    }
    a.push(os("--strict-mcp-config"));
    a.extend(["--setting-sources", ""].map(os));
    let tools = match (i.tools, profile) {
        (ToolPolicy::None, false) => "",
        (ToolPolicy::None, true) => "Skill",
        (ToolPolicy::ReadOnly, false) => "Read,Glob,Grep",
        (ToolPolicy::ReadOnly, true) => "Read,Glob,Grep,Skill",
    };
    a.extend([os("--tools"), os(tools)]);
    a.extend(i.profile_args.iter().cloned());
    if let Some(m) = i.model {
        a.extend([os("--model"), os(m)]);
    }
    if let Some(e) = i.effort {
        a.extend([os("--effort"), os(e)]);
    }
    match (i.system_file, i.system) {
        (Some(path), Some(_)) => {
            a.extend([os("--system-prompt-file"), path.as_os_str().to_os_string()]);
        }
        (_, Some(s)) => a.extend([os("--system-prompt"), os(s)]),
        _ => {}
    }
    if let (Some(schema), true) = (i.schema, i.native_schema) {
        a.extend([os("--json-schema"), os(&schema.to_string())]);
    }
    CommandPlan {
        program: i.program.to_path_buf(),
        args: a,
        stdin: prompt_with_schema(i, false, !i.native_schema).into_bytes(),
    }
}

fn build_codex(i: &CommandInputs<'_>) -> CommandPlan {
    let mut a: Vec<OsString> = [
        "exec",
        "--json",
        "--sandbox",
        "read-only",
        "--skip-git-repo-check",
        "--ephemeral",
        "--ignore-rules",
        "--color",
        "never",
    ]
    .into_iter()
    .map(os)
    .collect();
    if !i.use_user_config {
        a.push(os("--ignore-user-config"));
    }
    a.extend([os("-C"), i.workdir.as_os_str().to_os_string()]);
    if let Some(m) = i.model {
        a.extend([os("-m"), os(m)]);
    }
    if let Some(e) = i.effort {
        a.extend([os("-c"), os(&format!("model_reasoning_effort=\"{e}\""))]);
    }
    if let (Some(path), true, Some(_)) = (i.schema_file, i.native_schema, i.schema) {
        a.extend([os("--output-schema"), path.as_os_str().to_os_string()]);
    }
    a.extend(i.profile_args.iter().cloned());
    // `-` = read the prompt from stdin.
    a.push(os("-"));
    CommandPlan {
        program: i.program.to_path_buf(),
        args: a,
        stdin: prompt_with_schema(i, true, true).into_bytes(),
    }
}

/// Compose the stdin text. Codex has no system-prompt flag, so system text is prepended.
/// The schema is also spelled out in the prompt so non-native mode still gets JSON.
fn prompt_with_schema(
    i: &CommandInputs<'_>,
    include_system: bool,
    spell_out_schema: bool,
) -> String {
    let mut out = String::new();
    if include_system {
        if let Some(sys) = i.system {
            out.push_str("# Instructions\n");
            out.push_str(sys);
            out.push_str("\n\n# Task\n");
        }
    }
    out.push_str(i.prompt);
    if let (Some(schema), true) = (i.schema, spell_out_schema) {
        out.push_str("\n\nReturn ONLY a single JSON value (no prose, no markdown fences) matching this JSON Schema:\n");
        out.push_str(&schema.to_string());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base<'a>(backend: Backend, schema: Option<&'a Value>) -> CommandInputs<'a> {
        CommandInputs {
            backend,
            program: Path::new("/bin/cli"),
            workdir: Path::new("/tmp/w"),
            prompt: "hello",
            system: Some("be brief"),
            system_file: None,
            model: Some("haiku"),
            effort: Some("low"),
            tools: ToolPolicy::None,
            schema,
            native_schema: true,
            schema_file: Some(Path::new("/tmp/w/schema.json")),
            use_user_config: false,
            profile_args: &[],
        }
    }

    fn strs(p: &CommandPlan) -> Vec<String> {
        p.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn claude_flags_isolate_and_disable_tools() {
        let schema = json!({"type":"object"});
        let plan = build(&base(Backend::Claude, Some(&schema)));
        let a = strs(&plan);
        for f in [
            "-p",
            "--verbose",
            "--include-partial-messages",
            "--no-session-persistence",
            "--strict-mcp-config",
            "--disable-slash-commands",
        ] {
            assert!(a.contains(&f.to_string()), "missing {f}: {a:?}");
        }
        let pos = a.iter().position(|x| x == "--tools").unwrap();
        assert_eq!(a[pos + 1], "");
        let pos = a.iter().position(|x| x == "--json-schema").unwrap();
        assert_eq!(a[pos + 1], r#"{"type":"object"}"#);
        assert!(a
            .windows(2)
            .any(|w| w[0] == "--system-prompt" && w[1] == "be brief"));
        assert!(
            !a.contains(&"hello".to_string()),
            "prompt must not be in argv"
        );
        assert!(String::from_utf8(plan.stdin).unwrap().starts_with("hello"));
    }

    #[test]
    fn claude_system_prompt_file_keeps_text_out_of_argv() {
        let mut i = base(Backend::Claude, None);
        i.system_file = Some(Path::new("/tmp/s/system.txt"));
        let a = strs(&build(&i));
        assert!(a
            .windows(2)
            .any(|w| w[0] == "--system-prompt-file" && w[1] == "/tmp/s/system.txt"));
        assert!(!a.iter().any(|x| x == "be brief" || x == "--system-prompt"));
    }

    #[test]
    fn claude_read_only_tools() {
        let mut i = base(Backend::Claude, None);
        i.tools = ToolPolicy::ReadOnly;
        let a = strs(&build(&i));
        assert!(a
            .windows(2)
            .any(|w| w[0] == "--tools" && w[1] == "Read,Glob,Grep"));
        assert!(!a.contains(&"--json-schema".to_string()));
    }

    #[test]
    fn codex_is_sandboxed_and_reads_stdin() {
        let schema = json!({"type":"object"});
        let plan = build(&base(Backend::Codex, Some(&schema)));
        let a = strs(&plan);
        assert_eq!(a[0], "exec");
        assert!(a
            .windows(2)
            .any(|w| w[0] == "--sandbox" && w[1] == "read-only"));
        assert!(a.contains(&"--ephemeral".to_string()));
        assert!(a.contains(&"--ignore-user-config".to_string()));
        assert!(a
            .windows(2)
            .any(|w| w[0] == "--output-schema" && w[1] == "/tmp/w/schema.json"));
        assert!(a
            .windows(2)
            .any(|w| w[0] == "-c" && w[1] == "model_reasoning_effort=\"low\""));
        assert_eq!(a.last().unwrap(), "-");
        let stdin = String::from_utf8(plan.stdin).unwrap();
        assert!(stdin.starts_with("# Instructions\nbe brief"), "{stdin}");
        assert!(stdin.contains("JSON Schema"));
    }

    #[test]
    fn codex_without_native_schema_omits_flag_but_keeps_prompt_schema() {
        let schema = json!({"type":"object"});
        let mut i = base(Backend::Codex, Some(&schema));
        i.native_schema = false;
        i.use_user_config = true;
        let plan = build(&i);
        let a = strs(&plan);
        assert!(!a.contains(&"--output-schema".to_string()));
        assert!(!a.contains(&"--ignore-user-config".to_string()));
        assert!(String::from_utf8(plan.stdin)
            .unwrap()
            .contains("JSON Schema"));
    }
}
