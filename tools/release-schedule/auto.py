#!/usr/bin/env python3
"""The weekly release, run by itself (decisions RL4, RL5, RL5a; B-164). One idempotent tick.

Every 15 minutes (the Cloudflare Worker in the private tools/discord-trigger dispatches the "Release automation"
workflow; a GitHub cron is the backup) a tick reads the planned releases (schedule.json), the milestones (plan.py)
and the live state (tags, CI runs, the draft release, App Store Connect, the website repo), and does what is due:

  prep    Sun 23:59 freeze: bump, commit "chore: release X.Y.Z", tag, push, start CI + acceptance + release.yml
          Mon 20:00 signed builds: check the draft is complete (in-app update files: from 1.4.1 latest.json and the
                   Windows/AppImage .sig are required; the macOS archive, and every file below 1.4.1, only warn)
          Tue 12:00 store submission: iOS version + build + What's New, submit for review; the signed Play bundle
                    to closed testing (alpha) for the testers (RL8, never production); notes to #mod-chat
  decide  Thu 18:00 go/no-go: approved + complete + not held, else no-go: a note and the release moves a week
  publish Thu 20:00 GitHub release, iOS release, website merge, IndexNow, Discord announcement and bridge replies

A note in #mod-chat (footer key "release-auto <version>/<name>") records each finished step, so nothing is done
or posted twice. RELEASE_HOLD (repository variable) holds a version, or "all". RELEASE_AUTO must be "on" for a
live run; otherwise the tick is a dry run. Tags older than decide.TAKEOVER were made by hand: those releases are
skipped. Usage: see `--help` and specs/release-schedule.md. Standard library only (Python 3.9+).
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
import traceback
from datetime import date, datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Callable, Optional, Sequence

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import asc as asc_api  # noqa: E402
import bump  # noqa: E402
import clients  # noqa: E402
import decide  # noqa: E402
import notes as notes_mod  # noqa: E402
import plan  # noqa: E402
import play as play_mod  # noqa: E402
import remind  # noqa: E402
import scenario  # noqa: E402
import website as website_mod  # noqa: E402

REPO = "TeamBorealis/borealis"
STEPS = ("auto", "plan", "freeze", "signed", "submit", "gonogo", "publish", "slip", "ios-supersede", "notes")
SCHEDULE_PATH = "tools/release-schedule/schedule.json"


def log(text: str) -> None:
    print(text, flush=True)


class Tick:
    def __init__(self, now: datetime, dry_run: bool, hold: str, gh: Optional[clients.GitHub],
                 git: Optional[clients.Git], discord: clients.Discord,
                 asc: Optional[asc_api.Asc], site: Optional[website_mod.Website], facts: Optional[dict] = None,
                 run_url: str = "", play: Optional[play_mod.Play] = None):
        self.now, self.dry_run, self.hold = now, dry_run, hold
        self.gh, self.git, self.discord, self.asc, self.site = gh, git, discord, asc, site
        self.play = play
        self.facts = facts or {}
        self.run_url = run_url
        self.releases: tuple[tuple[str, date], ...] = ()
        self._cache: dict[str, Any] = {}

    # --- facts (each can be overridden by a scenario file for dry runs) ---------------------------------------

    def fact(self, name: str, get: Callable[[], Any]) -> Any:
        if name in self.facts:
            return self.facts[name]
        if name not in self._cache:
            self._cache[name] = get()
        return self._cache[name]

    def tag(self, version: str) -> Optional[clients.Tag]:
        def get():
            return self.gh.tag(f"v{version}") if self.gh else None
        found = self.fact("tag", get)
        if isinstance(found, dict):  # from a scenario file
            return clients.Tag(f"v{version}", found["sha"], datetime.fromisoformat(found["date"]))
        return found

    def main_head(self) -> tuple[str, str]:
        def get():
            sha = self.git.fetch_main()
            conf = self.git.show("origin/main", "src-tauri/tauri.conf.json")
            return {"sha": sha, "version": json.loads(conf)["version"]}
        head = self.fact("main", get)
        return head["sha"], head["version"]

    def ci(self, workflow: str, sha: str) -> str:
        if f"ci:{workflow}" in self.facts:  # a scenario gives one status per workflow
            return self.facts[f"ci:{workflow}"]
        return self.fact(f"ci:{workflow}@{sha}", lambda: self.gh.ci(workflow, sha) if self.gh else "unknown")

    def release(self, version: str) -> Optional[dict]:
        return self.fact("release", lambda: self.gh.release(f"v{version}") if self.gh else None)

    def ios_state(self, version: str) -> Optional[str]:
        return self.fact("ios_state", lambda: self.asc.ios_state(version) if self.asc else "unknown (no key)")

    def website(self, version: str) -> str:
        return self.fact("website", lambda: self.site.check(version) if self.site else "no deploy key in this run")

    def notes(self, version: str) -> tuple[str, str]:
        """(notes text, source): the committed release-notes/<version>.md on main, else a generated draft."""
        def get():
            committed = self.git.show("origin/main", f"{notes_mod.NOTES_DIR}/{version}.md") if self.git else None
            if committed:
                return {"text": committed, "source": f"{notes_mod.NOTES_DIR}/{version}.md on main"}
            token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN") or ""
            return {"text": notes_mod.gather(version, None, "main", token),
                    "source": f"generated (no {notes_mod.NOTES_DIR}/{version}.md on main)"}
        found = self.fact("notes", get)
        return found["text"], found["source"]

    def updater_signing(self, version: str) -> bool:
        """Whether the release commit (the tag, else main's head) has updater signing on (decide.updater_signing)."""
        def get():
            tag = self.tag(version)
            rev = tag.sha if tag else "origin/main"
            conf = self.git.show(rev, decide.TAURI_CONF) if self.git else None
            if conf is None:
                log(f"{version}: cannot read {decide.TAURI_CONF} at {rev[:12]}; updater files not checked")
            return decide.updater_signing(conf)
        return bool(self.fact("updater_signing", get))

    def missing_updater(self, version: str, rel: Optional[dict]) -> list[str]:
        names = [a["name"] for a in (rel or {}).get("assets", [])]
        return decide.missing_updater_assets(version, names, self.updater_signing(version))

    def updater_warnings(self, version: str, rel: Optional[dict]) -> list[str]:
        missing = decide.warning_updater_assets(version, self.missing_updater(version, rel))
        warning = decide.updater_warning(missing)
        if warning:
            log(f"::warning::{version}: {warning}")
        return [warning] if warning else []

    # --- helpers ----------------------------------------------------------------------------------------------

    def note(self, version: str, name: str, title: str, lines: Sequence[str]) -> bool:
        tail = [f"[Run]({self.run_url})"] if self.run_url else []
        return self.discord.post(decide.Note(version, name, title, [*lines, *tail]))

    def posted(self, version: str, name: str) -> bool:
        return decide.key(version, name) in self.discord.posted()

    def is_held(self, version: str, publish: date) -> bool:
        if not decide.held(self.hold, version):
            return False
        log(f"{version}: on hold (RELEASE_HOLD={self.hold!r})")
        self.note(version, f"hold/{publish.isoformat()}", "on hold", [
            f"RELEASE_HOLD is `{self.hold}`: the automation does nothing for {version} until it is cleared. "
            "At go/no-go a held release is a no-go and moves a week."])
        return True

    def sync_draft(self, version: str, rel: Optional[dict]) -> None:
        """The draft's title and body follow the notes file (comments removed)."""
        if not rel or not rel.get("draft"):
            return
        text, _ = self.notes(version)
        body, name = notes_mod.strip_comments(text), f"Borealis {version}"
        if (rel.get("body") or "").strip() != body.strip() or rel.get("name") != name:
            self.gh.write("PATCH", f"/releases/{rel['id']}", {"body": body, "name": name},
                          f"set the v{version} draft's title and notes")

    # --- steps --------------------------------------------------------------------------------------------------

    def freeze(self, version: str, publish: date) -> None:
        tag = self.tag(version)
        if tag:
            log(f"{version}: freeze: done (tag v{version} on {tag.sha[:7]})")
            self.note(version, "frozen", "frozen", [f"Tag `v{version}` is on {tag.sha[:7]}."])
            return
        sha, main_version = self.main_head()
        d = decide.freeze_decision(main_version, version, self.ci("ci.yml", sha), False)
        log(f"{version}: freeze: {d.action} ({d.reason}); main {sha[:7]} at {main_version}")
        if d.action == "wait":
            return
        if d.action in ("red", "error"):
            self.note(version, f"freeze-blocked/{sha[:12]}", "freeze waits (no-go for now)", [
                f"{d.reason} on {sha[:7]}: the freeze does not tag a red main.",
                "It retries every 15 minutes until go/no-go. Fix main, or hold the release.",
                decide.hold_help(version)])
            return
        self.cut(version, d.action == "bump")
        tag_ref = f"v{version}"
        self.gh.dispatch("ci.yml", tag_ref)
        self.gh.dispatch("acceptance.yml", tag_ref)
        self.gh.dispatch("release.yml", tag_ref, {"tag": tag_ref})
        self.note(version, "frozen", "frozen", [
            f"`chore: release {version}` on {sha[:7]}, tag `{tag_ref}`. Started on the tag: CI (all platforms), "
            "import acceptance and the signed release builds (draft release, App Store upload).",
            f"Next: signed builds {decide.stamp(decide.milestone(publish, 'signed'))}, store submission "
            f"{decide.stamp(decide.milestone(publish, 'submitted'))}."])

    def cut(self, version: str, bump_first: bool) -> None:
        """Bump (when needed), commit, tag and push main + tag atomically, from a fresh worktree of origin/main."""
        if self.dry_run:
            if bump_first:
                texts = {f: self.git.show("origin/main", f) or "" for f in bump.FILES}
                _, current = self.main_head()
                changed = [f for f, t in bump.bump_texts(texts, current, version).items() if t != texts[f]]
                log(f"DRY-RUN would: commit \"chore: release {version}\" ({', '.join(changed)}), "
                    f"build number {bump.build_number(version)}")
            log(f"DRY-RUN would: tag v{version} (annotated \"Borealis {version}\") and push main + tag atomically")
            return
        self.git.fetch_main()
        wt = self.git.worktree()
        try:
            if bump_first:
                changed = bump.bump_tree(wt, version)
                self.git.run("add", "--", *changed, cwd=wt)
                self.git.run("commit", "--quiet", "-m", f"chore: release {version}", cwd=wt)
            self.git.run("tag", "-a", f"v{version}", "-m", f"Borealis {version}", cwd=wt)
            self.git.push(wt, "HEAD:refs/heads/main", f"refs/tags/v{version}")
        finally:
            self.git.drop_worktree(wt)

    def signed(self, version: str, publish: date) -> None:
        tag, rel = self.tag(version), self.release(version)
        missing = decide.missing_assets(version, [a["name"] for a in (rel or {}).get("assets", [])])
        if rel:  # from 1.4.1 the update feed and the Windows/AppImage signatures are part of a complete draft (B-215)
            missing = missing + decide.blocking_updater_assets(version, self.missing_updater(version, rel))
        if tag and rel and not missing:
            self.sync_draft(version, rel)
            installers = len(decide.expected_assets(version))
            updater = (f"; with the in-app update files, {installers + len(decide.updater_assets(version))} files"
                       if self.updater_signing(version) else "")
            self.note(version, "signed", "signed builds ready", [
                f"The v{version} draft has every installer ({installers}){updater}.",
                *[f"**Warning:** {w}" for w in self.updater_warnings(version, rel)],
                f"Store submission {decide.stamp(decide.milestone(publish, 'submitted'))}."])
            return
        problem = ("not frozen yet" if not tag else "no draft release yet" if not rel
                   else f"the draft lacks {', '.join(missing)}")
        log(f"{version}: signed builds not ready: {problem}")
        if tag and self.retry_release(version, tag.sha):
            return
        run = self.ci("release.yml", tag.sha) if tag else "n/a"
        self.note(version, "signed-late", "signed builds not ready", [
            f"{problem} (release workflow on the tag: {run}).",
            "Re-run the failed jobs of the release workflow, or run it by hand with the tag. "
            "Go/no-go needs a complete draft."])

    def retry_release(self, version: str, sha: str) -> bool:
        """True when the release workflow is still running or was just re-run, so it is not late yet."""
        runs = self.facts["release_runs"] if "release_runs" in self.facts else self.fact(  # a scenario gives them
            f"release_runs@{sha}", lambda: self.gh.runs("release.yml", sha) if self.gh else [])
        choice, run = decide.signed_rerun(runs), decide.newest(runs)
        if choice == "wait":
            log(f"{version}: release workflow run {run['id']} is still running; checking again next tick")
            return True
        if choice != "rerun" or not self.gh:
            return False
        attempt = int(run.get("run_attempt") or 1) + 1
        self.gh.rerun_failed("release.yml", run["id"])
        self.note(version, f"signed-rerun/{attempt}", "re-running the release workflow", [
            f"Run {run['id']} ended `{run.get('conclusion')}`, so its failed jobs run again "
            f"(attempt {attempt} of {decide.RELEASE_ATTEMPTS}). GitHub cancels jobs no runner picks up."])
        return True

    def submit(self, version: str, publish: date) -> None:
        tag = self.tag(version)
        if not tag:
            self.note(version, "submit-wait", "store submission waits", ["Not frozen yet: no tag."])
            return
        rel = self.release(version)
        self.sync_draft(version, rel)
        text, source = self.notes(version)
        findings = notes_mod.cs1_findings(text)
        ios = self.submit_ios(version, text)
        site = self.prepare_site(version)
        android = self.submit_play(version, publish, tag, text)
        link = (rel or {}).get("html_url") or f"https://github.com/{REPO}/releases"
        sections = notes_mod.sections(text)
        counts = ", ".join(f"{h}: {len(notes_mod.bullets(ls))}" for h, ls in sections.items()
                           if h.lower().startswith(("new in", "fixed in")))
        self.note(version, "notes", "release notes for approval", [
            f"**Draft:** {link}",
            f"**Notes:** {source}{' (' + counts + ')' if counts else ''}",
            "", notes_mod.summary(text)[:900], "",
            f"**CS1:** {'; '.join(findings) if findings else 'clean'}",
            f"**iOS:** {ios}", f"**Website:** {site}", f"**Play (closed testing):** {android}",
            f"Publishes {decide.stamp(plan.milestone_at(publish, plan.MILESTONES[-2]))} if the App Store approves "
            "by go/no-go. **Silence = approval.**", decide.hold_help(version)])

    def submit_ios(self, version: str, text: str) -> str:
        if not self.asc:
            return "no App Store Connect key in this run"
        number = bump.build_number(version)
        state = self.ios_state(version)
        if state in decide.IOS_IN_REVIEW | decide.IOS_APPROVED | decide.IOS_RELEASED:
            self.note(version, "ios-submitted", "iOS in review", [f"iOS {version}: {state}."])
            return state
        build = self.asc.build(version)
        if not build:
            msg = f"waiting for an iOS {version} build (normally {number}) to be processed in App Store Connect"
            self.note(version, "ios-wait", "iOS waits for its build", [msg])
            return msg
        choice = asc_api.choose_version(self.asc.versions(), version)
        if choice.action == "blocked":
            return self.ios_blocked(version, choice)
        vid, did = self.asc.ensure_version(version)
        log(f"{version}: iOS: {did}")
        self.asc.attach_build(vid, build)
        self.asc.set_whats_new(vid, notes_mod.whats_new(text))
        self.asc.submit(vid)
        got = build["attributes"].get("version", number)
        self.note(version, "ios-submitted", "iOS submitted for review",
                  [f"iOS {version} with build {got}, manual release."])
        return f"submitted for review (build {got})"

    def ios_blocked(self, version: str, choice: asc_api.VersionChoice) -> str:
        """Another iOS version is in review or approved. One the automation folded (no longer scheduled, tag made
        after the takeover) is withdrawn; anything else waits for a person (step ios-supersede, or App Store
        Connect)."""
        old = choice.version["attributes"].get("versionString", "")
        old_tag = self.gh.tag(f"v{old}") if self.gh else None
        scheduled = {v for v, _ in self.releases}
        if decide.auto_supersede(old, scheduled, old_tag.date if old_tag else None,
                                 asc_api.state_of(choice.version)):
            did = self.asc.supersede(version)
            self.note(version, f"ios-superseded/{old}", "folded iOS version withdrawn", [did])
            return f"{did}; submitting on the next tick"
        self.note(version, f"ios-blocked/{old}", "iOS submission blocked", [choice.reason,
                  "If that version is folded into this one: in App Store Connect open it and reject the binary "
                  "(approved) or remove it from review, or run the Release automation workflow with step "
                  "ios-supersede (review only). The submission retries every 15 minutes."])
        return choice.reason

    def submit_play(self, version: str, publish: date, tag: clients.Tag, text: str) -> str:
        """RL8: the signed bundle release.yml built from the tag goes to closed testing (alpha), rolled out to the
        testers. Never production. A failure here never stops the iOS submission or the notes; the next tick
        retries until go/no-go."""
        if self.posted(version, "play-uploaded"):
            return f"on {play_mod.TRACK}"
        if not self.play:
            msg = ("no Play service account in this run (secret PLAY_SERVICE_ACCOUNT_JSON): upload the bundle by "
                   "hand, or add the secret and run step submit")
            self.note(version, "play-manual", "Play needs a hand", [msg])
            return msg
        name = play_mod.artifact_name(version, tag.sha)
        try:
            # Testers see Play's notes at once, so notes with a CS1 finding are replaced by a bare line.
            clean = not notes_mod.cs1_findings(text)
            notes = play_mod.release_notes(notes_mod.whats_new(text) if clean else f"Borealis {version}.")
            with tempfile.TemporaryDirectory() as tmp:
                aab = self.play_bundle(version, name, Path(tmp))
                if not aab:
                    msg = f"waiting for the signed bundle (release.yml artifact `{name}`)"
                    self.note(version, "play-wait", "Play waits for its bundle", [msg])
                    return msg
                did = self.play.release_to_testers(aab, version, bump.build_number(version), notes)
        except Exception as err:  # noqa: BLE001 - reported once, retried by the next tick
            traceback.print_exc()
            log(f"::error::{version} Play: {err}")
            self.note(version, f"play-error/{publish.isoformat()}", "Play upload failed", [
                f"`{type(err).__name__}: {str(err)[:500]}`",
                "The next tick retries until go/no-go. Fallback by hand: the release workflow's "
                f"`{name}` artifact, uploaded in Play Console (Test and release > Closed testing > Alpha)."])
            return f"failed ({type(err).__name__}); retrying"
        log(f"{version}: Play: {did}")
        self.note(version, "play-uploaded", "Android on closed testing", [f"Android {version}: {did}."])
        return did

    def play_bundle(self, version: str, name: str, dest: Path) -> Optional[Path]:
        """The signed bundle from the release workflow's artifact `name`, downloaded into `dest`; None while there
        is none. A scenario gives "play_artifact" (an artifact or null) and "play_bundle" (a path) instead."""
        art = self.facts["play_artifact"] if "play_artifact" in self.facts else (
            self.gh.artifact(name) if self.gh else None)
        if not art:
            return None
        if "play_bundle" in self.facts:
            return Path(self.facts["play_bundle"])
        return self.gh.download_artifact(art, play_mod.bundle_file(version), dest)

    def prepare_site(self, version: str) -> str:
        if not self.site:
            return "no deploy key in this run"
        try:
            return self.site.ensure_branch(version)
        except RuntimeError as err:
            return f"could not prepare the release branch: {err}"

    def go_facts(self, version: str) -> decide.GoFacts:
        tag, rel = self.tag(version), self.release(version)
        text, _ = self.notes(version)
        return decide.GoFacts(
            version=version, held=decide.held(self.hold, version), tag=tag is not None,
            release=None if not rel else ("draft" if rel.get("draft") else "published"),
            assets=tuple(a["name"] for a in (rel or {}).get("assets", [])),
            tag_ci=self.ci("ci.yml", tag.sha) if tag else "none",
            tag_acceptance=self.ci("acceptance.yml", tag.sha) if tag else "none",
            ios_state=self.ios_state(version), notes_findings=tuple(notes_mod.cs1_findings(text)),
            website=self.website(version), updater=self.updater_signing(version))

    def go_no_go(self, version: str, publish: date, current: str, forced: bool = False) -> int:
        rel = self.release(version)
        if rel and not rel.get("draft"):
            if decide.held(self.hold, version):
                return int(not self.is_held(version, publish))
            return self.publish(version, publish)  # already partly published: finish, never slip
        facts = self.go_facts(version)
        problems, warnings = decide.go_problems(facts), decide.go_warnings(facts)
        log(f"{version}: go/no-go: {'GO' if not problems else 'NO-GO: ' + '; '.join(problems)}")
        for warning in warnings:
            log(f"::warning::{version}: {warning}")
        if problems:
            if forced:
                return 1
            return self.slip(version, publish, problems, warnings)
        if current == "decide" and not forced:
            self.note(version, f"go/{publish.isoformat()}", "go", [
                f"Approved on the App Store, draft complete, CI green. Publishing "
                f"{decide.stamp(decide.milestone(publish, 'publish'))}.",
                *[f"**Warning:** {w}" for w in warnings], decide.hold_help(version)])
            return 0
        return self.publish(version, publish, warnings) if current == "publish" else 0

    def slip(self, version: str, publish: date, problems: Sequence[str], warnings: Sequence[str] = ()) -> int:
        def change(wt: Path) -> list[str]:
            path = wt / SCHEDULE_PATH
            new, _ = decide.slip(plan.load_schedule(path), version, publish)
            plan.save_schedule(new, path)
            return [SCHEDULE_PATH]
        new, folded = decide.slip(self.releases, version, publish)
        target = publish + timedelta(days=7)
        message = (f"chore: fold Borealis {version} into {folded} (no-go)" if folded
                   else f"chore: move Borealis {version} to {target.isoformat()} (no-go)")
        sha = self.git.commit_to_main(message, change) if self.git else "no checkout"
        outcome = (f"{version} folds into {folded}: it is not published; {folded} carries its changes."
                   if folded else f"It moves to Thursday {target.isoformat()} "
                   f"({decide.stamp(plan.milestone_at(target, plan.MILESTONES[-2]))}).")
        self.note(version, f"nogo/{publish.isoformat()}", "no-go", [
            *[f"- {p}" for p in problems], *[f"- **Warning (not blocking):** {w}" for w in warnings], "", outcome,
            f"schedule.json: {sha[:7]}"])
        return 0

    def publish(self, version: str, publish: date, warnings: Sequence[str] = ()) -> int:
        if self.posted(version, "published"):
            log(f"{version}: already published")
            return 0
        errors, done = [], []
        steps = (("GitHub", self.publish_github), ("App Store", self.publish_ios), ("website", self.publish_site),
                 ("Discord", self.publish_discord))
        for name, step in steps:
            try:
                done.append(f"**{name}:** {step(version)}")
            except Exception as err:  # noqa: BLE001 - every part is retried by the next tick
                errors.append(f"{name}: {err}")
                log(f"::error::{version}: publish {name}: {err}")
                traceback.print_exc()
        if errors:
            self.note(version, f"error/publish/{publish.isoformat()}", "publish incomplete", [
                *[f"- {e}" for e in errors], "", *done, "The next tick retries what is left."])
            return 1
        self.note(version, "published", "published", [*done, *[f"**Warning:** {w}" for w in warnings],
                  "**Board:** move the items with Release " + version + " to Done (orchestrator)."])
        return 0

    def publish_github(self, version: str) -> str:
        rel = self.release(version)
        if not rel:
            raise RuntimeError("no release")
        if not rel.get("draft"):
            return f"published ({rel.get('html_url')})"
        text, _ = self.notes(version)
        self.gh.write("PATCH", f"/releases/{rel['id']}", {
            "body": notes_mod.strip_comments(text), "name": f"Borealis {version}", "draft": False,
            "make_latest": "true"}, f"publish v{version} as the latest release")
        return f"published https://github.com/{REPO}/releases/tag/v{version}"

    def publish_ios(self, version: str) -> str:
        if not self.asc:
            raise RuntimeError("no App Store Connect key in this run")
        v = self.asc.version(version)
        state = asc_api.state_of(v) if v else None
        if state in decide.IOS_RELEASED:
            return f"released ({state})"
        if state not in decide.IOS_APPROVED:
            raise RuntimeError(f"iOS {version} is {state}, not approved")
        self.asc.release(v["id"], version)
        return "released"

    def publish_site(self, version: str) -> str:
        if not self.site:
            raise RuntimeError("no website deploy key in this run")
        merged = self.site.publish(version)
        if not self.site.wait_live(version):
            raise RuntimeError(f"{merged}, but {website_mod.SITE} does not serve {version} yet (Vercel deploy?)")
        return f"{merged}; live; {self.site.indexnow()}"

    def publish_discord(self, version: str) -> str:
        self.gh.dispatch("discord-issues.yml", "main", {"announce_tag": f"v{version}"})
        return "announcement and \"Released in\" replies started (discord-issues.yml)"

    def supersede(self, version: str) -> int:
        if not self.asc:
            log("no App Store Connect key in this run")
            return 2
        log(f"{version}: ios-supersede: {self.asc.supersede(version)}")
        return 0

    # --- the tick ----------------------------------------------------------------------------------------------

    def run_release(self, version: str, publish: date, current: str) -> int:
        tag = self.tag(version)
        if tag and decide.handmade(tag.date):
            log(f"{version}: tag v{version} dates from {tag.date.isoformat()}, before the automation took over "
                f"({decide.TAKEOVER.isoformat()}): this release is handled by hand; skipped")
            return 0
        log(f"{version} (publish {publish.isoformat()}): {current}")
        if current == "prep":
            if self.is_held(version, publish):
                return 0
            self.freeze(version, publish)
            if self.now >= decide.milestone(publish, "signed") and not self.posted(version, "signed"):
                self.signed(version, publish)
            if self.now >= decide.milestone(publish, "submitted") and not decide.submitted(
                    version, self.discord.posted()):
                self.submit(version, publish)
            return 0
        if self.posted(version, f"nogo/{publish.isoformat()}"):
            return 0
        if current == "decide" and self.posted(version, f"go/{publish.isoformat()}"):
            return 0
        return self.go_no_go(version, publish, current)

    def automated(self, releases: Sequence[tuple[str, date]]) -> list[tuple[str, date]]:
        """The releases the automation may act on: those due or upcoming whose tag (if any) it did not inherit
        from the hand-made era. Only active or upcoming releases are looked up."""
        out = []
        for version, publish in releases:
            if decide.milestone(publish, "publish") + decide.PUBLISH_GRACE <= self.now:
                continue
            if decide.phase(self.now, publish) or decide.milestone(publish, "freeze") > self.now:
                found = self.gh.tag(f"v{version}") if self.gh and "tag" not in self.facts else self.tag(version)
                if found and decide.handmade(found.date):
                    log(f"{version}: tagged by hand before the takeover; left alone")
                    continue
            out.append((version, publish))
        return out

    def plan(self, releases: Sequence[tuple[str, date]]) -> decide.Needs:
        due = [r for r in releases if decide.phase(self.now, r[1])]
        n = decide.needs(self.now, self.automated(due), self.discord.posted())
        log(f"plan: run={n.run} env={n.env} ({n.why})")
        return n

    def run(self, releases: Sequence[tuple[str, date]], step: str = "auto", only: Optional[str] = None) -> int:
        self.releases = tuple(releases)
        if step == "plan":
            self.plan(releases)
            return 0
        if step == "auto":
            todo = [r for r in decide.active(self.now, releases) if not only or r[0] == only]
            if not todo:
                log("nothing due")
            status = 0
            for version, publish, current in todo:
                status |= self.guarded(version, publish, current, lambda: self.run_release(version, publish, current))
            return status
        candidates = releases if (only and step == "notes") else self.automated(releases)
        version, publish = pick(candidates, only, self.now)
        actions: dict[str, Callable[[], int]] = {
            "freeze": lambda: self.freeze(version, publish) or 0,
            "signed": lambda: self.signed(version, publish) or 0,
            "submit": lambda: self.submit(version, publish) or 0,
            "gonogo": lambda: self.go_no_go(version, publish, "decide", forced=True),
            "publish": lambda: self.go_no_go(version, publish, "publish", forced=True),
            "slip": lambda: self.slip(version, publish, ["moved by hand (step slip)"]),
            "ios-supersede": lambda: self.supersede(version),
            "notes": lambda: self.show_notes(version),
        }
        if step not in ("notes", "slip") and self.is_held(version, publish):
            return 0
        return self.guarded(version, publish, step, actions[step])

    def guarded(self, version: str, publish: date, what: str, action: Callable[[], int]) -> int:
        try:
            return action() or 0
        except Exception as err:  # noqa: BLE001 - report, then let the next tick retry
            traceback.print_exc()
            log(f"::error::{version} {what}: {err}")
            self.note(version, f"error/{what}/{publish.isoformat()}", f"{what} failed", [
                f"`{type(err).__name__}: {str(err)[:500]}`", "The next tick retries."])
            return 1

    def show_notes(self, version: str) -> int:
        text, source = self.notes(version)
        log(f"--- notes ({source}) ---\n{notes_mod.strip_comments(text)}--- What's New ---\n"
            f"{notes_mod.whats_new(text)}\n--- CS1: {notes_mod.cs1_findings(text) or 'clean'}")
        return 0


def pick(releases: Sequence[tuple[str, date]], only: Optional[str], now: datetime) -> tuple[str, date]:
    """The release a manual step works on: --version, else the next one not yet past its grace period."""
    if only:
        found = [r for r in releases if r[0] == only]
        if not found:
            raise SystemExit(f"error: {only} is not in schedule.json, or was tagged by hand")
        return found[0]
    upcoming = [r for r in releases if decide.milestone(r[1], "publish") + decide.PUBLISH_GRACE > now]
    if not upcoming:
        raise SystemExit("error: no upcoming release in schedule.json")
    return upcoming[0]


def identity() -> tuple[str, str]:
    name, email = os.environ.get("RELEASE_GIT_NAME", "").strip(), os.environ.get("RELEASE_GIT_EMAIL", "").strip()
    if name and email:
        return name, email
    import subprocess  # local runs: git's configured author
    get = lambda k: subprocess.run(["git", "config", k], capture_output=True, text=True).stdout.strip()  # noqa: E731
    return get("user.name"), get("user.email")


def main(argv: Optional[Sequence[str]] = None) -> int:
    p = argparse.ArgumentParser(description="Borealis release automation: one tick (see the module docstring)")
    p.add_argument("--dry-run", action="store_true", help="log what would be done; touch nothing")
    p.add_argument("--now", help="pretend it is this time (ISO 8601 with a zone)")
    p.add_argument("--step", choices=STEPS, default="auto", help="run one step by hand, whatever the time")
    p.add_argument("--version", help="only this release")
    p.add_argument("--scenario", type=Path, help="JSON file of facts that replace the live state (dry runs)")
    p.add_argument("--asc-config", type=Path, help="local App Store Connect config.env (instead of ASC_* vars)")
    p.add_argument("--github-output", help="with --step plan: write run/env to this $GITHUB_OUTPUT file")
    args = p.parse_args(argv)

    facts = json.loads(args.scenario.read_text(encoding="utf-8")) if args.scenario else {}
    when = args.now or facts.get("now")
    now = remind.parse_now(when) if when else datetime.now(timezone.utc)
    dry_run = args.dry_run
    if not dry_run and os.environ.get("RELEASE_AUTO", "").strip().lower() != "on":
        log("RELEASE_AUTO is not 'on': this tick is a dry run")
        dry_run = True
    if (args.now or args.scenario) and not dry_run:
        print("error: --now and --scenario are only for dry runs", file=sys.stderr)
        return 2
    token = (os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN") or "").strip()
    discord_token = os.environ.get("DISCORD_BOT_TOKEN", "").strip() or None
    if not dry_run and not (token and discord_token):
        print("error: GITHUB_TOKEN and DISCORD_BOT_TOKEN must be set", file=sys.stderr)
        return 2
    who = identity()
    if not dry_run and not all(who):
        print("error: RELEASE_GIT_NAME and RELEASE_GIT_EMAIL must be set", file=sys.stderr)
        return 2
    posted = frozenset(facts["posted"]) if "posted" in facts else None
    discord = clients.Discord(discord_token, dry_run, log, posted)
    asc = (asc_api.Asc.from_config(args.asc_config, dry_run, log) if args.asc_config
           else asc_api.Asc.from_env(dry_run, log))
    site = website_mod.Website(dry_run, who, log) if website_mod.Website.available() else None
    play = play_mod.Play.from_env(dry_run, log)
    if "asc" in facts:
        asc = scenario.FakeAsc(facts["asc"], log)
    if "play" in facts:  # null: a run without the Play key
        play = scenario.FakePlay(facts["play"], log) if facts["play"] is not None else None
    if "website_repo" in facts:
        site = scenario.FakeSite(facts["website_repo"], log)
    run_url = (f"{os.environ['GITHUB_SERVER_URL']}/{os.environ['GITHUB_REPOSITORY']}/actions/runs/"
               f"{os.environ['GITHUB_RUN_ID']}") if os.environ.get("GITHUB_RUN_ID") else ""
    tick = Tick(now, dry_run, facts.get("hold", os.environ.get("RELEASE_HOLD", "")),
                clients.GitHub(token, REPO, dry_run, log),
                clients.Git(clients.ROOT, who, dry_run, log), discord, asc, site, facts, run_url, play)
    releases = plan.load_schedule()
    log(f"release tick at {now.astimezone(plan.LOCAL_TZ).isoformat(timespec='minutes')} Israel"
        f"{' (dry run)' if dry_run else ''}; hold={tick.hold!r}; App Store key: {'yes' if asc else 'no'}; "
        f"Play key: {'yes' if play else 'no'}; website key: {'yes' if site else 'no'}")
    if args.step == "plan":
        n = tick.plan(releases)
        if args.github_output:
            with open(args.github_output, "a", encoding="utf-8") as out:
                out.write(f"run={'true' if n.run else 'false'}\nenv={'true' if n.env else 'false'}\n")
        return 0
    return tick.run(releases, args.step, args.version)


if __name__ == "__main__":
    sys.exit(main())
