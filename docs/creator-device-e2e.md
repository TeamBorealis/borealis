# Homebrew creator: device suite (Android emulator, iOS simulator)

The creator's phone flows of spec 18.4 (flow 10: flows 1, 3, 4 and 8 on a phone) and the phone budget of spec 18.5
(save + reload under 4 s), on the real app with its Rust backend. The scripts are in `e2e/scripts/devices/`:

| file | what |
|---|---|
| `run.mjs` | the runner: fresh install with the build-identity check, the creator switched on, the flows, `results.json` |
| `flows.mjs` | the flows and the budget, as real touches through the phone UI |
| `install.mjs` | install, identity (sha256 of the built product = the installed one; HEAD; a stale build is refused), launch, kill |
| `driver.mjs` | reads the page through the WebView inspector, taps for real (Android `adb input tap`, iOS XCTest press) |
| `rwi.py` | the iOS simulator's WebKit remote inspector as a small HTTP bridge (`POST /eval`) |

What each flow does on the device (every tap is a real touch; text is put in through the page, the iOS keyboard check
types for real):

- **flow 1:** New pack (sheet) > New element (sheet) > a spell: level and school from U22 select sheets, the Wizard
  list > pinned Save > pinned Try it: the trial Wizard at level 3 knows the spell and has 2nd-level slots (the real engine).
- **flow 3:** a class: hit die (sheet), three skills (multi-select picker), the LEVELS row sheet at level 3 (Subclass),
  half-caster spellcasting (sheets; slots 4,2 at level 5), starting equipment > Save > Try it: 20 ladder rows without
  a problem, level 20, then "Try as a second class" (Fighter 1, then this class).
- **flow 4:** a pack for both editions; a species with darkvision 60 and a lineage trait with two options (child sheets)
  > Save > Try it offers the lineage's options; a 2014 race (+2 Con) and its subrace (+1 Cha), each saved.
- **flow 8:** the saved spell edited (name, a rule from the Add rule sheet and the block sheet), Undo x2 and Redo x2 on
  the pinned row, then the app killed (Android `am force-stop`, iOS `kill -9` of the app process) and launched again:
  opening the spell asks "Keep editing" and the draft comes back with its name and rule.
- **budget:** pinned Save timed in the page, from the tap's click event to the status line's "Saved '...'": three saves
  of a deep copy of the SRD 2024 Wizard (class, features, subclass) and three of a spell. Each must be under 4 s; every
  save of the flows is recorded too.

## Build

From the app folder (`app/` or a worktree of it; a worktree builds into its own `target/`):

    # Android (debug APK; JAVA_HOME = JDK 17, ANDROID_HOME, NDK_HOME as in docs/build.md, Android)
    npm run tauri -- android build --debug --target aarch64 --apk
    # iOS simulator (full Xcode: DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer)
    npm run tauri -- ios build --debug --target aarch64-sim

The runner refuses a build older than the last commit that touched `ui/`, `src-tauri/` or `crates/`, or a tree with
uncommitted changes there (`--allow-stale` runs it anyway and records `fresh: false`).

## Run

Boot the device first: the AVD `borealis` (`emulator -avd borealis`; `ANDROID_SERIAL` or `E2E_ANDROID_DEVICE` pick
another) or the simulator "iPhone 18 Pro" (`xcrun simctl boot <udid>`; `E2E_IOS_DEVICE` / `E2E_IOS_UDID` pick another).
Then, from `e2e/`:

    node scripts/devices/run.mjs android
    node scripts/devices/run.mjs ios
    node scripts/devices/run.mjs ios --only flow1,flow8      # some flows; --no-install keeps the installed app

The run uninstalls the app (its data goes with it), installs the build, checks the installed bytes against the built
ones, writes `settings.json` `{"creatorEnabled": true}` into the app's data folder (Android through `run-as`, iOS in the
simulator's data container), launches it and runs the flows in order (each needs the ones before it). Evidence
(`flowN-*.png`, `FAIL-*.png`) and `results.json` (identity, steps, every timed save) go to
`target/creator-devices/<platform>/`. The exit code is 0 only if every step passed and every save was under 4 s.

Ports: CDP on 9333 (`E2E_CDP_PORT`), the iOS bridge on 9334 (`E2E_RWI_PORT`). The runner starts and stops the bridge
and its agent-device session itself; it leaves the emulator and the simulator running (shut them down when done).

## Notes

- Android reads the page through the debug WebView's DevTools socket (`webview_devtools_remote_<pid>`, forwarded by
  adb). iOS reads it through the simulator's inspector socket (`RWI_LISTEN_SOCKET`): the debug build's WKWebView is
  inspectable; nothing in the app is changed for the tests.
- iOS taps use agent-device's XCTest runner; under heavy load its press can hit the runner's watchdog. The driver tries
  such a press again (4 tries) before failing the step.
- The Try it trial is the real engine on the device, so it stands for "a character takes it" there. The full character
  halves (Build pages, casting, levelling, multiclassing, equipping) run on the real backend in
  `src-tauri/tests/homebrew_flows.rs`, and flow 1 also through the real macOS window (`e2e/tests/desktop/d12-creator-spell.e2e.ts`).
