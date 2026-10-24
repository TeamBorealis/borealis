"""Read an SRD PDF through poppler's `pdftohtml -xml` and turn each page into reading-order events.

Deterministic, no AI: the PDF text layer (fragments with font, size, colour and position) is the only input.
An event is either a heading (`kind == "h"`) or a text row (`kind == "row"`).
"""
import html
import re
import subprocess
from dataclasses import dataclass, field
from typing import List, Optional, Tuple

# Heading fonts: both SRDs print headings in GillSans-SemiBold, dark red. Size gives the level.
HEADING_COLOURS = {"#943634", "#88191f"}
HEADING_SIZES = {39: 1, 27: 2, 21: 3, 18: 4}
FOOTER_BAND = 75  # fragments within this many px of the page bottom are running footers
BODY_FAMILY = "Cambria"
TEXT_MAX_GAP = 10  # px between fragments of one running-text line; wider gaps separate table cells
TEXT_MIN_WIDTH = 200
ROW_TOLERANCE = 7  # px: fragments whose tops differ by less belong to one text row
WIDE_MARGIN = 30  # a fragment crossing the page middle by this much spans both columns
# Soft/real hyphen glyph sequences used by the SRD 5.1 typesetting for hyphens inside words.
HYPHEN_CHARS = "­‐‑"
HARD_HYPHEN = "‑"  # internal marker: a real hyphen at a line end (never dropped)


@dataclass(frozen=True)
class Seg:
    text: str
    bold: bool = False
    italic: bool = False


@dataclass
class Frag:
    top: int
    left: int
    width: int
    height: int
    size: int
    family: str
    colour: str
    segs: List[Seg]
    page: int

    @property
    def text(self) -> str:
        return "".join(s.text for s in self.segs)

    @property
    def bold(self) -> bool:
        return any(s.bold for s in self.segs if s.text.strip())

    @property
    def right(self) -> int:
        return self.left + self.width


@dataclass
class Row:
    """One visual text line (all fragments sharing a baseline inside one column band)."""

    frags: List[Frag]
    page: int
    kind: str = "row"
    base: int = 0  # left edge of the column the row sits in

    @property
    def top(self) -> int:
        return self.frags[0].top

    @property
    def left(self) -> int:
        return self.frags[0].left

    @property
    def right(self) -> int:
        return max(f.right for f in self.frags)

    @property
    def is_text(self) -> bool:
        """Running text (body font, a bullet, or a wide unbroken line in another font) as opposed to a table row."""
        if self.frags[0].text.strip().startswith("\u2022"):
            return True
        if all(f.family.startswith(BODY_FAMILY) or f.family.startswith("Symbol") for f in self.frags):
            return True
        gaps = [b.left - a.right for a, b in zip(self.frags, self.frags[1:])]
        return max(gaps, default=0) <= TEXT_MAX_GAP and self.right - self.left >= TEXT_MIN_WIDTH


@dataclass
class Heading:
    level: int
    text: str
    page: int
    top: int
    size: int
    kind: str = "h"
    last_top: int = 0  # top of the last wrapped line of the title


def run_pdftohtml(pdf_path: str) -> str:
    out = subprocess.run(
        ["pdftohtml", "-xml", "-i", "-q", "-stdout", pdf_path],
        capture_output=True,
        check=True,
    )
    return out.stdout.decode("utf-8")


_FONT = re.compile(r'<fontspec id="(\d+)" size="(\d+)" family="([^"]*)" color="([^"]*)"')
_PAGE = re.compile(r'<page number="(\d+)"[^>]*height="(\d+)" width="(\d+)">(.*?)</page>', re.S)
_TEXT = re.compile(
    r'<text top="(-?\d+)" left="(-?\d+)" width="(\d+)" height="(\d+)" font="(\d+)">(.*?)</text>', re.S
)
_TAG = re.compile(r"<(/?)(b|i)>|<[^>]+>")


def _segments(inner: str) -> List[Seg]:
    segs: List[Seg] = []
    bold = italic = False
    pos = 0
    for m in _TAG.finditer(inner):
        if m.start() > pos:
            segs.append(Seg(html.unescape(inner[pos : m.start()]), bold, italic))
        if m.group(2) == "b":
            bold = m.group(1) == ""
        elif m.group(2) == "i":
            italic = m.group(1) == ""
        pos = m.end()
    if pos < len(inner):
        segs.append(Seg(html.unescape(inner[pos:]), bold, italic))
    return segs


def clean_text(text: str) -> str:
    text = re.sub("-?[" + HYPHEN_CHARS + "]+", HARD_HYPHEN, text)
    text = text.replace("̶", "—").replace("​", "")
    return re.sub(r"[  \t]+", " ", text)


def parse_pages(xml: str):
    """Return (pages, outline_xml). pages: list of (number, height, width, [Frag])."""
    fonts = {m[1]: (int(m[2]), m[3].split("+")[-1], m[4]) for m in _FONT.finditer(xml)}
    pages = []
    for pm in _PAGE.finditer(xml):
        number, height, width = int(pm[1]), int(pm[2]), int(pm[3])
        frags = []
        for t in _TEXT.finditer(pm[4]):
            size, family, colour = fonts[t[5]]
            segs = [Seg(clean_text(s.text), s.bold, s.italic) for s in _segments(t[6])]
            if not "".join(s.text for s in segs).strip():
                continue
            frags.append(Frag(int(t[1]), int(t[2]), int(t[3]), int(t[4]), size, family, colour, segs, number))
        pages.append((number, height, width, frags))
    return pages


def _is_heading(f: Frag) -> bool:
    return (
        f.family.startswith("GillSans-SemiBold")
        and "SC700" not in f.family
        and f.colour in HEADING_COLOURS
        and f.size in HEADING_SIZES
        and f.bold
    )


def _rows(frags: List[Frag], page: int) -> list:
    """Group the fragments of one column band into rows and headings, top to bottom."""
    out: list = []
    current: List[Frag] = []

    def flush():
        nonlocal current
        if current:
            current.sort(key=lambda f: f.left)
            out.append(Row(current, page))
            current = []

    for f in sorted(frags, key=lambda f: (f.top, f.left)):
        if _is_heading(f):
            flush()
            out.append(Heading(HEADING_SIZES[f.size], f.text.strip(), page, f.top, f.size))
            continue
        if current and abs(f.top - current[0].top) > ROW_TOLERANCE:
            flush()
        current.append(f)
    flush()
    return out


def page_events(number: int, height: int, width: int, frags: List[Frag]) -> list:
    """Reading-order events of one page: bands split at full-width fragments, left column then right."""
    mid = width // 2
    body = [f for f in frags if f.top < height - FOOTER_BAND]
    wide = sorted((f for f in body if f.left < mid - WIDE_MARGIN and f.right > mid + WIDE_MARGIN), key=lambda f: f.top)
    wide_ids = {id(f) for f in wide}
    bands: list = [[] for _ in range(len(wide) + 1)]
    for f in body:
        if id(f) in wide_ids:
            continue
        index = sum(1 for w in wide if w.top <= f.top)
        bands[index].append(f)
    events: list = []
    for i, band in enumerate(bands):
        for column in (
            [f for f in band if f.left + f.width / 2 < mid],
            [f for f in band if f.left + f.width / 2 >= mid],
        ):
            col_events = _rows(column, number)
            rows = [e for e in col_events if e.kind == "row" and e.is_text]
            base = min((r.left for r in rows), default=0)
            for e in col_events:
                if e.kind == "row":
                    e.base = base
            events.extend(col_events)
        if i < len(wide):
            for e in _rows([wide[i]], number):
                if e.kind == "row":
                    e.base = e.left
                events.append(e)
    return merge_headings(events)


def merge_headings(events: list) -> list:
    """Join consecutive heading lines of one size (titles wrapped over two lines)."""
    merged: list = []
    for ev in events:
        prev: Optional[object] = merged[-1] if merged else None
        if (
            ev.kind == "h"
            and prev is not None
            and prev.kind == "h"
            and prev.size == ev.size
            and 0 < ev.top - (prev.last_top or prev.top) <= prev.size * 1.6
        ):
            merged[-1] = Heading(prev.level, f"{prev.text} {ev.text}", prev.page, prev.top, prev.size, "h", ev.top)
        else:
            merged.append(ev)
    return merged


def document_events(pdf_path: str) -> Tuple[list, str]:
    xml = run_pdftohtml(pdf_path)
    events: list = []
    for number, height, width, frags in parse_pages(xml):
        events.extend(page_events(number, height, width, frags))
    return events, xml


def outline_items(xml: str) -> List[Tuple[int, str, Optional[int]]]:
    """The PDF bookmark tree as (depth, title, page); empty if the PDF has no outline."""
    start = xml.find("<outline>")
    if start < 0:
        return []
    items, depth = [], 0
    for line in xml[start:].splitlines():
        if line.startswith("<outline"):
            depth += 1
        elif line.startswith("</outline"):
            depth -= 1
        elif line.startswith("<item"):
            m = re.match(r'<item(?: page="(\d+)")?>(.*)</item>', line)
            if m:
                items.append((depth, html.unescape(m[2]), int(m[1]) if m[1] else None))
    return items
