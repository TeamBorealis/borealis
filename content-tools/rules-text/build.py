#!/usr/bin/env python3
"""Build the SRD rules-text packs (`borealis-docs/1`) from the official SRD PDFs.

    python3 -I content-tools/rules-text/build.py [--input-dir DIR] [--out-dir DIR] [--only srd-5.1|srd-5.2.1]

Writes `content/<pack>/docs.json`, `ATTRIBUTION.md` and `COVERAGE.md`. Deterministic: the same PDF always gives
byte-identical files (no clocks, no randomness, no AI). Needs poppler's `pdftohtml` and `pdftotext`.
"""
import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import unicodedata
from typing import Dict, List

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from config import PACKS, PackConfig  # noqa: E402
from layout import Vocabulary, render, row_text  # noqa: E402
from pdfxml import document_events, outline_items  # noqa: E402

FORMAT = "borealis-docs/1"
VERSION = "1.0.0"
SEP = " > "
MIN_OUTLINE_ITEMS = 10  # the SRD 5.1 PDF only bookmarks its two attached licence files


def slug(text: str) -> str:
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-") or "section"


def norm(text: str) -> str:
    text = unicodedata.normalize("NFKD", text).lower()
    text = text.replace("’", "'").replace("–", "-").replace("—", "-").replace("‑", "-")
    return re.sub(r"[^a-z0-9]+", " ", text).strip()


def is_excluded(cfg: PackConfig, path: List[str]) -> bool:
    joined = SEP.join(path)
    return any(re.search(p, joined) for p in cfg.exclude)


def build_sections(events: list, cfg: PackConfig):
    """Return (sections, detected) where detected lists every font-detected heading with its scope."""
    vocab = Vocabulary()
    for ev in events:
        if ev.kind == "row":
            vocab.add_row(ev)
    stack: List[tuple] = []  # (level, text, synthetic)
    sections: List[dict] = []
    detected: List[dict] = []
    current = None

    def finish():
        nonlocal current
        if current is None:
            return
        if not current["excluded"]:
            sections.append(
                {
                    "path": current["path"],
                    "page": current["page"],
                    "html": render(current["rows"], vocab, cfg.indent_paragraphs),
                }
            )
        current = None

    for ev in events:
        if ev.page < cfg.first_page:
            continue
        if ev.kind == "row":
            if current is not None:
                current["rows"].append(ev)
            continue
        finish()
        level, text = ev.level, ev.text
        virtual = cfg.virtual_chapters.get(text)
        if virtual and level == 2 and not (stack and stack[0][1] == virtual):
            stack = [(1, virtual, True)]
        in_flat = bool(stack) and stack[0][1] in cfg.flat_chapters
        if in_flat and level == 4:
            stack = [e for e in stack if e[0] <= 2][:1] + [(level, text, False)]
        else:
            while stack and stack[-1][0] >= level:
                stack.pop()
            stack.append((level, text, False))
        path = [e[1] for e in stack]
        excluded = is_excluded(cfg, path)
        detected.append({"path": path, "page": ev.page, "level": level, "excluded": excluded})
        current = {"path": path, "page": ev.page, "rows": [], "excluded": excluded}
    finish()
    return sections, detected


def assign_ids(sections: List[dict]) -> List[dict]:
    seen: Dict[str, int] = {}
    out = []
    for s in sections:
        base = "/".join(slug(p) for p in s["path"])
        seen[base] = seen.get(base, 0) + 1
        sid = base if seen[base] == 1 else f"{base}-{seen[base]}"
        out.append({"id": sid, "path": s["path"], "page": s["page"], "html": s["html"]})
    return out


def check_outline(cfg: PackConfig, xml: str, headings: List[dict]):
    """Compare in-scope PDF outline entries with the sections built; None if the PDF has no outline."""
    items = outline_items(xml)
    if len(items) < MIN_OUTLINE_ITEMS:
        return None
    stack: List[tuple] = []
    rows = []
    for depth, title, page in items:
        while stack and stack[-1][0] >= depth:
            stack.pop()
        stack.append((depth, title))
        path = [t for d, t in stack if d >= cfg.outline_root_depth + 1]
        if depth <= cfg.outline_root_depth or page is None:
            continue
        rows.append((path, title, page))
    have = {(norm(h["path"][-1]), h["page"]) for h in headings if not h["excluded"]}
    have_titles = {norm(h["path"][-1]) for h in headings if not h["excluded"]}
    in_scope, matched, same_page, missing, skipped = 0, 0, 0, [], 0
    for path, title, page in rows:
        if is_excluded(cfg, path):
            skipped += 1
            continue
        in_scope += 1
        if (norm(title), page) in have:
            matched += 1
            same_page += 1
        elif norm(title) in have_titles:
            matched += 1  # the section exists under this title; the outline page differs from the heading's page
        else:
            missing.append((SEP.join(path), page))
    return {"total": len(rows), "skipped": skipped, "in_scope": in_scope, "matched": matched, "same_page": same_page, "missing": missing}


def check_text_layer(pdf: str, sections: List[dict]):
    """Independent check with pdftotext: every section heading must occur in the text of its page."""
    text = subprocess.run(["pdftotext", pdf, "-"], capture_output=True, check=True).stdout.decode("utf-8")
    pages = [norm(p) for p in text.split("\f")]
    bad = []
    for s in sections:
        page = pages[s["page"] - 1]
        if norm(s["path"][-1]) not in page:
            bad.append((SEP.join(s["path"]), s["page"]))
    return bad


def info_block(cfg: PackConfig, sha256: str) -> dict:
    return {
        "id": cfg.pack_id,
        "name": cfg.name,
        "version": VERSION,
        "license": "CC-BY-4.0",
        "attribution": cfg.attribution + " " + cfg.modification,
        "description": cfg.description,
        "source": {"document": cfg.document, "pdf": cfg.pdf_name, "pdf_sha256": sha256},
    }


def words(html_text: str) -> int:
    return len(re.sub(r"<[^>]+>", " ", html_text).split())


def write_attribution(cfg: PackConfig, path: str) -> None:
    body = f"""# Attribution and licensing

This pack (`{cfg.pack_id}`, ruleset `{cfg.ruleset}`) is part of Borealis and is derived solely from the System Reference Document {cfg.version_label}.

## Required attribution (CC-BY-4.0)

> {cfg.attribution}

## Modifications (CC-BY-4.0 section 3(a)(1)(B))

{cfg.modification}

## Scope

* The pack is rules text for offline search. It is not an element pack and is never loaded into the content store.
* No "Dungeons & Dragons", Wizards or D&D Beyond logos or trade dress are used. The pack may be described as "compatible with fifth edition".
* The CC-BY-4.0 Section 5 disclaimer of warranties and limitation of liability applies.
"""
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(body)


def write_coverage(cfg, path, sections, headings, outline, bad_text, n_events) -> None:
    kept = [h for h in headings if not h["excluded"]]
    by_level = {lv: sum(1 for h in kept if h["level"] == lv) for lv in (1, 2, 3, 4)}
    chapters: Dict[str, int] = {}
    for s in sections:
        chapters[s["path"][0]] = chapters.get(s["path"][0], 0) + 1
    empty = sum(1 for s in sections if not s["html"])
    pages = [s["page"] for s in sections]
    lines = [
        f"# {cfg.name}: coverage",
        "",
        f"Generated by `content-tools/rules-text/build.py` from `input/srd/{cfg.pdf_name}` (CC-BY-4.0). "
        "Rebuild: `python3 -I content-tools/rules-text/build.py` (byte-identical output).",
        "",
        "| Count | Value |",
        "|---|---|",
        f"| Sections in the pack (one per heading) | {len(sections)} |",
        f"| Headings detected in the PDF text layer (all scopes) | {len(headings)} |",
        f"| Headings in scope (kept) | {len(kept)} |",
        f"| Kept headings by level H1 / H2 / H3 / H4 | {by_level[1]} / {by_level[2]} / {by_level[3]} / {by_level[4]} |",
        f"| Sections with no body text (a heading directly followed by a sub-heading) | {empty} |",
        f"| Words in the pack | {sum(words(s['html']) for s in sections)} |",
        f"| Pages covered (first - last) | {min(pages)} - {max(pages)} |",
        "| Sections without page or path | 0 |",
        "",
    ]
    if outline is not None:
        pct = 100.0 * outline["matched"] / outline["in_scope"]
        lines += [
            "## Check against the PDF outline",
            "",
            f"The PDF carries a bookmark outline of {outline['total']} headings. {outline['skipped']} of them lie in "
            f"the excluded element chapters (below); {outline['in_scope']} are in scope.",
            "",
            f"**{outline['matched']} / {outline['in_scope']} in-scope outline headings are present as sections "
            f"({pct:.1f}%).** {outline['same_page']} of them on the same page as the outline bookmark "
            "(some bookmarks point at the page before or after the printed heading; the pack records the printed page).",
            "",
        ]
        if outline["missing"]:
            lines += ["Missing from the pack:", ""] + [f"* {p} (page {pg})" for p, pg in outline["missing"]] + [""]
    else:
        lines += [
            "## Check against the PDF outline",
            "",
            f"The {cfg.document} PDF has no bookmark outline (only the two attached licence documents), so there is "
            "nothing to compare against. Heading detection uses the typeset heading fonts instead (GillSans "
            "SemiBold, dark red, sizes 39/27/21/18 pt = H1-H4), the same rule used for the SRD 5.2.1 pack, where it "
            "reproduces the outline. Independent check below.",
            "",
        ]
    ok = len(sections) - len(bad_text)
    lines += [
        "## Check against a second text extractor",
        "",
        f"`pdftotext` (different code path from the XML read) finds the heading text on its recorded page for "
        f"**{ok} / {len(sections)}** sections.",
        "",
    ]
    if bad_text:
        lines += [f"* {p} (page {pg})" for p, pg in bad_text] + [""]
    lines += ["## Sections per chapter", "", "| Chapter | Sections |", "|---|---|"]
    lines += [f"| {c} | {n} |" for c, n in chapters.items()]
    lines += ["", "## Not included (modelled as typed elements in other packs)", ""]
    lines += [f"* {line}" for line in cfg.scope_notes]
    lines += ["", "## Known limits", ""]
    lines += [f"* {line}" for line in cfg.limits]
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines) + "\n")


def find_input_dir(explicit: str) -> str:
    if explicit:
        return explicit
    here = os.path.dirname(os.path.abspath(__file__))
    probe = here
    for _ in range(8):
        cand = os.path.join(probe, "input", "srd")
        if os.path.isdir(cand):
            return cand
        probe = os.path.dirname(probe)
    sys.exit("input/srd not found; pass --input-dir")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--input-dir", default=os.environ.get("BOREALIS_SRD_DIR", ""))
    ap.add_argument("--out-dir", default="")
    ap.add_argument("--only", default="")
    args = ap.parse_args()
    input_dir = find_input_dir(args.input_dir)
    out_root = args.out_dir or os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "content")
    for cfg in PACKS:
        if args.only and not cfg.pack_id.startswith(args.only):
            continue
        pdf = os.path.join(input_dir, cfg.pdf_name)
        with open(pdf, "rb") as fh:
            sha = hashlib.sha256(fh.read()).hexdigest()
        events, xml = document_events(pdf)
        sections, headings = build_sections(events, cfg)
        sections = assign_ids(sections)
        outline = check_outline(cfg, xml, headings)
        bad_text = check_text_layer(pdf, sections)
        doc = {"format": FORMAT, "info": info_block(cfg, sha), "ruleset": cfg.ruleset, "sections": sections}
        out = os.path.normpath(os.path.join(out_root, cfg.pack_id))
        os.makedirs(out, exist_ok=True)
        with open(os.path.join(out, "docs.json"), "w", encoding="utf-8", newline="\n") as fh:
            json.dump(doc, fh, ensure_ascii=False, indent=1)
            fh.write("\n")
        write_attribution(cfg, os.path.join(out, "ATTRIBUTION.md"))
        write_coverage(cfg, os.path.join(out, "COVERAGE.md"), sections, headings, outline, bad_text, len(events))
        if outline is not None:
            print(cfg.pack_id, "outline", outline["matched"], "/", outline["in_scope"])
        print(cfg.pack_id, "sections", len(sections), "text-layer misses", len(bad_text))


if __name__ == "__main__":
    main()
