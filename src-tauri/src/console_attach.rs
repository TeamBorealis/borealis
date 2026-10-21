//! Windows release builds are GUI-subsystem executables, so a process started from cmd or
//! PowerShell has no console handles and CLI/MCP output goes nowhere (B-W1). In CLI/MCP mode this
//! attaches to the parent console, but only when std handles are not already valid, so pipes
//! and redirects (which MCP over stdio needs) keep working. GUI launches never call this.

/// Which std handles are usable (not null, invalid, or of unknown type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StdHandles {
    pub stdin_ok: bool,
    pub stdout_ok: bool,
    pub stderr_ok: bool,
}

/// Attach to the parent console only for a CLI/MCP run whose output would otherwise be lost.
/// stdin alone does not matter: the CLI does not read it, and MCP pipes it.
pub fn should_attach(is_cli_mode: bool, handles: StdHandles) -> bool {
    is_cli_mode && !(handles.stdout_ok && handles.stderr_ok)
}

/// Call first thing in CLI/MCP mode, before any output. No-op off Windows.
pub fn attach_if_needed(is_cli_mode: bool) {
    #[cfg(windows)]
    imp::attach_if_needed(is_cli_mode);
    #[cfg(not(windows))]
    let _ = is_cli_mode;
}

#[cfg(windows)]
mod imp {
    use super::{should_attach, StdHandles};
    use std::fs::OpenOptions;
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{GetFileType, FILE_TYPE_UNKNOWN};
    use windows_sys::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    fn is_valid(kind: u32) -> bool {
        // SAFETY: GetStdHandle/GetFileType only read process state; any value is safe to query.
        unsafe {
            let handle: HANDLE = GetStdHandle(kind);
            !handle.is_null()
                && handle != INVALID_HANDLE_VALUE
                && GetFileType(handle) != FILE_TYPE_UNKNOWN
        }
    }

    fn reopen(kind: u32, device: &str, write: bool) {
        if is_valid(kind) {
            return;
        }
        let file = OpenOptions::new().read(!write).write(write).open(device);
        if let Ok(file) = file {
            // SAFETY: the handle is owned (into_raw_handle) and stays open for the process life.
            unsafe { SetStdHandle(kind, file.into_raw_handle() as HANDLE) };
        }
    }

    pub fn attach_if_needed(is_cli_mode: bool) {
        let handles = StdHandles {
            stdin_ok: is_valid(STD_INPUT_HANDLE),
            stdout_ok: is_valid(STD_OUTPUT_HANDLE),
            stderr_ok: is_valid(STD_ERROR_HANDLE),
        };
        if !should_attach(is_cli_mode, handles) {
            return;
        }
        // SAFETY: plain Win32 call; failure (no parent console, e.g. double-click) is ignored.
        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
            return;
        }
        reopen(STD_OUTPUT_HANDLE, "CONOUT$", true);
        reopen(STD_ERROR_HANDLE, "CONOUT$", true);
        reopen(STD_INPUT_HANDLE, "CONIN$", false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_OK: StdHandles = StdHandles {
        stdin_ok: true,
        stdout_ok: true,
        stderr_ok: true,
    };
    const NONE_OK: StdHandles = StdHandles {
        stdin_ok: false,
        stdout_ok: false,
        stderr_ok: false,
    };

    #[test]
    fn gui_launch_never_attaches() {
        assert!(!should_attach(false, NONE_OK));
    }

    #[test]
    fn piped_cli_keeps_its_handles() {
        assert!(!should_attach(true, ALL_OK));
    }

    #[test]
    fn cli_without_output_handles_attaches() {
        assert!(should_attach(true, NONE_OK));
        assert!(should_attach(
            true,
            StdHandles {
                stdout_ok: false,
                ..ALL_OK
            }
        ));
        assert!(should_attach(
            true,
            StdHandles {
                stderr_ok: false,
                ..ALL_OK
            }
        ));
    }

    #[test]
    fn missing_stdin_alone_does_not_attach() {
        assert!(!should_attach(
            true,
            StdHandles {
                stdin_ok: false,
                ..ALL_OK
            }
        ));
    }
}
