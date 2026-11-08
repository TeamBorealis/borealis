"""Pure decisions of the release automation (decisions RL4, RL5, RL5a; B-164). No network, no files.

auto.py gathers the live state (tags, CI runs, the draft release, App Store Connect, the website) and calls these
functions to decide what a tick does; every rule that can be tested without the network lives here.
Standard library only (Python 3.9+).
"""
from __future__ import annotations

import base64
import json
from dataclasses import dataclass, field
from datetime import date, datetime, timedelta
from typing import Optional, Sequence

import plan

# Tags dated before this were made by hand (1.2.2 and earlier): the automation leaves those releases alone.
TAKEOVER = datetime(2026, 11, 5, 20, 0, tzinfo=plan.LOCAL_TZ)
# After the publish time the tick keeps finishing an incomplete publish for this long, then the release is over.
PUBLISH_GRACE = timedelta(days=1)
KEY_PREFIX = "release-auto"
HOLD_ALL = {"all", "*"}

# App Store states (appStoreState, with the newer appVersionState names).
IOS_EDITABLE = frozenset({"PREPARE_FOR_SUBMISSION", "DEVELOPER_REJECTED", "REJECTED", "METADATA_REJECTED",
                          "INVALID_BINARY"})
IOS_IN_REVIEW = frozenset({"WAITING_FOR_REVIEW", "IN_REVIEW", "WAITING_FOR_EXPORT_COMPLIANCE", "READY_FOR_REVIEW"})
IOS_APPROVED = frozenset({"PENDING_DEVELOPER_RELEASE"})
IOS_RELEASED = frozenset({"READY_FOR_SALE", "READY_FOR_DISTRIBUTION", "PROCESSING_FOR_APP_STORE",
                          "PROCESSING_FOR_DISTRIBUTION", "PENDING_APPLE_RELEASE", "REPLACED_WITH_NEW_VERSION"})


def milestone(publish: date, key: str) -> datetime:
    return plan.milestone_at(publish, next(m for m in plan.MILESTONES if m.key == key))


def phase(now: datetime, publish: date) -> Optional[str]:
    """'prep' (freeze .. go/no-go), 'decide' (go/no-go .. publish), 'publish' (publish .. +grace), else None."""
    if milestone(publish, "freeze") <= now < milestone(publish, "gonogo"):
        return "prep"
    if milestone(publish, "gonogo") <= now < milestone(publish, "publish"):
        return "decide"
    if milestone(publish, "publish") <= now < milestone(publish, "publish") + PUBLISH_GRACE:
        return "publish"
    return None


def active(now: datetime, releases: Sequence[tuple[str, date]]) -> list[tuple[str, date, str]]:
    """The scheduled releases a tick at `now` works on, with their phase."""
    out = []
    for version, publish in releases:
        p = phase(now, publish)
        if p:
            out.append((version, publish, p))
    return out


def held(hold: str, version: str) -> bool:
    """RELEASE_HOLD holds `version` when it lists it (comma or space separated, with or without 'v') or says all."""
    tokens = {t.strip().lower().lstrip("v") for t in hold.replace(",", " ").split() if t.strip()}
    return bool(tokens & HOLD_ALL) or version in tokens


def handmade(tag_date: Optional[datetime]) -> bool:
    """A tag older than the takeover was made by hand: the automation skips that release entirely."""
    return tag_date is not None and tag_date < TAKEOVER


def auto_supersede(old: str, scheduled: set[str], old_tag_date: Optional[datetime], old_state: str) -> bool:
    """Whether a tick may withdraw another iOS version's review to submit this one: only a version the automation
    itself folded (no longer in schedule.json, its tag made after the takeover or never made) and only while it
    is in review (Apple's API cannot withdraw an approved version)."""
    return (old not in scheduled and not handmade(old_tag_date) and old_state in IOS_IN_REVIEW)


RELEASE_ATTEMPTS = 3  # a release workflow run is tried at most this often (the first run plus two re-runs)
RERUN_CONCLUSIONS = frozenset({"failure", "cancelled", "timed_out", "startup_failure"})


def newest(runs: Sequence[dict]) -> Optional[dict]:
    return max(runs, key=lambda r: r.get("created_at", "")) if runs else None


def signed_rerun(runs: Sequence[dict]) -> str:
    """What Monday does about an incomplete draft, from the release workflow's runs on the tag: 'rerun' its failed
    jobs (GitHub cancels jobs no runner picked up, B-168), 'wait' while it runs, else 'late' (tell a person)."""
    run = newest(runs)
    if run is None:
        return "late"
    if run.get("status") != "completed":
        return "wait"
    if run.get("conclusion") in RERUN_CONCLUSIONS and int(run.get("run_attempt") or 1) < RELEASE_ATTEMPTS:
        return "rerun"
    return "late"


def ci_status(runs: Sequence[dict]) -> str:
    """'success' when any run of the workflow passed for the commit, 'pending' while one runs, 'none' without
    runs, else the newest run's conclusion (failure, cancelled, timed_out...)."""
    if any(r.get("conclusion") == "success" for r in runs):
        return "success"
    if any(r.get("status") != "completed" for r in runs):
        return "pending"
    if not runs:
        return "none"
    newest = max(runs, key=lambda r: r.get("created_at", ""))
    return newest.get("conclusion") or "unknown"


@dataclass(frozen=True)
class FreezeDecision:
    action: str  # done | bump | tag | wait | red | error
    reason: str


def freeze_decision(main_version: str, version: str, main_ci: str, tag_exists: bool) -> FreezeDecision:
    if tag_exists:
        return FreezeDecision("done", f"tag v{version} exists")
    if plan.parse_version(main_version) > plan.parse_version(version):
        return FreezeDecision("error", f"main is already at {main_version}, past {version}")
    if main_ci == "pending":
        return FreezeDecision("wait", "CI is still running on main")
    if main_ci != "success":
        return FreezeDecision("red", f"CI on main is {main_ci}")
    if main_version == version:
        return FreezeDecision("tag", f"main is already at {version}: tag it")
    return FreezeDecision("bump", f"bump {main_version} -> {version}, commit and tag")


def expected_assets(version: str) -> tuple[str, ...]:
    """Every file a complete draft carries (the website links to these names)."""
    v = version
    return (f"Borealis_{v}_aarch64.dmg", f"Borealis_{v}_x64-setup.exe", f"Borealis_{v}_amd64.AppImage",
            f"Borealis_{v}_amd64.deb", f"Borealis-{v}-1.x86_64.rpm", f"Borealis_{v}_arm64.apk", "SHA256SUMS.txt")


def missing_assets(version: str, names: Sequence[str]) -> list[str]:
    have = set(names)
    return [n for n in expected_assets(version) if n not in have]


# In-app updates (#25, UP2; build/ci/updater.sh). The release workflow makes these files only when updater signing is
# on: the `release` environment has TAURI_SIGNING_PRIVATE_KEY and plugins.updater.pubkey is a real minisign key.
TAURI_CONF = "src-tauri/tauri.conf.json"
MINISIGN_PUBKEY_HEADER = "untrusted comment: minisign public key"


def updater_signing(tauri_conf: Optional[str]) -> bool:
    """Whether the release commit turns updater signing on, from its tauri.conf.json text: the same test as
    `updater.sh enabled` (plugins.updater.pubkey base64-decodes to a minisign public key, not the placeholder).
    The secret itself is not visible here; without it the workflow skips the files and the check below warns."""
    if not tauri_conf:
        return False
    try:
        pubkey = ((json.loads(tauri_conf).get("plugins") or {}).get("updater") or {}).get("pubkey") or ""
        decoded = base64.b64decode(pubkey, validate=True).decode("utf-8")
    except (ValueError, AttributeError, TypeError):  # bad JSON, not an object, not a base64 string, not UTF-8
        return False
    return any(line.startswith(MINISIGN_PUBKEY_HEADER) for line in decoded.splitlines())


def updater_assets(version: str) -> tuple[str, ...]:
    """The in-app update feed and the signed files it points to (Windows NSIS, the AppImage, the macOS archive)."""
    v = version
    return ("latest.json", f"Borealis_{v}_x64-setup.exe.sig", f"Borealis_{v}_amd64.AppImage.sig",
            f"Borealis_{v}_aarch64.app.tar.gz", f"Borealis_{v}_aarch64.app.tar.gz.sig")


def missing_updater_assets(version: str, names: Sequence[str], signing: bool) -> list[str]:
    """Updater files the draft lacks; none are expected while updater signing is off."""
    if not signing:
        return []
    have = set(names)
    return [n for n in updater_assets(version) if n not in have]


# B-215 (decision FU1): from this version a draft without the update feed or the Windows/AppImage signatures cannot
# publish, since copies could not update to it from inside Borealis. The macOS archive (and its .sig) joins the draft
# after the rest (release.yml attach-macos), so it stays a warning at every version, as does everything below this one.
UPDATER_REQUIRED_FROM = "1.5.0"


def required_updater_assets(version: str) -> tuple[str, ...]:
    """The updater files whose absence blocks this version: the feed and the Windows and AppImage signatures from
    UPDATER_REQUIRED_FROM, none before."""
    if plan.parse_version(version) < plan.parse_version(UPDATER_REQUIRED_FROM):
        return ()
    v = version
    return ("latest.json", f"Borealis_{v}_x64-setup.exe.sig", f"Borealis_{v}_amd64.AppImage.sig")


def blocking_updater_assets(version: str, missing: Sequence[str]) -> list[str]:
    """The missing updater files that block this version (the rest of `missing` only warns)."""
    required = set(required_updater_assets(version))
    return [n for n in missing if n in required]


def warning_updater_assets(version: str, missing: Sequence[str]) -> list[str]:
    """The missing updater files that are only reported for this version."""
    required = set(required_updater_assets(version))
    return [n for n in missing if n not in required]


def updater_problem(version: str, blocking: Sequence[str]) -> Optional[str]:
    """Why missing required updater files stop the release (B-215)."""
    if not blocking:
        return None
    return (f"in-app updates: the draft lacks {', '.join(blocking)} (required from {UPDATER_REQUIRED_FROM}): copies "
            f"could not install {version} from inside Borealis (release.yml publish-draft / `updater.sh feed`)")


def updater_warning(missing: Sequence[str]) -> Optional[str]:
    """A missing updater file that is not required (warning_updater_assets) never blocks the release: the installers
    still download by hand, and the macOS archive joins the draft after the rest. It is reported instead."""
    if not missing:
        return None
    return (f"in-app updates: the draft lacks {', '.join(missing)}. Not blocking: the installers still download by "
            "hand, but copies cannot install this version from inside Borealis until the files are attached "
            "(release.yml attach-macos / `updater.sh feed`).")


@dataclass(frozen=True)
class GoFacts:
    version: str
    held: bool = False
    tag: bool = False
    release: Optional[str] = None  # None (no release), "draft" or "published"
    assets: tuple[str, ...] = ()
    tag_ci: str = "none"
    tag_acceptance: str = "none"
    ios_state: Optional[str] = None
    notes_findings: tuple[str, ...] = ()
    website: str = "unknown"  # "ready" or what is wrong
    updater: bool = False  # updater signing is on for the release commit (updater_signing)


def go_warnings(f: GoFacts) -> list[str]:
    """What the go/no-go summary reports without blocking the release."""
    if f.release is None:
        return []
    missing = missing_updater_assets(f.version, f.assets, f.updater)
    warning = updater_warning(warning_updater_assets(f.version, missing))
    return [warning] if warning else []


def go_problems(f: GoFacts) -> list[str]:
    """Why the release can't publish; empty means go (RL4 go/no-go, RL5a)."""
    problems = []
    if f.held:
        problems.append("on hold (RELEASE_HOLD)")
    if not f.tag:
        problems.append(f"not frozen: no tag v{f.version}")
    if f.release is None:
        problems.append("no draft release")
    else:
        missing = missing_assets(f.version, f.assets)
        if missing:
            problems.append(f"the draft lacks {', '.join(missing)}")
        updater = updater_problem(f.version, blocking_updater_assets(
            f.version, missing_updater_assets(f.version, f.assets, f.updater)))
        if updater:
            problems.append(updater)
    if f.tag_ci != "success":
        problems.append(f"CI on the release commit: {f.tag_ci}")
    if f.tag_acceptance != "success":
        problems.append(f"import acceptance on the release commit: {f.tag_acceptance}")
    if f.ios_state not in IOS_APPROVED | IOS_RELEASED:
        problems.append(f"App Store: {f.ios_state or 'no iOS version'} (needs PENDING_DEVELOPER_RELEASE)")
    if f.notes_findings:
        problems.append(f"release notes break CS1: {'; '.join(f.notes_findings)}")
    if f.website != "ready":
        problems.append(f"website: {f.website}")
    return problems


def slip(releases: Sequence[tuple[str, date]], version: str, publish: date
         ) -> tuple[tuple[tuple[str, date], ...], Optional[str]]:
    """The schedule after a no-go: the release moves a week. When another version is already planned for that
    Thursday, the slipped one folds into it (RL5a 4) and is dropped. Returns (schedule, folded_into)."""
    target = publish + timedelta(days=7)
    other = sorted((v for v, d in releases if d == target and v != version), key=plan.parse_version)
    if other:
        return tuple(r for r in releases if r[0] != version), other[-1]
    return plan.with_release(tuple(releases), version, target), None


def key(version: str, name: str) -> str:
    return f"{KEY_PREFIX} {version}/{name}"


@dataclass(frozen=True)
class Needs:
    run: bool
    env: bool  # the job needs the `release` environment (App Store Connect, Play, website deploy key)
    why: str = ""


# The Play step is finished once the bundle is on closed testing, or when the run had no Play key ("play-manual":
# the bundle then goes up by hand, or with step submit once the secret exists).
PLAY_DONE = ("play-uploaded", "play-manual")


def submitted(version: str, posted: frozenset[str]) -> bool:
    """Whether the Tuesday store step is finished: notes posted, iOS submitted, Play done."""
    done = lambda name: key(version, name) in posted  # noqa: E731
    return done("notes") and done("ios-submitted") and any(done(n) for n in PLAY_DONE)


def needs(now: datetime, releases: Sequence[tuple[str, date]], posted: frozenset[str]) -> Needs:
    """Whether a tick has anything to do (the cheap plan job), from the time and the notes already posted."""
    reasons, env = [], False
    for version, publish, p in active(now, releases):
        done = lambda name: key(version, name) in posted  # noqa: E731
        if p == "prep":
            todo = [] if done("frozen") else ["freeze"]
            if now >= milestone(publish, "signed") and not done("signed"):
                todo.append("signed")
            if now >= milestone(publish, "submitted") and not submitted(version, posted):
                todo.append("submit")
                env = True
            if todo:
                reasons.append(f"{version}: {', '.join(todo)}")
        elif p == "decide" and not (done(f"go/{publish.isoformat()}") or done(f"nogo/{publish.isoformat()}")):
            reasons.append(f"{version}: go/no-go")
            env = True
        elif p == "publish" and not (done("published") or done(f"nogo/{publish.isoformat()}")):
            reasons.append(f"{version}: publish")
            env = True
    return Needs(bool(reasons), env, "; ".join(reasons) or "nothing due")


@dataclass
class Note:
    """A #mod-chat message; its footer key makes posting it idempotent."""
    version: str
    name: str
    title: str
    lines: list[str] = field(default_factory=list)

    @property
    def key(self) -> str:
        return key(self.version, self.name)

    def message(self) -> dict:
        description = "\n".join(self.lines)[:4000] or "-"
        return {
            "content": f"**Borealis {self.version}: {self.title}**"[:2000],
            "embeds": [{"title": f"Borealis {self.version}: {self.title}"[:256], "description": description,
                        "color": 0x2BA68A, "footer": {"text": self.key}}],
            "allowed_mentions": {"parse": []},
        }


def stamp(dt: datetime, style: str = "F") -> str:
    return f"<t:{int(dt.timestamp())}:{style}>"


HOLD_HELP = ("To hold it, set the repository variable RELEASE_HOLD to the version: "
             "`gh variable set RELEASE_HOLD --body {v} -R TeamBorealis/borealis` "
             "(or Settings > Secrets and variables > Actions > Variables). Clear it to let the release go on.")


def hold_help(version: str) -> str:
    return HOLD_HELP.format(v=version)
