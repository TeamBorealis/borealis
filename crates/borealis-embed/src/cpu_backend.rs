//! llama.cpp's CPU backend on x86_64 Windows and Linux (B-49). These builds turn on llama-cpp-sys-2's
//! `dynamic-backends` (`GGML_BACKEND_DL` + `GGML_CPU_ALL_VARIANTS`): llama.cpp is three shared libraries (`llama`,
//! `ggml`, `ggml-base`, all at the SSE2 baseline, X86-1) plus one CPU backend module per x86 level, from
//! `ggml-cpu-x64` (SSE2) through `sse42`, `sandybridge` (AVX), `haswell` (AVX2 + FMA) and the AVX-512 / AMX ones.
//! ggml asks each module for its score on the running CPU (CPUID, compiled without the module's own SIMD flags)
//! and loads the best one, so an SSE2-only CPU gets `x64` and keeps working, and an AVX2 CPU gets `haswell` or better.
//! Every other target (arm64, the x86_64 phone and simulator builds) links the CPU backend statically: nothing to load.
//!
//! * The app names the folder its installer put the modules in ([`set_module_dir`], before the first model loads:
//!   `src-tauri/src/semantic_backend.rs`). Without that call (tests, developer tools) the build's own module folder
//!   is used (`llama_cpp_2::llama_backend::load_backends`).
//! * Only that one folder is searched. When no CPU backend loads, llama.cpp is not initialised at all: its
//!   `llama_backend_init` would otherwise search the executable's folder and the current folder for modules.
//! * ggml writes one line to stderr when it loads a module ("load_backend: loaded CPU backend from <path>"); CI reads
//!   it to record which level a machine got.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// This build loads its CPU backend from module files at run time (x86_64 Windows and Linux).
pub const MODULES: bool = crate::tier::X86_CPU_DISPATCH;

/// File name prefix of the CPU backend modules (`ggml-cpu-haswell.dll`, `libggml-cpu-haswell.so`).
pub const MODULE_PREFIX: &str = if cfg!(windows) {
    "ggml-cpu-"
} else {
    "libggml-cpu-"
};

/// File name extension of the modules.
pub const MODULE_EXTENSION: &str = if cfg!(windows) { "dll" } else { "so" };

/// Shown (after "llama.cpp could not start: ") when no CPU backend module loads.
pub const MISSING: &str =
    "the processor support files for semantic search are missing from the Borealis \
                           folder, so search uses keywords. Reinstall Borealis to restore them";

static MODULE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Use the CPU backend modules in `dir` (the app's install folder). Only the first call counts, and it must come
/// before the first model loads; returns whether this call set it. Builds without [`MODULES`] ignore it.
pub fn set_module_dir(dir: &Path) -> bool {
    MODULE_DIR.set(dir.to_path_buf()).is_ok()
}

/// The folder [`set_module_dir`] named, if any.
#[must_use]
pub fn module_dir() -> Option<&'static Path> {
    MODULE_DIR.get().map(PathBuf::as_path)
}

/// The CPU backend module files in `dir`, sorted (`ggml-cpu-haswell.dll`, ...).
#[must_use]
pub fn module_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| is_module_name(name))
        .collect();
    names.sort();
    names
}

/// `name` is a CPU backend module file name of this platform.
fn is_module_name(name: &str) -> bool {
    name.strip_prefix(MODULE_PREFIX)
        .and_then(|rest| rest.strip_suffix(MODULE_EXTENSION))
        .and_then(|tag| tag.strip_suffix('.'))
        .is_some_and(|tag| !tag.is_empty())
}

/// The backend devices llama.cpp has registered, as "backend: description" (for example "CPU: Intel(R) Xeon(R)
/// Platinum 8370C CPU @ 2.80GHz"). Empty before the first model load, or when no backend loaded.
#[must_use]
pub fn devices() -> Vec<String> {
    llama_cpp_2::list_llama_ggml_backend_devices()
        .into_iter()
        .map(|d| format!("{}: {}", d.backend, d.description))
        .collect()
}

/// A CPU device is registered.
#[cfg(all(
    target_arch = "x86_64",
    any(target_os = "windows", target_os = "linux")
))]
fn has_cpu() -> bool {
    llama_cpp_2::list_llama_ggml_backend_devices()
        .iter()
        .any(|d| d.device_type == llama_cpp_2::LlamaBackendDeviceType::Cpu)
}

/// Load the best CPU backend module for this CPU (once, before `llama_backend_init`).
///
/// # Errors
/// [`MISSING`] when no CPU backend could be loaded (no module in the folder, or the folder path is not valid
/// UTF-8, which llama.cpp needs).
#[cfg(all(
    target_arch = "x86_64",
    any(target_os = "windows", target_os = "linux")
))]
pub(crate) fn load() -> Result<(), String> {
    if has_cpu() {
        return Ok(());
    }
    match module_dir() {
        Some(dir) if dir.to_str().is_some_and(|s| !s.contains('\0')) => {
            llama_cpp_2::llama_backend::load_backends_from_path(dir);
        }
        Some(_) => return Err(MISSING.into()),
        None => llama_cpp_2::llama_backend::load_backends(),
    }
    if has_cpu() {
        Ok(())
    } else {
        Err(MISSING.into())
    }
}

/// Statically linked CPU backend: always there.
#[cfg(not(all(
    target_arch = "x86_64",
    any(target_os = "windows", target_os = "linux")
)))]
pub(crate) const fn load() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_names_follow_the_platform_pattern() {
        let ext = MODULE_EXTENSION;
        let ok = format!("{MODULE_PREFIX}haswell.{ext}");
        assert!(is_module_name(&ok), "{ok}");
        for bad in [
            format!("{MODULE_PREFIX}.{ext}"),
            format!("{MODULE_PREFIX}haswell.{ext}.bak"),
            format!("ggml-base.{ext}"),
            format!("x{MODULE_PREFIX}haswell.{ext}"),
        ] {
            assert!(!is_module_name(&bad), "{bad}");
        }
    }

    #[test]
    fn module_files_lists_only_modules_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let ext = MODULE_EXTENSION;
        for name in [
            format!("{MODULE_PREFIX}x64.{ext}"),
            format!("{MODULE_PREFIX}haswell.{ext}"),
            "LICENSE-llama.cpp.txt".to_string(),
        ] {
            std::fs::write(tmp.path().join(name), b"").unwrap();
        }
        assert_eq!(
            module_files(tmp.path()),
            vec![
                format!("{MODULE_PREFIX}haswell.{ext}"),
                format!("{MODULE_PREFIX}x64.{ext}")
            ]
        );
        assert!(module_files(&tmp.path().join("missing")).is_empty());
    }

    #[test]
    fn only_x86_64_windows_and_linux_load_modules() {
        let expected = cfg!(target_arch = "x86_64") && (cfg!(windows) || cfg!(target_os = "linux"));
        assert_eq!(MODULES, expected);
    }
}
