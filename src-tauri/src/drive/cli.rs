//! `borealis <tool> --library <dir> [--<arg> <value>]... [--json|--human]`: the drive tools as
//! one-shot commands with the MCP tool names and arguments (`-` and `_` are interchangeable).
//! JSON on stdout by default; `--human` prints a short text form. Exit codes: 0 done, 1 the tool
//! refused or failed (the engine's reason is printed), 2 usage error.

use super::{specs, Drive, DriveError};
use serde_json::{json, Map, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const EXIT_OK: i32 = 0;
pub const EXIT_FAILED: i32 = 1;
pub const EXIT_USAGE: i32 = 2;

fn tool_name(arg: &str) -> String {
    arg.replace('-', "_")
}

/// True when the first argument is a drive tool or `help` (checked before the GUI starts).
#[must_use]
pub fn requested(args: &[String]) -> bool {
    args.first()
        .is_some_and(|a| a == "help" || specs::find(&tool_name(a)).is_some())
}

/// Parsed command line.
struct Command {
    tool: String,
    library: Option<PathBuf>,
    content_dir: Option<PathBuf>,
    human: bool,
    args: Map<String, Value>,
}

fn usage(message: impl Into<String>) -> DriveError {
    DriveError::new("usage", message)
}

/// One `--key value` argument typed by the tool's input schema.
fn typed(schema: &Value, key: &str, raw: &str) -> Result<Value, DriveError> {
    let ty = schema["type"].as_str().unwrap_or("string");
    let bad = |what: &str| usage(format!("--{key} expects {what}, got {raw}"));
    Ok(match ty {
        "integer" => json!(raw.parse::<i64>().map_err(|_| bad("an integer"))?),
        "boolean" => match raw {
            "true" | "yes" | "1" => json!(true),
            "false" | "no" | "0" => json!(false),
            _ => return Err(bad("true or false")),
        },
        "array" => json!(raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()),
        "object" if raw.trim_start().starts_with('{') => {
            serde_json::from_str(raw).map_err(|_| bad("a JSON object"))?
        }
        "object" => {
            let mut map = Map::new();
            for pair in raw.split(',').filter(|p| !p.trim().is_empty()) {
                let (k, v) = pair.split_once('=').ok_or_else(|| bad("key=value pairs"))?;
                let n: i64 = v.trim().parse().map_err(|_| bad("integer values"))?;
                map.insert(k.trim().to_string(), json!(n));
            }
            Value::Object(map)
        }
        _ => json!(raw),
    })
}

fn parse(args: &[String]) -> Result<Command, DriveError> {
    let tool = tool_name(args.first().map(String::as_str).unwrap_or_default());
    let spec = specs::find(&tool).ok_or_else(|| {
        DriveError::new(
            "unknown_tool",
            format!("unknown command: {tool} (see `borealis help`)"),
        )
    })?;
    let props = spec["inputSchema"]["properties"].clone();
    let mut cmd = Command {
        tool,
        library: None,
        content_dir: None,
        human: false,
        args: Map::new(),
    };
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        let Some(flag) = arg.strip_prefix("--") else {
            return Err(usage(format!("unexpected argument: {arg}")));
        };
        let (flag, inline) = match flag.split_once('=') {
            Some((f, v)) => (f, Some(v.to_string())),
            None => (flag, None),
        };
        let key = tool_name(flag);
        let value = |i: &mut usize| -> Result<String, DriveError> {
            if let Some(v) = inline.clone() {
                return Ok(v);
            }
            let v = args
                .get(*i)
                .ok_or_else(|| usage(format!("--{flag} needs a value")))?;
            *i += 1;
            Ok(v.clone())
        };
        match key.as_str() {
            "json" => cmd.human = false,
            "human" => cmd.human = true,
            "library" => cmd.library = Some(PathBuf::from(value(&mut i)?)),
            "content_dir" => cmd.content_dir = Some(PathBuf::from(value(&mut i)?)),
            "args" => {
                let parsed: Value = serde_json::from_str(&value(&mut i)?)
                    .map_err(|e| usage(format!("--args is not JSON: {e}")))?;
                let Value::Object(map) = parsed else {
                    return Err(usage("--args must be a JSON object"));
                };
                cmd.args.extend(map);
            }
            _ => {
                let schema = props.get(&key).ok_or_else(|| {
                    usage(format!(
                        "{} has no argument --{flag} (see `borealis help {}`)",
                        cmd.tool, cmd.tool
                    ))
                })?;
                let is_flag = schema["type"] == "boolean"
                    && inline.is_none()
                    && args.get(i).is_none_or(|n| n.starts_with("--"));
                let typed_value = if is_flag {
                    json!(true)
                } else {
                    typed(schema, flag, &value(&mut i)?)?
                };
                match (cmd.args.get_mut(&key), typed_value) {
                    (Some(Value::Array(list)), Value::Array(more)) => list.extend(more),
                    (_, v) => {
                        cmd.args.insert(key, v);
                    }
                }
            }
        }
    }
    Ok(cmd)
}

/// `borealis help` / `borealis help <tool>`.
fn help(topic: Option<&str>) -> String {
    if let Some(name) = topic {
        let Some(spec) = specs::find(&tool_name(name)) else {
            return format!("unknown command: {name}\n");
        };
        let mut out = format!(
            "borealis {} --library <dir> [args]\n  {}\n",
            spec["name"].as_str().unwrap_or_default(),
            spec["description"].as_str().unwrap_or_default()
        );
        let required: Vec<&str> = spec["inputSchema"]["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if let Some(props) = spec["inputSchema"]["properties"].as_object() {
            for (k, p) in props {
                let need = if required.contains(&k.as_str()) {
                    " (required)"
                } else {
                    ""
                };
                let ty = p["type"].as_str().unwrap_or("string");
                let desc = p["description"].as_str().unwrap_or_default();
                let choices = p["enum"].as_array().map(|e| {
                    let v: Vec<&str> = e.iter().filter_map(Value::as_str).collect();
                    format!(" [{}]", v.join("|"))
                });
                out.push_str(&format!(
                    "  --{} <{ty}>{need}{} {desc}\n",
                    k.replace('_', "-"),
                    choices.unwrap_or_default()
                ));
            }
        }
        return out;
    }
    let mut out = String::from(
        "Borealis command line: drive characters without the window.\n\n\
         usage: borealis <command> --library <dir> [--<arg> <value>]... [--json | --human]\n\
         \x20      borealis help [command]\n\n\
         --library <dir>      the library folder (characters, imported content); created if missing\n\
         --content-dir <dir>  bundled content folder (default: next to the app)\n\
         --args '<json>'      all arguments as one JSON object (as the MCP tool takes them)\n\
         --json               JSON output (default); --human: short text\n\n\
         Exit codes: 0 done, 1 refused or failed (reason printed), 2 usage error.\n\ncommands:\n",
    );
    for spec in specs::all() {
        let desc = spec["description"].as_str().unwrap_or_default();
        let first = desc.split(". ").next().unwrap_or(desc);
        out.push_str(&format!(
            "  {:<20} {}\n",
            spec["name"].as_str().unwrap_or_default(),
            first.trim_end_matches('.')
        ));
    }
    out
}

/// A short text form of a result.
fn human(v: &Value) -> String {
    for key in [
        "characters",
        "slots",
        "options",
        "items",
        "packs",
        "sources",
    ] {
        if let Some(rows) = v[key].as_array() {
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id = r["id"]
                        .as_str()
                        .or(r["slotId"].as_str())
                        .or(r["uid"].as_str())
                        .or(r["abbr"].as_str())
                        .unwrap_or("");
                    let name = r["name"].as_str().or(r["title"].as_str()).unwrap_or("");
                    let extra = r["chosen"]["name"]
                        .as_str()
                        .or(r["disabledReason"].as_str())
                        .map(|x| format!(" ({x})"))
                        .unwrap_or_default();
                    format!("{id}  {name}{extra}")
                })
                .collect();
            return format!("{} {key}\n{}\n", rows.len(), lines.join("\n"));
        }
    }
    if let (Some(name), Some(level)) = (v["name"].as_str(), v["level"].as_u64()) {
        let classes: Vec<String> = v["classes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| Some(format!("{} {}", c["name"].as_str()?, c["level"])))
            .collect();
        return format!(
            "{name} [{}] level {level} {}; AC {} HP {}; {} open slot(s)\n",
            v["id"].as_str().unwrap_or(""),
            classes.join(" / "),
            v["armorClass"],
            v["hpMax"],
            v["openSlots"]
        );
    }
    if let Some(path) = v["path"].as_str() {
        return format!("wrote {path} ({} bytes)\n", v["bytes"]);
    }
    format!("{}\n", serde_json::to_string_pretty(v).unwrap_or_default())
}

fn default_content_root(exe: &Path) -> Option<PathBuf> {
    crate::mcp_mode::default_content_dirs(exe)
        .into_iter()
        .next()
}

/// Run a parsed command.
fn execute(cmd: &Command, exe: &Path) -> Result<Value, DriveError> {
    let library = cmd
        .library
        .as_ref()
        .ok_or_else(|| usage("--library <dir> is required"))?;
    let content = cmd
        .content_dir
        .clone()
        .or_else(|| default_content_root(exe))
        .ok_or_else(|| usage("no content directory found; pass --content-dir <dir>"))?;
    let drive = Drive::open(library, content)?;
    drive.call(&cmd.tool, &Value::Object(cmd.args.clone()))
}

/// Run a command line (without argv[0]) writing to the given streams; returns the exit code.
pub fn run_with(args: &[String], exe: &Path, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.first().map(String::as_str) == Some("help") {
        let _ = write!(out, "{}", help(args.get(1).map(String::as_str)));
        return EXIT_OK;
    }
    let human_mode = args.iter().any(|a| a == "--human");
    let result = parse(args).and_then(|cmd| execute(&cmd, exe));
    match result {
        Ok(value) => {
            let text = if human_mode {
                human(&value)
            } else {
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(&value).unwrap_or_default()
                )
            };
            let _ = write!(out, "{text}");
            EXIT_OK
        }
        Err(e) => {
            if !human_mode {
                let body = json!({"error": {"code": e.code, "message": e.message}});
                let _ = writeln!(out, "{body}");
            }
            let _ = writeln!(err, "borealis: {}", e.message);
            if e.is_usage() {
                EXIT_USAGE
            } else {
                EXIT_FAILED
            }
        }
    }
}

/// Run on the process streams.
#[must_use]
pub fn run(args: &[String]) -> i32 {
    let exe = std::env::current_exe().unwrap_or_default();
    run_with(
        args,
        &exe,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    )
}
