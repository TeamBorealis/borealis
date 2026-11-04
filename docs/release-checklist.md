# Release checklist

Run every step for each public release, in order. A release is not cut while any step is open.

Most of the week is automatic: the **Release automation** workflow (`.github/workflows/release-auto.yml`,
`tools/release-schedule/auto.py`) bumps and tags at the freeze, starts CI, import acceptance and the release
builds, submits the iOS build, checks go/no-go and publishes the GitHub release, the iOS release, the website and
the Discord announcement on schedule. Steps marked *(automatic)* below are done by it; the rest stay with people.
The repository variable `RELEASE_HOLD` (a version, or `all`) stops a release; `RELEASE_AUTO` must be `on` for live
runs. Any step runs by hand from Actions > Release automation > Run workflow (`step`, `version`, `dry_run`).

## 1. Gates

- [ ] Full regression gate clean on the release commit: `python3 -I tools/regression-gate/gate.py` with all suites
      (0 regressed, 0 vanished, 0 still failing). See [regression-gate.md](regression-gate.md).
- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
      `cargo metadata --locked`.
- [ ] `ui`: `npx tsc --noEmit` and `npx vitest run`; `e2e`: `npx tsc --noEmit` and the web replay.
- [ ] CI green on the release commit (every job of `ci.yml` and `acceptance.yml`, Linux and Windows).
- [ ] Desktop GUI e2e suites pass on macOS against the release bundle.
- [ ] Performance thresholds hold on a phone and a desktop (`tools/ci/check_thresholds.py`).

## 2. Version

- [ ] *(automatic)* Bump the version in the release files, to the same value: `Cargo.toml`
      (`[workspace.package] version`), the workspace crates in `Cargo.lock`, `README.md`, `src-tauri/tauri.conf.json`,
      the iOS `Info.plist` and `project.yml`, `ui/package.json` and `ui/package-lock.json`
      (`python3 -I tools/release-schedule/bump.py X.Y.Z`; store build number 1.3.0 -> 1003000).
- [ ] Release notes written in `release-notes/X.Y.Z.md` and committed before the freeze (draft:
      `python3 -I tools/release-schedule/notes.py X.Y.Z`, then edit). The automation makes the file the release body
      (HTML comments removed) and the App Store "What's New"; the in-app update flyout shows the release body.
      `notes.py X.Y.Z --check` must report CS1 clean: "5etools" only as "5etools-format", no link or name of a
      non-SRD source.
- [ ] Discord credits in the release notes (`notes.py` adds them): run `python3 -I tools/discord-issues/credits.py` (needs `GH_TOKEN`;
      `--since <previous tag>` when the previous release is not the latest published one) and paste its Markdown,
      which thanks the Discord users whose bug reports and ideas were closed since the previous release.
      The website changelog is built from the release notes, so the credits reach it too.
- [ ] *(automatic)* Discord announcement: when the release is published, the `announce` job of `discord-issues.yml`
      (started by the automation with `announce_tag`) posts the notes to #announcements (`tools/discord-issues/announce.py`). Put a `---` line, with a blank line above it,
      between the notes and the checksums/licence text; everything after it stays out of the announcement.
      Keep "### Fixed in", "### New in" and the community credits above it. Check the post; a missed one can be
      sent with the workflow's `announce_tag` input (`dry_run` previews it).
- [ ] The bundles carry the new version (Info.plist, installer and APK file names).

## 3. External services

- [ ] Re-check the current Model Context Protocol specification version. If it changed, update the
      `protocolVersion` the MCP server advertises (`crates/borealis-mcp/src/rpc.rs`) and the smoke command in
      [build.md](build.md), keeping the legacy `initialize` path.
- [ ] The website's import-report endpoint is deployed and accepts the `logs` field the app sends with a report.

## 4. Privacy and attribution sweep

Run on the exact tree being published:

- [ ] No personal data: names, e-mail addresses other than the project's, local home-directory paths.
- [ ] No AI-tool attribution: `Co-Authored-By` or "Generated with" trailers in commit messages, agent
      instruction files or tool directories (`CLAUDE.md`, `.claude/`), agent handoff text.
- [ ] No third-party rules text beyond the bundled SRD content; user-imported content and caches are not in the tree.
- [ ] No secrets: keystores, tokens, `.env` files.

## 5. Publish

- [ ] *(automatic)* Tag the release commit and push the tag; the release workflow (started with input `tag`) builds
      and signs the artifacts and uploads the iOS build.
- [ ] *(automatic)* iOS submitted for review with manual release; released at the publish time once approved.
- [ ] *(automatic)* Go/no-go at Thursday 18:00 (Israel); publish at 20:00: GitHub release as latest, iOS, website
      (release branch merged into the website's main), IndexNow, Discord.
- [ ] Download each artifact from the release page and smoke-test it: install, start, create a character, import a file.
- [ ] Confirm an older installed version shows the update notice for the new release.
