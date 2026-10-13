//! `borealis-import-5etools <data-dir> -o <out-dir>`: convert a local 5etools folder into
//! Borealis packs. See `--help`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = borealis_import_5etools::cli::run(
        args,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    );
    std::process::exit(code);
}
