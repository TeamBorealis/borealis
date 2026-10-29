#!/usr/bin/env python3
"""Bridge the Discord #bug-reports and #feature-requests forums to GitHub issues, and report back.

Each run, idempotently (no state file; the issue body's leading `<!-- discord-thread:<id> -->` marker is the link):
  1. every forum post without an issue becomes one (labels from the forum and its tags; images rehosted on the
     orphan `discord-media` branch because Discord CDN links expire), and the bot replies once with the link;
  2. bridged issues without a credit line ("**Reported by** <poster name> on Discord") get one;
  3. bridged issues closed in the last RECONCILE_DAYS get the Fixed/Done/Not planned tag and one reply;
  4. releases published in the last RECONCILE_DAYS get one "Released in vX.Y.Z" reply on the posts whose issues
     were closed as completed since the previous release.
Fixed/done/released replies thank the poster by mention (only they are pinged). Release-notes credits: credits.py.
A reply is never repeated: the bot's own messages in the thread are checked first. Post text is untrusted data.

Env: DISCORD_BOT_TOKEN, GITHUB_TOKEN, GITHUB_REPOSITORY (default TeamBorealis/borealis).
Usage: sync.py [--dry-run]. Exit 0 ok, 1 when any post or issue failed (each is logged with its id and HTTP status).
Python 3.9+, stdlib only.
"""
import argparse
import base64
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import bridge  # noqa: E402

DISCORD_API = "https://discord.com/api/v10"
GITHUB_API = "https://api.github.com"
MEDIA_BRANCH = "discord-media"
MESSAGE_CONTENT_FLAGS = (1 << 18) | (1 << 19)  # GATEWAY_MESSAGE_CONTENT, GATEWAY_MESSAGE_CONTENT_LIMITED
PINNED_THREAD = 1 << 1
SUPPRESS_EMBEDS = 1 << 2
RECONCILE_DAYS = 7
REPLY_RETRY_HOURS = 48
MAX_IMAGE_BYTES = 8 * 1024 * 1024
MAX_IMAGES = 10
CDN_HOSTS = {"cdn.discordapp.com", "media.discordapp.net"}
RETRIES = 5
TIMEOUT_S = 30


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None  # attachment downloads stay on the Discord CDN


CDN_OPENER = urllib.request.build_opener(_NoRedirect)


class ApiError(Exception):
    def __init__(self, method, path, status, message=""):
        super().__init__(f"{method} {path} -> HTTP {status} {message}".strip())
        self.status = status


def _retry_after(err, raw):
    try:
        return float(json.loads(raw).get("retry_after"))
    except (ValueError, TypeError, AttributeError):
        return float(err.headers.get("Retry-After") or 5)


def _error_message(raw):
    try:
        return str(json.loads(raw).get("message", ""))[:200]
    except (ValueError, AttributeError):
        return ""


def http(method, url, headers, body=None, log_path=None):
    """One JSON request. 429 is retried; 5xx and network errors only for GET, so a write is never sent twice.
    Errors carry the method, path and status only, never headers."""
    log_path = log_path or urllib.parse.urlsplit(url).path
    data = None if body is None else json.dumps(body).encode()
    for attempt in range(RETRIES):
        req = urllib.request.Request(url, data=data, method=method, headers=headers)
        if data is not None:
            req.add_header("Content-Type", "application/json")
        try:
            with urllib.request.urlopen(req, timeout=TIMEOUT_S) as resp:
                raw = resp.read()
                return json.loads(raw) if raw else None
        except urllib.error.HTTPError as err:
            raw = err.read()
            limited = err.code == 429 or (err.code == 403 and err.headers.get("Retry-After"))
            if limited and attempt < RETRIES - 1:
                time.sleep(min(_retry_after(err, raw), 60) + 0.5)
                continue
            if err.code >= 500 and method == "GET" and attempt < RETRIES - 1:
                time.sleep(2 ** attempt)
                continue
            raise ApiError(method, log_path, err.code, _error_message(raw)) from None
        except urllib.error.URLError as err:
            if method == "GET" and attempt < RETRIES - 1:
                time.sleep(2 ** attempt)
                continue
            raise ApiError(method, log_path, "network", str(err.reason)[:100]) from None
    raise ApiError(method, log_path, "retries exhausted")


class Bridge:
    def __init__(self, dry_run, discord_token, github_token, repo):
        self.dry_run = dry_run
        self.repo = repo
        self.failures = []
        self._discord_headers = {
            "Authorization": f"Bot {discord_token}",
            "User-Agent": f"DiscordBot (https://github.com/{repo}, 1)",
        }
        self._github_headers = {
            "Authorization": f"Bearer {github_token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "borealis-discord-issues",
        }
        self._threads = {}
        self._bot_messages = {}
        self._names = {}
        self._labels = None
        self._media_ready = False

    # --- plumbing -------------------------------------------------------------------------------------------

    def discord(self, method, path, body=None):
        return http(method, DISCORD_API + path, self._discord_headers, body)

    def github(self, method, path, body=None):
        return http(method, GITHUB_API + path, self._github_headers, body)

    def github_pages(self, path):
        sep = "&" if "?" in path else "?"
        page, items = 1, []
        while True:
            batch = self.github("GET", f"{path}{sep}per_page=100&page={page}")
            items += batch
            if len(batch) < 100:
                return items
            page += 1

    def write(self, description, action):
        """Run a write, or only print it under --dry-run."""
        print(("DRY-RUN would: " if self.dry_run else "") + description)
        return None if self.dry_run else action()

    def fail(self, what, err):
        self.failures.append(what)
        print(f"::error::{what}: {err}")

    # --- Discord reads --------------------------------------------------------------------------------------

    def forum_threads(self):
        """Every post in both forums: active ones plus archived ones (paginated)."""
        active = self.discord("GET", f"/guilds/{bridge.GUILD}/threads/active")["threads"]
        threads = {t["id"]: t for t in active if t.get("parent_id") in bridge.FORUMS}
        for forum in bridge.FORUMS:
            before = ""
            while True:
                page = self.discord("GET", f"/channels/{forum}/threads/archived/public?limit=100{before}")
                threads.update((t["id"], t) for t in page["threads"])
                if not page.get("has_more") or not page["threads"]:
                    break
                oldest = page["threads"][-1]["thread_metadata"]["archive_timestamp"]
                before = "&before=" + urllib.parse.quote(oldest)
        self._threads.update(threads)
        return sorted(threads.values(), key=lambda t: int(t["id"]))

    def thread(self, thread_id):
        if thread_id not in self._threads:
            self._threads[thread_id] = self.discord("GET", f"/channels/{thread_id}")
        return self._threads[thread_id]

    def bot_messages(self, thread_id):
        """Text of the bot's own recent messages in a thread (visible without the Message Content intent)."""
        if thread_id not in self._bot_messages:
            messages = self.discord("GET", f"/channels/{thread_id}/messages?limit=100")
            self._bot_messages[thread_id] = [m["content"] for m in messages if m["author"]["id"] == self.bot_id]
        return self._bot_messages[thread_id]

    def poster(self, user_id):
        """The poster's credit name: server nickname, global display name or username ("" when unknown)."""
        if not str(user_id or "").isdigit():
            return ""
        if user_id not in self._names:
            try:
                member = self.discord("GET", f"/guilds/{bridge.GUILD}/members/{user_id}")
                user = member.get("user")
            except ApiError as err:
                if err.status != 404:
                    raise
                member, user = None, self.discord("GET", f"/users/{user_id}")  # left the server
            self._names[user_id] = bridge.poster_name(member, user)
        return self._names[user_id]

    def reply_once(self, thread_id, text, tags=None, mention=None):
        """Post `text` in the thread unless the bot already said it; optionally set the post's tags first.
        Only `mention` (a user id) may be pinged."""
        if text in self.bot_messages(thread_id):
            return False
        thread = self.thread(thread_id)
        patch = {}
        if tags is not None and tags != thread.get("applied_tags", []):
            patch["applied_tags"] = tags
        if thread.get("thread_metadata", {}).get("archived"):
            patch["archived"] = False
        if patch:
            self.write(f"set thread {thread_id} {patch}", lambda: self.discord("PATCH", f"/channels/{thread_id}", patch))
            metadata = {**thread.get("thread_metadata", {}), "archived": False}
            self._threads[thread_id] = {**thread, "applied_tags": patch.get("applied_tags", thread.get("applied_tags")),
                                        "thread_metadata": metadata}
        allowed = {"parse": [], "users": [mention]} if mention else {"parse": []}
        body = {"content": text, "allowed_mentions": allowed, "flags": SUPPRESS_EMBEDS}
        self.write(f"reply in thread {thread_id}: {text!r}",
                   lambda: self.discord("POST", f"/channels/{thread_id}/messages", body))
        self._bot_messages[thread_id].append(text)
        return True

    # --- GitHub ---------------------------------------------------------------------------------------------

    def bridged_issues(self):
        """thread id -> issue, from every issue whose body starts with the marker (pull requests skipped)."""
        found = {}
        for issue in self.github_pages(f"/repos/{self.repo}/issues?state=all"):
            thread_id = bridge.thread_of(issue.get("body"))
            if thread_id and "pull_request" not in issue:
                found.setdefault(thread_id, issue)
        return found

    def ensure_labels(self, names):
        if self._labels is None:
            self._labels = {l["name"].lower() for l in self.github_pages(f"/repos/{self.repo}/labels")}
        for name in names:
            if name.lower() in self._labels or name not in bridge.LABEL_COLOURS:
                continue
            colour, description = bridge.LABEL_COLOURS[name]
            body = {"name": name, "color": colour, "description": description}
            self.write(f"create label {name}", lambda: self.github("POST", f"/repos/{self.repo}/labels", body))
            self._labels.add(name)

    def ensure_media_branch(self):
        """Create the orphan `discord-media` branch (one README commit, no parents) if it is missing."""
        if self._media_ready:
            return
        try:
            self.github("GET", f"/repos/{self.repo}/git/ref/heads/{MEDIA_BRANCH}")
        except ApiError as err:
            if err.status != 404:
                raise
            self.write(f"create orphan branch {MEDIA_BRANCH}", self._create_media_branch)
        self._media_ready = True

    def _create_media_branch(self):
        readme = "Images from Discord bug reports and feature requests, rehosted by tools/discord-issues.\n"
        blob = self.github("POST", f"/repos/{self.repo}/git/blobs", {"content": readme, "encoding": "utf-8"})
        tree = self.github("POST", f"/repos/{self.repo}/git/trees", {
            "tree": [{"path": "README.md", "mode": "100644", "type": "blob", "sha": blob["sha"]}]})
        commit = self.github("POST", f"/repos/{self.repo}/git/commits", {
            "message": "chore: start discord-media branch", "tree": tree["sha"], "parents": []})
        self.github("POST", f"/repos/{self.repo}/git/refs", {"ref": f"refs/heads/{MEDIA_BRANCH}", "sha": commit["sha"]})

    def upload_media(self, path, data):
        """Commit one image to the media branch (skipped when already there); returns its raw URL."""
        url = f"https://raw.githubusercontent.com/{self.repo}/{MEDIA_BRANCH}/{path}"
        api = f"/repos/{self.repo}/contents/{path}"
        try:
            self.github("GET", f"{api}?ref={MEDIA_BRANCH}")
            return url
        except ApiError as err:
            if err.status != 404:
                raise
        body = {"message": f"chore: discord media {path}", "branch": MEDIA_BRANCH,
                "content": base64.b64encode(data).decode()}
        self.write(f"upload {path} ({len(data)} bytes) to {MEDIA_BRANCH}", lambda: self.github("PUT", api, body))
        return url

    # --- step 1: new posts ----------------------------------------------------------------------------------

    def download(self, attachment):
        """Attachment bytes from the Discord CDN only, capped at MAX_IMAGE_BYTES; None when too big or elsewhere."""
        parts = urllib.parse.urlsplit(attachment.get("url", ""))
        if parts.scheme != "https" or parts.hostname not in CDN_HOSTS:
            return None
        if int(attachment.get("size") or 0) > MAX_IMAGE_BYTES:
            return None
        req = urllib.request.Request(attachment["url"], headers={"User-Agent": "borealis-discord-issues"})
        with CDN_OPENER.open(req, timeout=TIMEOUT_S) as resp:
            data = resp.read(MAX_IMAGE_BYTES + 1)
        return data if len(data) <= MAX_IMAGE_BYTES else None

    def rehost(self, thread_id, attachments):
        """([(name, raw url)], [other file names]): images rehosted, everything else only named."""
        images, others = [], []
        for att in attachments:
            name = att.get("filename", "file")
            looks_image = (att.get("content_type") or "").startswith("image/") and "svg" not in att.get("content_type")
            if not looks_image or len(images) >= MAX_IMAGES:
                others.append(name)
                continue
            if self.dry_run:
                print(f"DRY-RUN would: rehost image attachment {att.get('id')} of thread {thread_id}")
                images.append((name, f"https://raw.githubusercontent.com/{self.repo}/{MEDIA_BRANCH}/{thread_id}/..."))
                continue
            try:
                data = self.download(att)
            except (urllib.error.URLError, OSError) as err:
                print(f"::warning::attachment {att.get('id')} of thread {thread_id} not downloaded: {type(err).__name__}")
                data = None
            ext = bridge.sniff_image(data) if data else None
            if not ext:
                others.append(name)
                continue
            self.ensure_media_branch()
            images.append((name, self.upload_media(bridge.media_path(thread_id, att["id"], name, ext), data)))
        return images, others

    def import_post(self, thread):
        thread_id, forum_id = thread["id"], thread["parent_id"]
        kind = bridge.FORUMS[forum_id]["kind"]
        if any("/issues/" in text for text in self.bot_messages(thread_id)):
            print(f"::warning::thread {thread_id} already has a bot issue link but no marked issue; skipped")
            return
        starter = self.discord("GET", f"/channels/{thread_id}/messages/{thread_id}")
        text, attachments = starter.get("content", ""), starter.get("attachments", [])
        if not text.strip() and not attachments:
            if not self.dry_run:
                raise ApiError("GET", f"/channels/{thread_id}/messages/{thread_id}", 200,
                               "starter message has no visible content (Message Content intent?)")
            text = "(content not visible to the bot: Message Content intent is off)"
        names = bridge.tag_names(thread.get("applied_tags"), self.forum_tags[forum_id])
        credit = bridge.credit_line(kind, self.poster(thread.get("owner_id")))
        labels = bridge.labels_for(forum_id, names)
        images, others = self.rehost(thread_id, attachments)
        self.ensure_labels(labels)
        payload = {"title": bridge.issue_title(thread.get("name"), thread_id),
                   "body": bridge.issue_body(thread_id, names, text, images, others, credit), "labels": labels}
        print(f"post {thread_id} ({kind}) -> new issue {payload['title']!r} labels={labels}")
        if self.dry_run:
            print("DRY-RUN issue body:\n" + "\n".join("    " + l for l in payload["body"].splitlines()))
        issue = self.write(f"create issue for thread {thread_id}",
                           lambda: self.github("POST", f"/repos/{self.repo}/issues", payload))
        number, url = (issue["number"], issue["html_url"]) if issue else ("N", f"https://github.com/{self.repo}/issues/N")
        self.reply_once(thread_id, bridge.created_message(kind, number, url))

    def retry_reply(self, thread, issue):
        """A recent bridged issue whose link reply failed earlier gets it now."""
        created = datetime.fromisoformat(issue["created_at"].replace("Z", "+00:00"))
        if datetime.now(timezone.utc) - created > timedelta(hours=REPLY_RETRY_HOURS):
            return
        if any(issue["html_url"] in text for text in self.bot_messages(thread["id"])):
            return
        kind = bridge.FORUMS[thread["parent_id"]]["kind"]
        self.reply_once(thread["id"], bridge.created_message(kind, issue["number"], issue["html_url"]))

    def import_posts(self, issues):
        flags = self.discord("GET", "/applications/@me").get("flags") or 0
        has_intent = bool(flags & MESSAGE_CONTENT_FLAGS)
        for thread in self.forum_threads():
            thread_id = thread["id"]
            if thread_id in bridge.SKIP_THREADS or thread.get("owner_id") == self.bot_id:
                continue
            if (thread.get("flags") or 0) & PINNED_THREAD:
                continue
            try:
                if thread_id in issues:
                    self.retry_reply(thread, issues[thread_id])
                elif not has_intent and not self.dry_run:
                    print(f"::warning::post {thread_id} not imported: the bot's Message Content intent is off")
                else:
                    self.import_post(thread)
            except ApiError as err:
                if err.status == 404:
                    print(f"::warning::post {thread_id} skipped: {err}")
                    continue
                self.fail(f"post {thread_id}", err)
            except Exception as err:  # one bad post must not stop the others
                self.fail(f"post {thread_id}", type(err).__name__)

    # --- steps 2 and 3: report back -------------------------------------------------------------------------

    def report_closed(self, issues, since):
        for thread_id, issue in issues.items():
            if issue["state"] != "closed" or not issue.get("closed_at") or issue["closed_at"] < since:
                continue
            try:
                thread = self.thread(thread_id)
                kind = bridge.FORUMS[thread["parent_id"]]["kind"]
                poster_id = thread.get("owner_id")
                plan = bridge.close_plan(kind, issue.get("state_reason"), issue["html_url"], poster_id)
                if plan is None:
                    continue
                add, remove, text = plan
                tags = None
                if add:
                    tags = bridge.with_status_tag(thread.get("applied_tags"), self.forum_tags[thread["parent_id"]],
                                                  add, remove)
                if self.reply_once(thread_id, text, tags, _mention(text, poster_id)):
                    print(f"issue #{issue['number']} closed ({issue.get('state_reason')}) -> thread {thread_id}")
            except ApiError as err:
                if err.status == 404:
                    print(f"::warning::issue #{issue['number']}: thread {thread_id} is gone")
                    continue
                self.fail(f"issue #{issue['number']} (thread {thread_id})", err)
            except Exception as err:
                self.fail(f"issue #{issue['number']} (thread {thread_id})", type(err).__name__)

    def report_releases(self, issues, since):
        releases = self.github("GET", f"/repos/{self.repo}/releases?per_page=50")
        for release in releases:
            if not release.get("published_at") or release["published_at"] < since:
                continue
            window = bridge.release_window(releases, release["tag_name"])
            if window is None:
                continue
            for issue in bridge.released_issues(issues.values(), *window):
                thread_id = bridge.thread_of(issue["body"])
                try:
                    thread = self.thread(thread_id)
                    poster_id = thread.get("owner_id")
                    kind = bridge.FORUMS[thread["parent_id"]]["kind"]
                    text = bridge.release_message(release["tag_name"], release["html_url"], kind, poster_id)
                    if self.reply_once(thread_id, text, mention=_mention(text, poster_id)):
                        print(f"release {release['tag_name']} -> thread {thread_id} (issue #{issue['number']})")
                except ApiError as err:
                    if err.status == 404:
                        continue
                    self.fail(f"release {release['tag_name']} issue #{issue['number']}", err)
                except Exception as err:
                    self.fail(f"release {release['tag_name']} issue #{issue['number']}", type(err).__name__)

    # --- credits on issues bridged before the credit line existed ------------------------------------------

    def credit_issues(self, issues):
        for thread_id, issue in list(issues.items()):
            if bridge.credit_of(issue.get("body")):
                continue
            try:
                thread = self.thread(thread_id)
                kind = bridge.FORUMS[thread["parent_id"]]["kind"]
                body = bridge.with_credit(issue["body"], bridge.credit_line(kind, self.poster(thread.get("owner_id"))))
                print(f"issue #{issue['number']}: add credit line {body.splitlines()[1]!r}")
                path = f"/repos/{self.repo}/issues/{issue['number']}"
                self.write(f"update body of issue #{issue['number']}", lambda: self.github("PATCH", path, {"body": body}))
                issues[thread_id] = {**issue, "body": body}
            except ApiError as err:
                if err.status == 404:
                    print(f"::warning::issue #{issue['number']}: thread {thread_id} is gone; no credit added")
                    continue
                self.fail(f"credit issue #{issue['number']}", err)
            except Exception as err:
                self.fail(f"credit issue #{issue['number']}", type(err).__name__)

    def run(self):
        self.bot_id = self.discord("GET", "/users/@me")["id"]
        self.forum_tags = {f: self.discord("GET", f"/channels/{f}")["available_tags"] for f in bridge.FORUMS}
        issues = self.bridged_issues()
        print(f"{len(issues)} bridged issue(s) found")
        self.import_posts(issues)
        self.credit_issues(issues)
        since = (datetime.now(timezone.utc) - timedelta(days=RECONCILE_DAYS)).strftime("%Y-%m-%dT%H:%M:%SZ")
        self.report_closed(issues, since)
        self.report_releases(issues, since)
        if self.failures:
            print(f"{len(self.failures)} failure(s): {', '.join(self.failures)}")
            return 1
        print("done")
        return 0


def _mention(text, poster_id):
    """The poster's id when `text` mentions them (so only they may be pinged), else None."""
    return poster_id if poster_id and f"<@{poster_id}>" in text else None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dry-run", action="store_true", help="read everything, write nothing, print the plan")
    args = parser.parse_args(argv)
    discord_token = os.environ.get("DISCORD_BOT_TOKEN", "").strip()
    github_token = os.environ.get("GITHUB_TOKEN", "").strip()
    if not discord_token or not github_token:
        print("::error::DISCORD_BOT_TOKEN and GITHUB_TOKEN must be set")
        return 2
    repo = os.environ.get("GITHUB_REPOSITORY", "TeamBorealis/borealis")
    try:
        return Bridge(args.dry_run, discord_token, github_token, repo).run()
    except ApiError as err:
        print(f"::error::{err}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
