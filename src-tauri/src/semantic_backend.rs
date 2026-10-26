//! B-49: where the installed app keeps llama.cpp's CPU backend modules, and `borealis --semantic-backend-check`.
//!
//! x86_64 Windows and Linux builds load the CPU backend for the running processor from module files
//! (`borealis_embed::cpu_backend`). `build/stage-ggml-libs.sh` puts them into the installers:
//! * Windows (NSIS): `<install>\ggml\ggml-cpu-*.dll`, and `llama.dll`, `ggml.dll`, `ggml-base.dll` next to
//!   `borealis.exe` (the loader finds a DLL the executable imports in the executable's folder).
//! * Linux (deb, rpm, AppImage): `/usr/lib/Borealis/ggml/` (the bundle's resource folder) holds all of them; the
//!   executable's run path names that folder (`build.rs`).
//!
//! [`install`] names the folder before anything loads a model. A debug build without staged modules (`tauri dev`,
//! `cargo run`) falls back to the build's own module folder; a release build never does, because that path is on
//! the build machine: it reports the modules as missing instead and search stays keyword-only.
//!
//! `borealis --semantic-backend-check [--model <gguf>]` starts llama.cpp the same way and prints one JSON line (the
//! folder, its module files, the backend devices and, with a model, the app's query-speed check on this machine).
//! It exits 0 when a CPU backend loaded. CI runs it on the installed bundles and on emulated old CPUs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use borealis_embed::cpu_backend::{self, MODULES};
use borealis_embed::llama::{LlamaEmbedder, LlamaOptions};
use borealis_embed::{DeviceProfile, Embedder, GuardedEmbedder, SemanticGuard};

/// The command-line flag.
pub const FLAG: &str = "--semantic-backend-check";

/// The modules' folder inside the bundle's resource folder (Windows: the install folder).
pub const MODULE_SUBDIR: &str = "ggml";

/// `borealis --semantic-backend-check ...` was asked for.
#[must_use]
pub fn requested(args: &[String]) -> bool {
    args.first().is_some_and(|a| a == FLAG)
}

/// Where a bundle installed next to `exe_dir` keeps the modules, most likely first. Linux: the deb / rpm / AppImage
/// resource folder `../lib/<productName>` (also the lower-case name, as `mcp_mode::default_content_dirs`), then
/// `ggml` next to the binary (a release build in the cargo target folder, where tauri-build copies resources).
fn candidates(exe_dir: &Path) -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![exe_dir.join(MODULE_SUBDIR)]
    } else {
        vec![
            exe_dir.join("../lib/Borealis").join(MODULE_SUBDIR),
            exe_dir.join("../lib/borealis").join(MODULE_SUBDIR),
            exe_dir.join(MODULE_SUBDIR),
        ]
    }
}

/// The module folder for the executable `exe`: the first candidate holding a CPU backend module. None found: a
/// `release` build still names the first candidate (so llama.cpp reports the modules missing rather than loading
/// from the build machine's folder); a debug build returns `None` and uses the build's own folder.
#[must_use]
pub fn module_dir_for(exe: &Path, release: bool) -> Option<PathBuf> {
    let exe_dir = exe.parent()?;
    let all = candidates(exe_dir);
    let found = all
        .iter()
        .find(|d| !cpu_backend::module_files(d).is_empty())
        .cloned();
    match found {
        Some(dir) => Some(dir),
        None if release => all.into_iter().next(),
        None => None,
    }
}

/// Name this executable's module folder for llama.cpp (x86_64 Windows / Linux; a no-op elsewhere). Call before
/// anything loads a model. Returns the folder named.
pub fn install() -> Option<PathBuf> {
    if !MODULES {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let dir = module_dir_for(&exe, !cfg!(debug_assertions))?;
    cpu_backend::set_module_dir(&dir);
    Some(dir)
}

/// The value after `--model`.
fn model_arg(args: &[String]) -> Option<PathBuf> {
    args.iter()
        .position(|a| a == "--model")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
}

/// `borealis --semantic-backend-check [--model <gguf>]`: print the JSON line; exit 0 when a CPU backend loaded.
#[must_use]
pub fn run(args: &[String]) -> i32 {
    let dir = install();
    let modules = dir
        .as_deref()
        .map(cpu_backend::module_files)
        .unwrap_or_default();
    let started = borealis_embed::llama::start_backend();
    let mut out = serde_json::json!({
        "modules_at_run_time": MODULES,
        "module_dir": dir.as_ref().map(|d| d.display().to_string()),
        "modules": modules,
        "devices": started.as_ref().ok(),
        "error": started.as_ref().err().map(ToString::to_string),
    });
    if let (Ok(_), Some(model)) = (&started, model_arg(args)) {
        out["speed_check"] = speed_check(&model);
    }
    println!("{out}");
    i32::from(started.is_err())
}

/// The app's query-speed check (`vector_service`) with this device's tier on the GGUF at `model`.
fn speed_check(model: &Path) -> serde_json::Value {
    let profile = DeviceProfile::probe();
    let tier = profile.tier().config_for(&profile);
    let (Some(file), Some(dims)) = (tier.model, tier.dims) else {
        return serde_json::json!({ "error": "this device has no semantic tier" });
    };
    let loaded = LlamaEmbedder::load_with(
        model,
        file.info(),
        dims,
        tier.threads,
        LlamaOptions::for_max_tokens(tier.max_tokens),
    );
    let embedder = match loaded {
        Ok(e) => e,
        Err(e) => return serde_json::json!({ "error": e.to_string() }),
    };
    let guarded = GuardedEmbedder::new(
        Arc::new(embedder) as Arc<dyn Embedder>,
        SemanticGuard::new(),
    );
    let ms = crate::vector_status::probe_query_ms(&guarded);
    let fast = matches!((ms, tier.query_budget_ms), (Some(ms), Some(budget)) if ms <= budget);
    serde_json::json!({
        "tier": format!("{:?}", tier.tier),
        "quant": file.quant,
        "dims": dims,
        "threads": tier.threads,
        "max_tokens": tier.max_tokens,
        "cores": profile.cores,
        "query_ms_median": ms,
        "budget_ms": tier.query_budget_ms,
        "verdict": if fast { "hybrid" } else { "keyword-only" },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        let name = format!(
            "{}x64.{}",
            cpu_backend::MODULE_PREFIX,
            cpu_backend::MODULE_EXTENSION
        );
        std::fs::write(dir.join(name), b"").unwrap();
    }

    #[test]
    fn the_flag_is_the_first_argument_only() {
        let args = |a: &[&str]| a.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert!(requested(&args(&[FLAG])));
        assert!(requested(&args(&[FLAG, "--model", "m.gguf"])));
        assert!(!requested(&args(&["character.dnd5e", FLAG])));
        assert!(!requested(&args(&[])));
        assert_eq!(
            model_arg(&args(&[FLAG, "--model", "m.gguf"])),
            Some(PathBuf::from("m.gguf"))
        );
        assert_eq!(model_arg(&args(&[FLAG, "--model"])), None);
    }

    #[test]
    fn the_bundle_layout_is_found_next_to_the_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let (bin, expected) = if cfg!(windows) {
            (
                tmp.path().join("Borealis"),
                tmp.path().join("Borealis/ggml"),
            )
        } else {
            (
                tmp.path().join("usr/bin"),
                tmp.path().join("usr/bin/../lib/Borealis/ggml"),
            )
        };
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("borealis");
        assert_eq!(module_dir_for(&exe, false), None, "debug: nothing staged");
        assert_eq!(
            module_dir_for(&exe, true),
            Some(expected.clone()),
            "release: the install folder even when empty, never the build machine's"
        );
        module(&expected);
        assert_eq!(module_dir_for(&exe, false), Some(expected));
    }

    #[cfg(not(windows))]
    #[test]
    fn linux_falls_back_to_ggml_next_to_the_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("release/borealis");
        module(&tmp.path().join("release/ggml"));
        assert_eq!(
            module_dir_for(&exe, true),
            Some(tmp.path().join("release/ggml"))
        );
    }
}
