//! Command line front end of the importer (`borealis-import-5etools`).

use crate::import::import_folder_with_progress;
use crate::options::{ImportOptions, Phase};
use crate::scan::scan_folder;
use borealis_content::Ruleset;
use std::io::Write;
use std::path::PathBuf;

/// Exit code: success.
pub const EXIT_OK: i32 = 0;
/// Exit code: the import failed (bad folder, unwritable output).
pub const EXIT_FAILED: i32 = 1;
/// Exit code: usage error.
pub const EXIT_USAGE: i32 = 2;

/// Usage text.
pub const USAGE: &str = "\
Usage: borealis-import-5etools <data-dir> -o <out-dir> [options]
       borealis-import-5etools <data-dir> --scan

Converts the user's own local 5etools data folder into Borealis packs (one per source).
The output is for local use only and must not be redistributed.

Options:
  -o, --out <dir>       Directory that receives one pack directory per source
  --source <A,B,..>     Only these source abbreviations (case-insensitive)
  --ruleset <2014|2024> Only entries of this ruleset
  --fluff               Append lore text from the fluff files
  --only-srd            Only entries flagged srd / srd52
  --srd-dir <dir>       Borealis SRD pack directory used for maps_to links (repeatable)
  --report <file>       Write the JSON import report to this file
  --scan                List the sources of the folder and exit
  -h, --help            Show this text

Exit codes: 0 success, 1 import failed, 2 usage error.
";

#[derive(Debug, Default)]
struct Args {
    data_dir: Option<PathBuf>,
    out: Option<PathBuf>,
    report: Option<PathBuf>,
    scan: bool,
    help: bool,
    options: ImportOptions,
}

fn value(it: &mut std::vec::IntoIter<String>, flag: &str) -> Result<String, String> {
    it.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut sources: Vec<String> = Vec::new();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => parsed.help = true,
            "--scan" => parsed.scan = true,
            "--fluff" => parsed.options.include_fluff = true,
            "--only-srd" => parsed.options.only_srd = true,
            "-o" | "--out" => parsed.out = Some(value(&mut it, &arg)?.into()),
            "--report" => parsed.report = Some(value(&mut it, &arg)?.into()),
            "--srd-dir" => parsed.options.srd_dirs.push(value(&mut it, &arg)?.into()),
            "--source" => sources.extend(
                value(&mut it, &arg)?
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_string),
            ),
            "--ruleset" => {
                parsed.options.ruleset = Some(match value(&mut it, &arg)?.as_str() {
                    "2014" | "dnd2014" => Ruleset::Dnd2014,
                    "2024" | "dnd2024" => Ruleset::Dnd2024,
                    other => return Err(format!("unknown ruleset {other:?} (use 2014 or 2024)")),
                });
            }
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            _ if parsed.data_dir.is_none() => parsed.data_dir = Some(arg.into()),
            _ => return Err(format!("unexpected argument {arg:?}")),
        }
    }
    if !sources.is_empty() {
        parsed.options = parsed.options.with_sources(sources);
    }
    Ok(parsed)
}

fn run_scan(dir: &std::path::Path, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match scan_folder(dir) {
        Ok(sources) => {
            for s in sources {
                let counts: Vec<String> =
                    s.counts.iter().map(|(k, v)| format!("{k}={v}")).collect();
                let _ = writeln!(
                    out,
                    "{}\t{}\t{}\t{}",
                    s.abbr,
                    s.ruleset,
                    s.name,
                    counts.join(" ")
                );
            }
            EXIT_OK
        }
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            EXIT_FAILED
        }
    }
}

fn run_import(
    args: &Args,
    dir: &std::path::Path,
    out_dir: &std::path::Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let mut last_phase = None;
    let mut on_progress = |ev: &crate::options::ProgressEvent| {
        if last_phase != Some(ev.phase) && ev.phase != Phase::Done {
            last_phase = Some(ev.phase);
            let _ = writeln!(err, "{:?}...", ev.phase);
        }
    };
    let result = match import_folder_with_progress(dir, &args.options, &mut on_progress) {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            return EXIT_FAILED;
        }
    };
    for pack in &result.packs {
        if let Err(e) = pack.write_to(out_dir) {
            let _ = writeln!(err, "error: {e}");
            return EXIT_FAILED;
        }
        let _ = writeln!(
            out,
            "{}\t{} elements",
            pack.manifest.id,
            pack.element_count()
        );
    }
    if let Some(path) = &args.report {
        let written = result
            .report
            .to_json_pretty()
            .map_err(|e| e.to_string())
            .and_then(|text| std::fs::write(path, text).map_err(|e| e.to_string()));
        if let Err(e) = written {
            let _ = writeln!(err, "error: cannot write report {}: {e}", path.display());
            return EXIT_FAILED;
        }
    }
    let totals = result.report.totals();
    let _ = writeln!(
        out,
        "converted {} of {} entries ({} partial, {} skipped), {} packs",
        totals.converted,
        totals.total,
        totals.partial,
        totals.skipped,
        result.packs.len()
    );
    EXIT_OK
}

/// Run the CLI. `args` excludes the program name. Returns the process exit code.
pub fn run(args: Vec<String>, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let args = match parse_args(args) {
        Ok(a) => a,
        Err(msg) => {
            let _ = writeln!(err, "error: {msg}\n\n{USAGE}");
            return EXIT_USAGE;
        }
    };
    if args.help {
        let _ = write!(out, "{USAGE}");
        return EXIT_OK;
    }
    let Some(dir) = args.data_dir.clone() else {
        let _ = writeln!(err, "error: missing <data-dir>\n\n{USAGE}");
        return EXIT_USAGE;
    };
    if args.scan {
        return run_scan(&dir, out, err);
    }
    let Some(out_dir) = args.out.clone() else {
        let _ = writeln!(err, "error: missing -o <out-dir>\n\n{USAGE}");
        return EXIT_USAGE;
    };
    run_import(&args, &dir, &out_dir, out, err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn parses_all_options() {
        let a = parse_args(strings(&[
            "data",
            "-o",
            "out",
            "--source",
            "phb,XGE",
            "--ruleset",
            "2024",
            "--fluff",
            "--only-srd",
            "--srd-dir",
            "a",
            "--srd-dir",
            "b",
            "--report",
            "r.json",
        ]))
        .unwrap();
        assert_eq!(a.data_dir.unwrap().to_str(), Some("data"));
        assert_eq!(a.options.srd_dirs.len(), 2);
        assert!(a.options.include_fluff && a.options.only_srd);
        assert_eq!(a.options.ruleset, Some(Ruleset::Dnd2024));
        assert!(a.options.allows_source("xge") && !a.options.allows_source("TCE"));
    }

    #[test]
    fn usage_errors_exit_2() {
        for bad in [
            vec!["--bogus"],
            vec!["d", "-o"],
            vec!["d", "--ruleset", "5e"],
            vec!["d", "extra"],
            vec![],
            vec!["d"],
        ] {
            let (mut o, mut e) = (Vec::new(), Vec::new());
            assert_eq!(run(strings(&bad), &mut o, &mut e), EXIT_USAGE, "{bad:?}");
            assert!(String::from_utf8(e).unwrap().contains("Usage:"));
        }
        let (mut o, mut e) = (Vec::new(), Vec::new());
        assert_eq!(run(strings(&["--help"]), &mut o, &mut e), EXIT_OK);
        assert!(String::from_utf8(o).unwrap().starts_with("Usage:"));
    }

    #[test]
    fn missing_folder_exits_1() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");
        let (mut o, mut e) = (Vec::new(), Vec::new());
        let code = run(
            strings(&[
                missing.to_str().unwrap(),
                "-o",
                dir.path().to_str().unwrap(),
            ]),
            &mut o,
            &mut e,
        );
        assert_eq!(code, EXIT_FAILED);
        assert!(String::from_utf8(e).unwrap().contains("not a directory"));
    }
}
