"""Turn the text rows of one section into a small HTML fragment: <p>, <ul><li>, <table>.

Paragraph breaks come from the typesetting (a bold run-in head, an indented first line, a blank line, a short
last line); wrapped lines are joined and line-end hyphens are resolved against the document's own vocabulary.
"""
import html
import re
from typing import Dict, List, Set

from pdfxml import HARD_HYPHEN, Row, Seg

BULLET = "•"
INDENT_PX = 8
GAP_FACTOR = 1.45
LINE_PITCH = 1.27  # line pitch as a multiple of the font size
SHORT_LAST_LINE = 0.75
SENTENCE_END = tuple(".!?:)”\"")
WORD = re.compile(r"[A-Za-z]+")
COMPOUND_MIN_PART = 4  # shorter parts (mem, un) are word pieces, not words, when a wrap splits them
SMALLCAPS_SIZES = {17, 12}
CAPTION_MIN_SIZE = 16


class Vocabulary:
    """Words seen inside lines (not across a wrap) used to decide whether a wrap hyphen is real."""

    def __init__(self) -> None:
        self.words: Set[str] = set()
        self.compounds: Set[str] = set()

    def add_row(self, row: Row) -> None:
        text = row_text(row)
        for w in re.findall(r"[A-Za-z]+(?:-[A-Za-z]+)*", text.replace(HARD_HYPHEN, "-")):
            if "-" in w:
                self.compounds.add(w.lower())
            else:
                self.words.add(w.lower())

    def keep_hyphen(self, left: str, right: str) -> bool:
        merged = (left + right).lower()
        if merged in self.words:
            return False
        if f"{left}-{right}".lower() in self.compounds:
            return True
        if min(len(left), len(right)) >= COMPOUND_MIN_PART and left.lower() in self.words and right.lower() in self.words:
            return True  # two real words (real-world): a compound the wrap split
        return not right[:1].islower()


def row_segs(row: Row) -> List[Seg]:
    segs: List[Seg] = []
    prev_right = None
    for f in row.frags:
        if prev_right is not None and f.left - prev_right > 2:
            if not (segs and segs[-1].text.endswith(" ")) and not f.segs[0].text.startswith(" "):
                segs.append(Seg(" "))
        segs.extend(f.segs)
        prev_right = f.right
    return segs


def row_text(row: Row) -> str:
    return "".join(s.text for s in row_segs(row)).strip()


def _is_bullet(row: Row) -> bool:
    return row_text(row).startswith(BULLET)


def _small_caps_title(row: Row) -> bool:
    fs = row.frags
    return (
        len(fs) > 2
        and all(f.family.startswith("GillSans") and f.bold for f in fs)
        and {f.size for f in fs} <= SMALLCAPS_SIZES
        and len({f.size for f in fs}) == 2
    )


def _decode_small_caps(row: Row) -> str:
    """Small-capital titles print capitals at the big size and the rest at the small size, in capital glyphs."""
    out = []
    prev_right = None
    for f in row.frags:
        t = f.text
        if prev_right is not None and f.left - prev_right > 2 and not t.startswith(" ") and not "".join(out).endswith(" "):
            out.append(" ")
        prev_right = f.right
        if f.size == min(SMALLCAPS_SIZES):
            out.append(t.lower())
        else:
            stripped = t.lstrip()
            out.append(t[: len(t) - len(stripped)] + stripped[:1].upper() + stripped[1:])
    return "".join(out).strip()


def _is_caption(row: Row) -> bool:
    f = row.frags[0]
    return len(row.frags) == 1 and f.bold and f.size >= CAPTION_MIN_SIZE


def _coalesce(segs: List[Seg]) -> str:
    merged: List[Seg] = []
    for s in segs:
        if not s.text:
            continue
        if merged and merged[-1].bold == s.bold and merged[-1].italic == s.italic:
            merged[-1] = Seg(merged[-1].text + s.text, s.bold, s.italic)
        else:
            merged.append(s)
    parts = []
    for i, s in enumerate(merged):
        text = re.sub(r" {2,}", " ", s.text)
        if i == 0:
            text = text.lstrip()
        if i == len(merged) - 1:
            text = text.rstrip()
        if not text.strip():
            parts.append(text)
            continue
        esc = html.escape(text.replace(HARD_HYPHEN, "-"), quote=False)
        lead, core, trail = re.match(r"^(\s*)(.*?)(\s*)$", esc, re.S).groups()
        if s.bold:
            core = f"<b>{core}</b>"
        if s.italic:
            core = f"<i>{core}</i>"
        parts.append(lead + core + trail)
    return "".join(parts).strip()


class _Paragraph:
    def __init__(self, vocab: Vocabulary) -> None:
        self.segs: List[Seg] = []
        self.vocab = vocab
        self.last_row: Row = None

    def add(self, row: Row, strip_bullet: bool = False) -> None:
        segs = list(row_segs(row))
        if strip_bullet:
            segs = _strip_leading(segs, BULLET)
        self.last_row = row
        if not self.segs:
            self.segs = segs
            return
        self._join(segs)

    def _join(self, segs: List[Seg]) -> None:
        last = self.segs[-1]
        text = last.text.rstrip()
        nxt = "".join(s.text for s in segs).lstrip()
        if text.endswith(HARD_HYPHEN):
            self.segs[-1] = Seg(text, last.bold, last.italic)
            self.segs.extend(_lstrip(segs))
            return
        m = re.search(r"([A-Za-z]+)-$", text)
        r = WORD.match(nxt)
        if m and r:
            if self.vocab.keep_hyphen(m[1], r[0]):
                self.segs[-1] = Seg(text, last.bold, last.italic)
            else:
                self.segs[-1] = Seg(text[:-1], last.bold, last.italic)
            self.segs.extend(_lstrip(segs))
            return
        self.segs[-1] = Seg(text + " ", last.bold, last.italic)
        self.segs.extend(_lstrip(segs))

    def html(self) -> str:
        return _coalesce(self.segs)


def _strip_leading(segs: List[Seg], char: str) -> List[Seg]:
    out, done = [], False
    for s in segs:
        if not done and s.text.strip():
            out.append(Seg(s.text.lstrip().lstrip(char).lstrip(), s.bold, s.italic))
            done = True
        elif done or s.text.strip():
            out.append(s)
    return out


def _lstrip(segs: List[Seg]) -> List[Seg]:
    out, done = [], False
    for s in segs:
        if not done and s.text.strip():
            out.append(Seg(s.text.lstrip(), s.bold, s.italic))
            done = True
        elif done:
            out.append(s)
    return out


def _continues_bold(prev: Row, row: Row) -> bool:
    last = next((s for s in reversed(row_segs(prev)) if s.text.strip()), None)
    return bool(last and last.bold)


def _starts_bold(row: Row) -> bool:
    first = next((s for s in row_segs(row) if s.text.strip()), None)
    return bool(first and first.bold)


def _continues_text(prev: Row, row: Row) -> bool:
    """A lone plain fragment in the same font as the running text above is the last line of that paragraph."""
    if prev is None or not prev.is_text or len(row.frags) != 1 or row.frags[0].bold:
        return False
    a, b = prev.frags[-1], row.frags[0]
    return a.family == b.family and a.size == b.size and not _is_bullet(row)


def _starts_new_paragraph(prev: Row, row: Row, max_width: Dict[int, int], indent_rule: bool) -> bool:
    if _starts_bold(row) and not _continues_bold(prev, row):
        return True
    same_column = prev.page == row.page and abs(prev.left - row.left) < 200
    if same_column and row.top < prev.top:
        same_column = False
    if indent_rule and row.left - row.base >= INDENT_PX and same_column:
        return True
    if same_column:
        pitch = LINE_PITCH * row.frags[0].size
        if row.top - prev.top > GAP_FACTOR * pitch:
            return True
    text = row_text(prev)
    width = max_width.get(prev.page, 0)
    if (
        width
        and prev.right - prev.base < SHORT_LAST_LINE * width
        and text.endswith(SENTENCE_END)
        and row_text(row)[:1].isupper()
    ):
        return True
    return False


def render(rows: List[Row], vocab: Vocabulary, indent_rule: bool) -> str:
    """HTML for the rows between two headings (reading order)."""
    max_width: Dict[int, int] = {}
    for r in rows:
        if r.is_text:
            max_width[r.page] = max(max_width.get(r.page, 0), r.right - r.base)
    out: List[str] = []
    state = {"para": None, "item": None}
    items: List[_Paragraph] = []
    table: List[Row] = []

    def close_para():
        if state["para"] is not None:
            h = state["para"].html()
            if h:
                out.append(f"<p>{h}</p>")
            state["para"] = None

    def close_items():
        if items:
            out.append("<ul>" + "".join(f"<li>{i.html()}</li>" for i in items) + "</ul>")
            items.clear()
        state["item"] = None

    def close_table():
        if table:
            out.append(_table_html(table))
            table.clear()

    prev: Row = None
    for row in rows:
        text_row = row.is_text or _continues_text(prev, row)
        if _small_caps_title(row) or not text_row:
            close_para()
            close_items()
            if _small_caps_title(row):
                close_table()
                out.append(f"<p><b>{html.escape(_decode_small_caps(row), quote=False)}</b></p>")
            elif _is_caption(row):
                close_table()
                out.append(f"<p><b>{html.escape(row_text(row).replace(HARD_HYPHEN, '-'), quote=False)}</b></p>")
            else:
                table.append(row)
            prev = row
            continue
        close_table()
        if _is_bullet(row):
            close_para()
            item = _Paragraph(vocab)
            item.add(row, strip_bullet=True)
            items.append(item)
            state["item"] = item
        else:
            hanging = state["item"] is not None  # list continuation lines are indented under the bullet text
            new = prev is None or not prev.is_text or _starts_new_paragraph(
                prev, row, max_width, indent_rule and not hanging
            )
            if state["item"] is not None and not new:
                state["item"].add(row)
            else:
                close_items()
                if state["para"] is None or new:
                    close_para()
                    state["para"] = _Paragraph(vocab)
                state["para"].add(row)
        prev = row
    close_para()
    close_items()
    close_table()
    return "".join(out)


def _table_html(rows: List[Row]) -> str:
    lines = []
    for r in rows:
        tag = "th" if all(f.bold for f in r.frags) else "td"
        cells = "".join(f"<{tag}>{_coalesce(f.segs)}</{tag}>" for f in r.frags)
        lines.append(f"<tr>{cells}</tr>")
    return "<table>" + "".join(lines) + "</table>"
