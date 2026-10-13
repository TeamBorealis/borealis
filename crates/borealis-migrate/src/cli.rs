//! Command line front end: argument parsing and the run loop (kept in the library so it
//! can be tested without spawning a process).

use crate::pipeline::{migrate_path, write_output, MigrateOptions};
use borealis_content::{PackOrigin, Ruleset, Severity};
use std::io::Write;
use std::path::PathBuf;

/// Usage text.
pub const USAGE: &str = "\
borealis-migrate <input dir|file|.index> -o <out dir> [options]

Converts Aurora XML element files and .index manifests to Borealis JSON content packs.
No network access is made; index URLs are only stored as metadata.

Options:
  -o, --out <dir>          output directory (required)
      --report <file>      write the migration report (JSON) to this file
      --strict             fail (exit 1, write no content) on any warning or error
      --origin <core|custom>   pack load semantics (default custom)
      --ruleset <dnd2014|dnd2024>  ruleset stamped on converted files (default dnd2014)
      --pack-id <id>       pack id (default: derived from the input name)
      --pack-name <name>   pack display name
      --pack-version <v>   pack version (default 1.0.0)
      --license <text>     license recorded in the manifest
      --attribution <text> attribution recorded in the manifest
      --no-validate        skip cross-reference and schema validation
  -h, --help               show this help
  -V, --version            show the version

Exit codes: 0 success, 1 conversion problems, 2 usage or I/O failure.";

/// Parsed command line.
#[derive(Debug, Clone, PartialEq)]
pub struct Cli {
    /// Input path.
    pub input: PathBuf,
    /// Output directory.
    pub out: PathBuf,
    /// Report path.
    pub report: Option<PathBuf>,
    /// Fail on warnings.
    pub strict: bool,
    /// Migration options.
    pub options: MigrateOptions,
}

/// What the arguments ask for.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Run a migration.
    Run(Box<Cli>),
    /// Print help.
    Help,
    /// Print the version.
    Version,
}

impl PartialEq for MigrateOptions {
    fn eq(&self, other: &Self) -> bool {
        self.pack_id == other.pack_id
            && self.pack_name == other.pack_name
            && self.pack_version == other.pack_version
            && self.origin == other.origin
            && self.ruleset == other.ruleset
            && self.license == other.license
            && self.attribution == other.attribution
            && self.validate == other.validate
    }
}

fn value(args: &mut std::slice::Iter<'_, String>, flag: &str) -> Result<String, String> {
    args.next()
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

/// Parse command line arguments (without the program name).
///
/// # Errors
/// A message describing the usage problem.
pub fn parse_args(args: &[String]) -> Result<Command, String> {
    let mut input: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut cli_report = None;
    let mut strict = false;
    let mut options = MigrateOptions::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "-o" | "--out" => out = Some(PathBuf::from(value(&mut iter, arg)?)),
            "--report" => cli_report = Some(PathBuf::from(value(&mut iter, arg)?)),
            "--strict" => strict = true,
            "--no-validate" => options.validate = false,
            "--origin" => {
                options.origin = match value(&mut iter, arg)?.as_str() {
                    "core" => PackOrigin::Core,
                    "custom" => PackOrigin::Custom,
                    other => return Err(format!("--origin must be core or custom, got {other:?}")),
                };
            }
            "--ruleset" => {
                options.ruleset = match value(&mut iter, arg)?.as_str() {
                    "dnd2014" => Ruleset::Dnd2014,
                    "dnd2024" => Ruleset::Dnd2024,
                    other => {
                        return Err(format!(
                            "--ruleset must be dnd2014 or dnd2024, got {other:?}"
                        ))
                    }
                };
            }
            "--pack-id" => options.pack_id = Some(value(&mut iter, arg)?),
            "--pack-name" => options.pack_name = Some(value(&mut iter, arg)?),
            "--pack-version" => options.pack_version = value(&mut iter, arg)?,
            "--license" => options.license = Some(value(&mut iter, arg)?),
            "--attribution" => options.attribution = Some(value(&mut iter, arg)?),
            flag if flag.starts_with('-') && flag.len() > 1 => {
                return Err(format!("unknown option {flag}"));
            }
            positional => {
                if input.replace(PathBuf::from(positional)).is_some() {
                    return Err("only one input path may be given".into());
                }
            }
        }
    }
    let input = input.ok_or("missing input path")?;
    let out = out.ok_or("missing -o <out dir>")?;
    Ok(Command::Run(Box::new(Cli {
        input,
        out,
        report: cli_report,
        strict,
        options,
    })))
}

/// Run the tool; returns the process exit code. Output goes to the given writers.
pub fn run(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let command = match parse_args(args) {
        Ok(c) => c,
        Err(msg) => {
            let _ = writeln!(stderr, "error: {msg}\n\n{USAGE}");
            return 2;
        }
    };
    let cli = match command {
        Command::Help => {
            let _ = writeln!(stdout, "{USAGE}");
            return 0;
        }
        Command::Version => {
            let _ = writeln!(stdout, "borealis-migrate {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        Command::Run(cli) => cli,
    };
    execute(&cli, stdout, stderr).unwrap_or_else(|msg| {
        let _ = writeln!(stderr, "error: {msg}");
        2
    })
}

fn execute(cli: &Cli, stdout: &mut dyn Write, stderr: &mut dyn Write) -> Result<i32, String> {
    let output = migrate_path(&cli.input, &cli.options).map_err(|e| e.to_string())?;
    let report = &output.report;
    for issue in report
        .issues
        .iter()
        .filter(|i| i.severity != Severity::Info)
    {
        let _ = writeln!(stderr, "{issue}");
    }
    let passes = report.passes(cli.strict);
    if let Some(path) = &cli.report {
        let text = report.to_json_pretty().map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if cli.strict && !passes {
        let _ = writeln!(
            stderr,
            "strict mode: not writing output because of warnings or errors"
        );
    } else {
        write_output(&cli.out, &output).map_err(|e| e.to_string())?;
    }
    let s = &report.summary;
    let _ = writeln!(
        stdout,
        "{} files ({} content, {} index, {} skipped), {} elements, {} rules; {} errors, {} warnings",
        s.files_read,
        s.content_files,
        s.index_files,
        s.skipped_files,
        report.totals.elements,
        report.totals.rules,
        s.errors,
        s.warnings
    );
    Ok(i32::from(!passes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn parses_all_options() {
        let cmd = parse_args(&args(&[
            "in",
            "-o",
            "out",
            "--report",
            "r.json",
            "--strict",
            "--origin",
            "core",
            "--ruleset",
            "dnd2024",
            "--pack-id",
            "p",
            "--pack-name",
            "P",
            "--pack-version",
            "2",
            "--license",
            "CC-BY-4.0",
            "--attribution",
            "a",
            "--no-validate",
        ]))
        .unwrap();
        let Command::Run(cli) = cmd else { panic!() };
        assert!(cli.strict && !cli.options.validate);
        assert_eq!(cli.options.origin, PackOrigin::Core);
        assert_eq!(cli.options.ruleset, Ruleset::Dnd2024);
        assert_eq!(cli.options.pack_id.as_deref(), Some("p"));
        assert_eq!(cli.report.as_deref(), Some(std::path::Path::new("r.json")));
    }

    #[test]
    fn usage_errors() {
        for bad in [
            vec!["in"],
            vec!["-o", "out"],
            vec!["a", "b", "-o", "out"],
            vec!["in", "-o"],
            vec!["in", "-o", "out", "--bogus"],
            vec!["in", "-o", "out", "--origin", "x"],
            vec!["in", "-o", "out", "--ruleset", "x"],
        ] {
            assert!(parse_args(&args(&bad)).is_err(), "{bad:?}");
        }
        assert_eq!(parse_args(&args(&["--help"])).unwrap(), Command::Help);
        assert_eq!(parse_args(&args(&["-V"])).unwrap(), Command::Version);
    }

    #[test]
    fn help_version_and_usage_exit_codes() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(run(&args(&["--help"]), &mut out, &mut err), 0);
        assert!(String::from_utf8(out.clone())
            .unwrap()
            .contains("Exit codes"));
        out.clear();
        assert_eq!(run(&args(&["--version"]), &mut out, &mut err), 0);
        assert_eq!(run(&args(&["--nope"]), &mut out, &mut err), 2);
        assert!(String::from_utf8(err).unwrap().contains("unknown option"));
    }
}
