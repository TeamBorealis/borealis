// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Windows GUI-subsystem builds have no console: attach to the parent's for CLI/MCP runs (B-W1).
    #[cfg(desktop)]
    borealis_lib::console_attach::attach_if_needed(
        borealis_lib::mcp_mode::requested(&args)
            || borealis_lib::drive::cli::requested(&args)
            || backend_check_requested(&args),
    );
    // `borealis --semantic-backend-check [--model <gguf>]` (B-49): which llama.cpp CPU backend loads here.
    #[cfg(all(desktop, feature = "llama"))]
    if borealis_lib::semantic_backend::requested(&args) {
        std::process::exit(borealis_lib::semantic_backend::run(&args));
    }
    if borealis_lib::mcp_mode::requested(&args) {
        std::process::exit(borealis_lib::mcp_mode::run(&args));
    }
    // `borealis <command> --library <dir>` (AI15): the drive CLI, before any window exists.
    #[cfg(desktop)]
    if borealis_lib::drive::cli::requested(&args) {
        std::process::exit(borealis_lib::drive::cli::run(&args));
    }
    borealis_lib::run()
}

#[cfg(all(desktop, feature = "llama"))]
fn backend_check_requested(args: &[String]) -> bool {
    borealis_lib::semantic_backend::requested(args)
}

#[cfg(all(desktop, not(feature = "llama")))]
fn backend_check_requested(_args: &[String]) -> bool {
    false
}
