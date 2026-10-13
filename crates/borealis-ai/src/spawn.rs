//! Platform details of starting helper programs from a GUI app.
//!
//! On Windows a GUI-subsystem app that starts a console program (`claude.exe`, `codex.cmd`,
//! `npm.cmd`, `taskkill`) gets a console window flashing up unless the child is created with
//! `CREATE_NO_WINDOW`. Executable lookup also differs: a bare name only runs through its
//! extension (`.exe`, or `.cmd`/`.bat` which std runs via `cmd.exe`), and npm installs an
//! extension-less `#!/bin/sh` shim next to the `.cmd` that Windows cannot start.

/// `CREATE_NO_WINDOW` process creation flag (winbase.h).
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Extensions tried, in order, when resolving a bare program name on Windows. `.ps1` and the
/// extension-less npm shim are deliberately absent: `CreateProcess` cannot start them.
pub const WINDOWS_LAUNCHER_EXTS: [&str; 3] = [".exe", ".cmd", ".bat"];

/// File names to look for when resolving `base` on a directory search path.
#[must_use]
pub fn launcher_names(base: &str, windows: bool) -> Vec<String> {
    if windows {
        WINDOWS_LAUNCHER_EXTS
            .iter()
            .map(|ext| format!("{base}{ext}"))
            .collect()
    } else {
        vec![base.to_string()]
    }
}

/// [`launcher_names`] for the platform this binary runs on.
#[must_use]
pub fn host_launcher_names(base: &str) -> Vec<String> {
    launcher_names(base, cfg!(windows))
}

/// Keep a console child from opening a window (Windows); a no-op elsewhere.
pub fn hide_console(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// [`hide_console`] for `std::process::Command`.
pub fn hide_console_std(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_names_try_exe_then_cmd_then_bat_and_never_the_bare_shim() {
        assert_eq!(
            launcher_names("claude", true),
            ["claude.exe", "claude.cmd", "claude.bat"]
        );
        assert!(!launcher_names("npm", true).contains(&"npm".to_string()));
    }

    #[test]
    fn unix_names_are_the_bare_program() {
        assert_eq!(launcher_names("codex", false), ["codex"]);
    }

    #[test]
    fn host_names_match_the_build_target() {
        assert_eq!(
            host_launcher_names("codex"),
            launcher_names("codex", cfg!(windows))
        );
    }
}
