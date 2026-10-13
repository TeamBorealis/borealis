//! `borealis-migrate` command line entry point.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = borealis_migrate::cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    std::process::exit(code);
}
