# Release checklist

Run every step for each public release, in order. A release is not cut while any step is open.

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

- [ ] Bump the version in all three places, to the same value:
      `Cargo.toml` (`[workspace.package] version`), `src-tauri/tauri.conf.json` (`version`), `ui/package.json` (`version`).
- [ ] Release notes written; the in-app update flyout shows them (they come from the GitHub release body).
- [ ] Discord credits in the release notes: run `python3 -I tools/discord-issues/credits.py` (needs `GH_TOKEN`;
      `--since <previous tag>` when the previous release is not the latest published one) and paste its Markdown,
      which thanks the Discord users whose bug reports and ideas were closed since the previous release.
      The website changelog is built from the release notes, so the credits reach it too.
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

- [ ] Tag the release commit and push the tag; the release workflow builds and signs the artifacts.
- [ ] Download each artifact from the release page and smoke-test it: install, start, create a character, import a file.
- [ ] Confirm an older installed version shows the update notice for the new release.
