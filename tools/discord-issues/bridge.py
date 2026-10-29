"""Pure helpers for the Discord forum -> GitHub issues bridge (no network; sync.py does the I/O).

Post text is untrusted user content: it is quoted, never interpreted. Python 3.9+, stdlib only.
"""
import re
from datetime import datetime, timezone

GUILD = "1466376402986336550"
BUG_FORUM = "1564901776216293481"
IDEA_FORUM = "1564901779353763951"
# The bot's pinned "How to report a bug (read first)" guide post.
SKIP_THREADS = {"1564902018445877338"}

FORUMS = {
    BUG_FORUM: {"kind": "bug", "label": "bug"},
    IDEA_FORUM: {"kind": "idea", "label": "enhancement"},
}

MAX_TAGS = 5  # Discord's limit on applied tags per forum post
BODY_LIMIT = 60000  # GitHub caps issue bodies at 65536 characters
TITLE_LIMIT = 200
ZWSP = "​"

MARKER_RE = re.compile(r"\A\s*<!-- discord-thread:(\d+) -->")
# The credit line sits right after the marker: "**Reported by** <name> on Discord" (ideas: "Suggested by").
CREDIT_RE = re.compile(r"\*\*(Reported|Suggested) by\*\* (.*) on Discord")
MARKDOWN_SPECIALS = re.compile(r"([\\`*_~\[\]()|!>#])")
NAME_LIMIT = 80

# Discord tag name (lower case) -> GitHub label. Status tags (Confirmed, Fixed, ...) are not mapped.
TAG_LABELS = {
    "windows": "windows",
    "macos": "macos",
    "linux": "linux",
    "android": "android",
    "crash": "crash",
    "builder": "builder",
    "character sheet": "character-sheet",
    "imports": "imports",
    "phone": "phone",
    "desktop": "desktop",
}
LABEL_COLOURS = {
    "discord": ("5865f2", "Reported on the Borealis Discord"),
    "windows": ("0078d4", "Windows"),
    "macos": ("8e8e93", "macOS"),
    "linux": ("f0c419", "Linux"),
    "android": ("3ddc84", "Android"),
    "crash": ("b60205", "The app crashes"),
    "builder": ("c5def5", "Character builder"),
    "character-sheet": ("bfd4f2", "Character sheet"),
    "imports": ("fbca04", "Aurora / 5etools imports"),
    "phone": ("d4c5f9", "Phone layout"),
    "desktop": ("c2e0c6", "Desktop layout"),
}
PLATFORM_TAGS = {"windows", "macos", "linux", "android", "phone", "desktop"}
STATUS_TAGS = {"confirmed", "fixed", "needs info", "planned", "done", "not planned"}
# Status tags dropped first when a post would go over MAX_TAGS.
DROP_FIRST = ("needs info", "confirmed", "planned")


def marker(thread_id):
    return f"<!-- discord-thread:{thread_id} -->"


def thread_of(body):
    """The Discord thread id an issue body was bridged from, or None. Only the leading marker counts."""
    match = MARKER_RE.match(body or "")
    return match.group(1) if match else None


def thread_url(thread_id):
    return f"https://discord.com/channels/{GUILD}/{thread_id}"


def _timestamp(match):
    try:
        moment = datetime.fromtimestamp(int(match.group(1)), tz=timezone.utc)
    except (OverflowError, OSError, ValueError):
        return match.group(0)
    return moment.strftime("%Y-%m-%d %H:%M UTC")


def scrub_discord(text):
    """Replace Discord markup that carries ids (users, roles, channels, emoji) with plain words."""
    text = re.sub(r"<@!?\d+>", "@user", text)
    text = re.sub(r"<@&\d+>", "@role", text)
    text = re.sub(r"<#\d+>", "#channel", text)
    text = re.sub(r"<a?:(\w+):\d+>", r":\1:", text)
    text = re.sub(r"</([\w -]+):\d+>", r"/\1", text)
    return re.sub(r"<t:(-?\d+)(?::[a-zA-Z])?>", _timestamp, text)


def neutralise(text):
    """Stop GitHub from acting on user text: no @mentions, no #123 cross-references, no raw HTML or comments."""
    text = text.replace("@", "@" + ZWSP)
    text = re.sub(r"#(?=\d)", "#" + ZWSP, text)
    return text.replace("<", "<" + ZWSP)


def inline_text(text, limit=NAME_LIMIT):
    """Untrusted text for one Markdown line: one line, bounded, Markdown escaped, then neutralised."""
    text = " ".join(scrub_discord(text or "").split())[:limit]
    return neutralise(MARKDOWN_SPECIALS.sub(r"\\\1", text))


def poster_name(member, user):
    """Server nickname, else global display name, else username ("" when none). `member` is None after a leave."""
    for candidate in ((member or {}).get("nick"), (user or {}).get("global_name"), (user or {}).get("username")):
        name = " ".join(str(candidate or "").split())
        if name:
            return name
    return ""


def credit_line(kind, name):
    verb = "Reported" if kind == "bug" else "Suggested"
    return f"**{verb} by** {inline_text(name) or 'a community member'} on Discord"


def credit_of(body):
    """(verb, name as Markdown) from the credit line right after the marker, or None."""
    lines = (body or "").split("\n")
    match = CREDIT_RE.fullmatch(lines[1]) if len(lines) > 1 else None
    return (match.group(1).lower(), match.group(2)) if match else None


def with_credit(body, line):
    """The body with `line` as its credit line (replacing an older one); the same body when already there."""
    lines = body.split("\n")
    rest = lines[2:] if credit_of(body) else lines[1:]
    return "\n".join(lines[:1] + [line] + rest)


def thanks(kind, poster_id):
    """A thank-you that mentions the poster, or "" without a valid user id."""
    if not str(poster_id or "").isdigit():
        return ""
    return f"Thanks <@{poster_id}> for reporting this!" if kind == "bug" else f"Thanks <@{poster_id}> for the idea!"


def _with_thanks(text, kind, poster_id):
    line = thanks(kind, poster_id)
    return f"{text}\n{line}" if line else text


def quote(text):
    return "\n".join(("> " + line) if line else ">" for line in text.splitlines()) or ">"


def safe_filename(name):
    """A filename safe for a repository path and Markdown: [A-Za-z0-9._-], no leading dot, bounded."""
    cleaned = re.sub(r"[^A-Za-z0-9._-]", "_", name or "").lstrip(".")[:80]
    return cleaned or "file"


def sniff_image(data):
    """The image extension for PNG/JPEG/GIF/WebP bytes, or None. SVG and anything else is not an image here."""
    if data.startswith(b"\x89PNG\r\n\x1a\n"):
        return "png"
    if data.startswith(b"\xff\xd8\xff"):
        return "jpg"
    if data[:6] in (b"GIF87a", b"GIF89a"):
        return "gif"
    if data[:4] == b"RIFF" and data[8:12] == b"WEBP":
        return "webp"
    return None


def media_path(thread_id, attachment_id, filename, ext):
    stem = safe_filename(filename).rsplit(".", 1)[0] or "image"
    return f"{thread_id}/{attachment_id}-{stem}.{ext}"


def tag_names(applied_ids, available_tags):
    """Names of the applied forum tags, in the forum's tag order."""
    applied = set(applied_ids or ())
    return [tag["name"] for tag in available_tags if tag["id"] in applied]


def labels_for(forum_id, names):
    labels = {"discord", FORUMS[forum_id]["label"]}
    labels.update(TAG_LABELS[n.lower()] for n in names if n.lower() in TAG_LABELS)
    return sorted(labels)


def issue_title(thread_name, thread_id):
    title = " ".join((thread_name or "").split())
    return title[:TITLE_LIMIT] or f"Discord post {thread_id}"


def issue_body(thread_id, names, text, images, other_files, credit):
    """The bridged issue body: marker, credit, link, tags, the quoted starter message, rehosted images, file notes."""
    parts = [
        marker(thread_id),
        credit,
        f"**Reported on Discord:** {thread_url(thread_id)}",
        "**Discord tags:** " + (", ".join(names) if names else "none"),
        "",
        quote(neutralise(scrub_discord(text.strip()))) if text.strip() else "> *(no text, attachments only)*",
    ]
    if images:
        parts += ["", "**Screenshots:**", ""]
        parts += [f"![{safe_filename(name)}]({url})" for name, url in images]
    if other_files:
        parts.append("")
        parts += [f"- Attached file `{safe_filename(name)}` is in the Discord post." for name in other_files]
    parts += ["", "---", "<sub>Imported from the Borealis Discord. Further discussion is in the Discord post.</sub>"]
    body = "\n".join(parts)
    if len(body) > BODY_LIMIT:
        body = body[:BODY_LIMIT] + "\n\n*(truncated; the full text is in the Discord post)*"
    return body


def _drop_order(name, add_name):
    """Lower drops first: status tags, then area tags, then platform tags. The tag being added never drops."""
    if name == add_name:
        return 99
    if name in DROP_FIRST:
        return DROP_FIRST.index(name)
    if name in STATUS_TAGS:
        return 3
    if name not in PLATFORM_TAGS:
        return 4
    return 5


def with_status_tag(applied_ids, available_tags, add_name, remove_names=()):
    """New applied-tag id list with `add_name` on, `remove_names` off and at most MAX_TAGS tags."""
    by_name = {t["name"].lower(): t["id"] for t in available_tags}
    by_id = {t["id"]: t["name"].lower() for t in available_tags}
    removed = {by_name[n] for n in remove_names if n in by_name}
    tags = [t for t in (applied_ids or []) if t not in removed]
    add_id = by_name.get(add_name) if add_name else None
    if add_id and add_id not in tags:
        tags = tags + [add_id]
    while len(tags) > MAX_TAGS:
        victim = min(tags, key=lambda t: (_drop_order(by_id.get(t, ""), add_name), -tags.index(t)))
        tags = [t for t in tags if t != victim]
    return tags


def created_message(kind, number, url):
    if kind == "bug":
        return f"Thanks for the report! It's tracked as GitHub issue #{number}: {url}\nI'll post here when it's fixed."
    return f"Thanks for the idea! It's tracked as GitHub issue #{number}: {url}\nI'll post here when it's done."


def close_plan(kind, state_reason, url, poster_id=None):
    """(tag to add, tags to remove, reply) for a closed issue, or None when nothing should be said.
    Fixed/done replies thank the poster by mention."""
    if state_reason == "completed":
        if kind == "bug":
            text = f"✅ Fixed on main: {url}\nThe fix ships in the next release; I'll post here when it's out."
            return "fixed", (), _with_thanks(text, kind, poster_id)
        text = f"✅ Done: {url}\nIt ships in the next release; I'll post here when it's out."
        return "done", ("not planned",), _with_thanks(text, kind, poster_id)
    if state_reason == "not_planned":
        if kind == "bug":
            return None, (), f"This was closed on GitHub as not planned: {url}"
        return "not planned", ("done", "planned"), f"🚫 Not planned for now: {url}"
    if state_reason == "duplicate":
        return None, (), f"This was closed on GitHub as a duplicate: {url}"
    return None


def release_message(tag, url, kind=None, poster_id=None):
    return _with_thanks(f"🚀 Released in {tag}: {url}", kind, poster_id)


def release_window(releases, tag):
    """(start, end) ISO times: from the previous published release to this one. None for drafts/pre-releases."""
    stable = [r for r in releases if not r.get("draft") and not r.get("prerelease") and r.get("published_at")]
    current = next((r for r in stable if r["tag_name"] == tag), None)
    if current is None:
        return None
    end = current["published_at"]
    earlier = [r["published_at"] for r in stable if r["published_at"] < end]
    return (max(earlier) if earlier else ""), end


def released_issues(issues, start, end):
    """Bridged issues closed as completed inside (start, end]."""
    return [
        i for i in issues
        if i.get("state_reason") == "completed" and i.get("closed_at") and start < i["closed_at"] <= end
    ]


def credits_markdown(entries):
    """Release-notes Markdown for [(number, title, verb, name)]: one line per issue, thanking the poster."""
    if not entries:
        return ""
    lines = ["**Community credits**: thanks to everyone who reported bugs and suggested features on the Borealis Discord!", ""]
    for number, title, verb, name in sorted(entries, key=lambda e: (e[2] != "reported", e[0])):
        lines.append(f"- {inline_text(title, TITLE_LIMIT)} (#{number}), {verb} by {name}")
    return "\n".join(lines)
