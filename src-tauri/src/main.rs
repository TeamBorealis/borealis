// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Windows GUI-subsystem builds have no console: attach to the parent's for CLI/MCP runs (B-W1).
    #[cfg(desktop)]
    borealis_lib::console_attach::attach_if_needed(
        ai_cli_requested(&args) || backend_check_requested(&args),
    );
    // `borealis --semantic-backend-check [--model <gguf>]` (B-49): which llama.cpp CPU backend loads here.
    #[cfg(all(desktop, feature = "llama"))]
    if borealis_lib::semantic_backend::requested(&args) {
        std::process::exit(borealis_lib::semantic_backend::run(&args));
    }
    #[cfg(ai_cli)]
    if borealis_lib::mcp_mode::requested(&args) {
        std::process::exit(borealis_lib::mcp_mode::run(&args));
    }
    // `borealis <command> --library <dir>` (AI15): the drive CLI, before any window exists.
    #[cfg(ai_cli)]
    if borealis_lib::drive::cli::requested(&args) {
        std::process::exit(borealis_lib::drive::cli::run(&args));
    }
    // Builds without the AI CLI integration (the Mac App Store build, MAS1) have no MCP mode: say so
    // instead of opening the window for a program that expects an MCP server on stdio.
    #[cfg(not(ai_cli))]
    if borealis_lib::mcp_mode::requested(&args) {
        eprintln!("borealis --mcp: this build of Borealis has no MCP mode");
        std::process::exit(2);
    }
    borealis_lib::run()
}

/// The arguments ask for the `--mcp` server or a drive CLI command (AI15).
#[cfg(ai_cli)]
fn ai_cli_requested(args: &[String]) -> bool {
    borealis_lib::mcp_mode::requested(args) || borealis_lib::drive::cli::requested(args)
}

#[cfg(all(desktop, not(ai_cli)))]
fn ai_cli_requested(_args: &[String]) -> bool {
    false
}

#[cfg(all(desktop, feature = "llama"))]
fn backend_check_requested(args: &[String]) -> bool {
    borealis_lib::semantic_backend::requested(args)
}

#[cfg(all(desktop, not(feature = "llama")))]
fn backend_check_requested(_args: &[String]) -> bool {
    false
}
