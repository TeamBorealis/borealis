# Building Borealis 1.0.0

All commands run from the repository root. Prereq: `source ~/.cargo/env`; `npm --prefix ui ci`.

## macOS (aarch64)

    npm run tauri -- build

Runs `npm --prefix ui run build` (gen-contract, `tsc --noEmit`, vite) then cargo release.
Measured: ~3m47s wall (cold crates), release binary 25 MB.

| Artifact | Path | Size |
|---|---|---|
| App | target/release/bundle/macos/Borealis.app | 30 MB |
| DMG | target/release/bundle/dmg/Borealis_1.0.0_aarch64.dmg | 11 MB |

Identifier `app.borealis`, name `Borealis`, version 1.0.0 (Info.plist verified).
Resources in `Contents/Resources`: `content/{core,srd-5.1,srd-5.2.1,srd-5.1-monsters,srd-5.2.1-monsters}`,
`ai-profile/{claude,codex}`. The UI bundles the Selawik woff2 fonts (`ui/src/assets/fonts/`, OFL); the sheet renderer embeds Noto fonts (`crates/borealis-sheet/fonts/`, OFL). See `NOTICE`.

### Smoke checks
MCP (bundled content resolves from `../Resources/content`; logs `ready (3802 elements)`):

    B=target/release/bundle/macos/Borealis.app/Contents/MacOS/borealis
    M='"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}'
    printf '{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{%s}}\n{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{%s}}\n' "$M" "$M" | $B --mcp

Both `_meta` keys are mandatory. tools/list returns 7 tools. GUI: running `$B` stays alive 10s+.
App data: `~/Library/Application Support/app.borealis/` (`ai-setup.json`, `characters/`, `library/`,
`ai-profile/{manifest.json,installed-for-app-version,profile/}`).

## Android (debug APK, aarch64)

Needs JDK 17 (not on PATH by default), SDK with NDK 27.2.12479018, rust android targets installed.

    export JAVA_HOME=/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home
    export ANDROID_HOME=$HOME/Library/Android/sdk ANDROID_SDK_ROOT=$ANDROID_HOME
    export NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018 ANDROID_NDK_HOME=$NDK_HOME
    T=$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin
    export CC_aarch64_linux_android=$T/aarch64-linux-android24-clang
    export AR_aarch64_linux_android=$T/llvm-ar
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$T/aarch64-linux-android24-clang
    npm run tauri -- android build --debug --target aarch64 --apk

~3m52s. Output (git-ignored):
`src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`, 435 MiB
(unstripped debug `libborealis_lib.so` is 447 MB uncompressed; release build will be far smaller).
Content packs are in `assets/content/`.

### Release signing (Android)

The committed Gradle project signs the `release` build only when both env vars are set; otherwise release
builds stay unsigned as before. The keystore is PKCS12, key alias `borealis`, key password equal to the store
password. `ANDROID_KEYSTORE_PATH` is the keystore file path and `ANDROID_KEYSTORE_PASSWORD` its password.
In the release workflow they come from the repository secrets `ANDROID_KEYSTORE_B64` (base64 of the keystore,
decoded to a temp file) and `ANDROID_KEYSTORE_PASSWORD`. Local signed build (arm64):
`ANDROID_KEYSTORE_PATH=... ANDROID_KEYSTORE_PASSWORD=... npm run tauri -- android build --target aarch64 --apk`,
output `src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk`
(about 21 MiB; arm64 only despite the Gradle variant name, published as `Borealis_<version>_arm64.apk`); check with `apksigner verify --print-certs`.

## iOS (simulator, debug, aarch64-sim) - verified 2026-10-08

Prereqs (no sudo): Xcode 27 (license accepted), `brew install cocoapods xcodegen` (the gem route needs sudo),
rustup targets `aarch64-apple-ios-sim aarch64-apple-ios`. `xcode-select` may still point at CommandLineTools;
`DEVELOPER_DIR` alone is NOT enough because tauri spawns `xcodebuild` without it. Either run
`sudo xcode-select -s /Applications/Xcode.app/Contents/Developer` once, or put a shim named `xcodebuild` first
on PATH that exports DEVELOPER_DIR and execs `$DEVELOPER_DIR/usr/bin/xcodebuild`.

    source ~/.cargo/env; export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
    npm run tauri -- ios init --ci          # generates src-tauri/gen/apple (committed; assets/ and build/ ignored)
    npm run tauri -- ios build --debug --target aarch64-sim
    APP=$(ls -d ~/Library/Developer/Xcode/DerivedData/borealis-*/Build/Products/debug-iphonesimulator/Borealis.app)
    xcrun simctl boot "iPhone 17"; xcrun simctl install booted "$APP"; xcrun simctl launch booted app.borealis

Result: BUILD SUCCEEDED in ~1.5 min, no Rust cfg changes needed (aws-lc-sys cross-compiled fine). Borealis.app is
214 MB (debug), bundles `assets/content/{core,srd-5.1,srd-5.2.1,*-monsters}` and `assets/ai-profile`. Launches on
iPhone 17 / iOS 27.0, renders the Characters page; app data dir `Library/Application Support/app.borealis/`
(`library/`, `characters/`).

Device / App Store build (not attempted): needs an Apple Developer team ID
(`bundle > iOS > developmentTeam` in tauri.conf.json or env `APPLE_DEVELOPMENT_TEAM`), a signing certificate and
provisioning profile for `app.borealis` in the keychain, then `npm run tauri -- ios build --target aarch64`
(or `--export-method app-store-connect`) and upload via Transporter/`xcrun altool`.

## Windows (x64 / arm64 NSIS installer) - cross-compiled from macOS, verified 2026-10-13

Tauri's supported macOS -> Windows path: `cargo-xwin` (downloads the MSVC CRT + Windows SDK on first use,
accepting Microsoft's licence, cache in `~/Library/Caches/cargo-xwin`) and NSIS. One-time setup:

    rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc
    cargo install --locked cargo-xwin
    brew install nsis llvm lld

Build (separate target dir so the macOS `target/` is untouched; `target-windows/` is git-ignored):

    source ~/.cargo/env
    export PATH=/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/lld/bin:$PATH CARGO_TARGET_DIR=$PWD/target-windows
    npm run tauri -- build --runner cargo-xwin --target x86_64-pc-windows-msvc
    npm run tauri -- build --runner cargo-xwin --target aarch64-pc-windows-msvc   # optional
    cargo xwin clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings

x64 ~8 min cold on an idle M-series Mac (arm64 took 29 min on a heavily loaded one). The bundler downloads `nsis_tauri_utils.dll` and the WebView2 bootstrapper on the
first run.

| Artifact | Path (under target-windows/) | Size |
|---|---|---|
| x64 installer | x86_64-pc-windows-msvc/release/bundle/nsis/Borealis_1.0.0_x64-setup.exe | 12.5 MiB |
| x64 exe | x86_64-pc-windows-msvc/release/borealis.exe | 40 MiB |
| arm64 installer | aarch64-pc-windows-msvc/release/bundle/nsis/Borealis_1.0.0_arm64-setup.exe | 11.3 MiB |
| arm64 exe | aarch64-pc-windows-msvc/release/borealis.exe | 34 MiB |

Windows settings live in `src-tauri/tauri.windows.conf.json` (merged over tauri.conf.json): NSIS only (no MSI,
which needs WiX on a Windows host), per-user install (no admin prompt) into
`%LOCALAPPDATA%\Borealis`, the WebView2 bootstrapper embedded and run silently, and file associations
for `.dnd5e` and `.index` only (`.xml`/`.zip` would take over the system default handler on Windows). The exe
is GUI-subsystem in release (no console window), links the VC runtime statically (needs only the UCRT that ships
with Windows 10+), and is ~40 MB; content packs and `ai-profile/` install beside it as `content\...`.
App data: `%APPDATA%\app.borealis\`. `borealis.exe --mcp` finds `content\` next to the exe.

### Limits
- **Unsigned.** Signing needs a Windows host or a custom `bundle > windows > signCommand` and a code-signing
  certificate. Unsigned installers trigger SmartScreen ("Windows protected your PC" -> More info -> Run anyway)
  until the certificate (or an EV certificate) builds reputation.
- **WebView2.** Windows 11 ships it; Windows 10 machines without it get the embedded bootstrapper, which
  downloads the Evergreen runtime (needs internet during install). Offline installs would need
  `webviewInstallMode: offlineInstaller` (+~130 MB).
- Cross-compiled builds are "experimental" per Tauri; nothing was run on real Windows here (no VM, no Rosetta
  for wine; the wine-stable cask is disabled by Homebrew). CI's `windows-latest` jobs are the real test.
- Tests whose fake `claude`/`codex`/`npm` are `#!/bin/sh` scripts are `cfg(unix)` (sec_wire, slice3b,
  slice3c2, parts of slice3c1/picked_paths/play_session).
- On a Windows source checkout set `git config core.autocrlf false` before cloning: the AI profile is
  hash-verified and CRLF conversion makes it "repaired" on every start.
- A second double-click on a `.dnd5e` while Borealis runs opens a second instance (no single-instance plugin).

### CI (.github/workflows/ci.yml)
Push/PR run Linux + Windows only: `test` (clippy `-D warnings`, `cargo test --workspace`; fmt and UI tests
on Linux only) then `desktop-bundle` (release; uploads the NSIS `.exe` and the Linux `.deb`/`.AppImage`).
macOS, iOS and Android jobs run only from "Run workflow" (workflow_dispatch). Windows jobs set
`core.autocrlf false` before checkout; the Tauri CLI runs as `node ui/node_modules/@tauri-apps/cli/tauri.js`
(also what `npm run tauri` now calls) because Windows cannot start the extension-less `.bin/tauri` shim.

## Linux (deb / rpm / AppImage) - built in Docker on macOS, verified 2026-10-13

`build/linux/` holds an Ubuntu 24.04 image (Tauri 2 deps: webkit2gtk-4.1, gtk3, ayatana-appindicator,
librsvg, patchelf; Node 22; Rust stable) and `run.sh`, which runs a command in it with the repo bind-mounted at
`/src`. The target dir, cargo registry, `ui/node_modules` and npm cache are named Docker volumes
(`borealis-linux-target-<arch>`, `borealis-linux-cargo-registry`, `borealis-linux-node-modules-<arch>`,
`borealis-linux-npm-cache`), so the macOS `target/` and `ui/node_modules` are never touched. The container
builds as a non-root user (root bypasses the read-only-folder tests) with 4 jobs and no debuginfo; with 10 jobs
and debuginfo the 8 GB Docker Desktop VM OOM-kills the linker.

    build/linux/run.sh 'cargo test --workspace'                      # 1203 pass, 7 ignored (aarch64)
    build/linux/run.sh 'cargo clippy --workspace --all-targets -- -D warnings'
    build/linux/run.sh 'cd ui && npm ci && npm test && npm run build'  # 267 tests
    build/linux/run.sh build/linux/bundle.sh                         # tauri build --ci (+ Rosetta AppImage fix)
    PLATFORM=linux/amd64 build/linux/run.sh '...'                    # x86_64 under Rosetta (Docker Desktop)

Cold aarch64 times: test ~12 min, release bundle ~17 min (2.5 min warm). x86_64 runs under Rosetta
(`PLATFORM=linux/amd64`): release bundle ~19 min cold, tests ~16 min (1201 pass; 2 fail only under Rosetta,
which reports a missing executable as exit 127 instead of ENOENT: `session::missing_binary_is_a_spawn_error`,
`golden_raster`'s pdftoppm probe; native x86_64 CI is the real check). There the AppImage tools fail with "Exec format error"
(Rosetta's binfmt rule rejects the AppImage magic at ELF offset 8); `bundle.sh` zeroes it and retries.
Artifacts are in the target volume (copy out with `build/linux/run.sh 'cp /target/release/bundle/deb/*.deb /src/target/'` or `docker cp`):

| Artifact | Path (in /target/release/bundle) | aarch64 | x86_64 |
|---|---|---|---|
| deb | deb/Borealis_1.0.0_<arch>.deb | 14.2 MiB | 15.1 MiB |
| rpm | rpm/Borealis-1.0.0-1.<arch>.rpm | 14.2 MiB | 15.1 MiB |
| AppImage | appimage/Borealis_1.0.0_<arch>.AppImage | 85.9 MiB | 88.7 MiB |
| binary | /target/release/borealis | 28.6 MiB | 33.4 MiB |

Layout (deb/rpm): `/usr/bin/borealis`, resources in `/usr/lib/Borealis/{content,ai-profile}`, desktop entry
`/usr/share/applications/Borealis.desktop`, MIME package `/usr/share/mime/packages/borealis.xml`. deb depends
on `libwebkit2gtk-4.1-0, libgtk-3-0`. The AppImage bundles webkit/gtk but, per AppImage policy, expects the host
to provide the graphics stack and gtk3 base (`libgtk-3-0`, `libegl1`), as every desktop does.
App data: `~/.local/share/app.borealis/`. `borealis --mcp` finds `../lib/Borealis/content` next to the binary
(deb, rpm, AppImage, and the unpacked `bundle/deb/*/data/usr/bin/borealis`).

Linux settings live in `src-tauri/tauri.linux.conf.json`: desktop template `src-tauri/linux/borealis.desktop`
(`Exec=borealis %F`, so "Open with" passes the file; the app queues argv files like Windows),
`Categories=Game;RolePlaying;`, and the `application/x-dnd5e` (`*.dnd5e`) shared-mime-info package
(`src-tauri/linux/borealis-mime.xml`). `.xml` and `.zip` are listed as MimeTypes, which only adds Borealis to
"Open with"; it does not become the default handler.

### Launch smoke
`build/linux/smoke.sh <binary> <shot-dir> [label] [file]` starts Xvfb, checks `--mcp` (server/discover +
tools/list), launches the GUI under `dbus-run-session`, screenshots splash/main window, clicks START > NEW
CHARACTER, names the hero and creates it, and optionally relaunches with a file argument. Clean installs:

    docker run --rm -v borealis-linux-target-arm64:/target:ro -v "$PWD/build/linux":/smoke:ro -v <shots>:/shots \
      ubuntu:24.04 bash -c 'apt-get update && apt-get install -y /target/release/bundle/deb/*.deb xvfb xauth \
      dbus-x11 xdotool imagemagick && useradd -m t && su t -c "/smoke/smoke.sh /usr/bin/borealis /shots deb"'

Verified (aarch64): deb on Ubuntu 24.04, rpm on Fedora 42 (`dnf install ./*.rpm`), AppImage on Ubuntu 24.04 with only
gtk3 + EGL installed (`APPIMAGE_EXTRACT_AND_RUN=1`, no FUSE in containers). All: MCP `ready (3802 elements)`, main
window 1200x800 renders Characters, a character is created and listed after a restart, and a file passed as an
argument reaches the UI's import route. x86_64: deb on Ubuntu 24.04 (amd64 under Rosetta), same checks. Software rendering in Xvfb needs `WEBKIT_DISABLE_COMPOSITING_MODE=1`
(set by smoke.sh); real desktops do not.

### Limits
- Unsigned packages (no deb/rpm repo signing, no AppImage signature or update info).
- Not run on real hardware or Wayland; only Xvfb (X11) in containers.
