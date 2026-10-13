// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if borealis_lib::mcp_mode::requested(&args) {
        std::process::exit(borealis_lib::mcp_mode::run(&args));
    }
    borealis_lib::run()
}
