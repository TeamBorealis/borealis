//! `borealis-updater-check <pubkey (base64, as in tauri.conf.json)> <file> <file.sig> <version>`
//!
//! Exit 0 when `<file>.sig` (written by `tauri signer sign --app-version <version>`) verifies with the public key and
//! was signed for `<version>`; exit 1 with the reason otherwise; exit 2 on bad arguments. Used by
//! `build/ci/updater.sh check-key` in the release workflow (B-203). It reads only public material.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [pubkey, file, sig, version] = args.as_slice() else {
        eprintln!("usage: borealis-updater-check <pubkey base64> <file> <file.sig> <version>");
        return ExitCode::from(2);
    };
    let read = |path: &str| std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"));
    let checked = read(file).and_then(|data| {
        let sig = String::from_utf8(read(sig)?).map_err(|_| format!("{sig} is not text"))?;
        borealis_updater_check::verify(pubkey, &data, &sig, version).map_err(|e| e.to_string())
    });
    match checked {
        Ok(()) => {
            println!(
                "the updater signing key matches plugins.updater.pubkey (signed for {version})"
            );
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!("{reason}");
            ExitCode::FAILURE
        }
    }
}
