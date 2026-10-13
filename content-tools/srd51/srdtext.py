"""Load and normalise the SRD 5.1 plain text (pdftotext raw output)."""
import re, os
HERE = os.path.dirname(os.path.abspath(__file__))

def _load():
    t = open(os.path.join(HERE, 'srd_clean.txt'), encoding='utf-8').read()
    t = t.replace('\x0c', '\n')
    t = t.replace('­', '').replace('‐', '-').replace('‑', '-')
    t = t.replace('-‑', '-')
    # strip running footers: "System Reference Document 5.1" + page number
    t = re.sub(r'\nSystem Reference Document 5\.1\n+\d+\n', '\n', t)
    t = re.sub(r'\nSystem Reference Document 5\.1\n', '\n', t)
    return t.split('\n')

LINES = _load()

def find(pattern, start=0, flags=0):
    """index of first line (>=start) fully matching regex pattern"""
    rx = re.compile(pattern, flags)
    for i in range(start, len(LINES)):
        if rx.fullmatch(LINES[i].strip()):
            return i
    return -1

def findall(pattern):
    rx = re.compile(pattern)
    return [i for i, l in enumerate(LINES) if rx.fullmatch(l.strip())]

def seg(a, b):
    return LINES[a:b]

END_PUNCT = tuple('.!?:)"”;')

def reflow(lines, width=None):
    """join hard-wrapped lines into paragraphs (list of str)."""
    lines = [l.strip() for l in lines]
    ls = [l for l in lines if l]
    if not ls:
        return []
    if width is None:
        lens = sorted(len(l) for l in ls)
        width = lens[int(len(lens) * 0.9)] if len(lens) > 3 else 50
    out = []
    cur = []
    prev_blank = False
    for l in lines:
        if not l:
            prev_blank = True
            continue
        newpara = False
        if cur:
            pl = cur[-1]
            runin = re.match(r"^([A-Z][A-Za-z’'\-/]*(?: (?:[A-Z][A-Za-z’'\-/]*|of|the|and|to|in|for|a|on|with|from))*)\.(?: |$)", l)
            if pl.endswith(END_PUNCT) and (len(pl) < width * 0.8):
                newpara = True
            elif runin and pl.endswith(END_PUNCT) and len(runin.group(1)) < 45:
                newpara = True
            elif l.startswith('•') or re.match(r'^\d+\.\s', l):
                newpara = True
        if newpara:
            out.append(' '.join(cur)); cur = []
        cur.append(l)
        prev_blank = False
    if cur:
        out.append(' '.join(cur))
    return [re.sub(r'\s+', ' ', p).strip() for p in out]

def esc(s):
    return s.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;')

def html_paras(paras, runin=True):
    """paragraphs -> HTML; leading 'Title.' run-in headings become <strong><em>."""
    out = []
    bullets = []
    def flush():
        nonlocal bullets
        if bullets:
            out.append('<ul>' + ''.join(f'<li>{b}</li>' for b in bullets) + '</ul>')
            bullets = []
    for p in paras:
        if p.startswith('•'):
            bullets.append(esc(p.lstrip('• ').strip())); continue
        flush()
        e = esc(p)
        if runin:
            m = re.match(r"^([A-Z][A-Za-z’'\-/]*(?: (?:[A-Z][A-Za-z’'\-/(]*\)?|of|the|and|to|in|for|a|on|with|from|at|by))*)\.( |$)", p)
            if m and len(m.group(1)) < 45:
                head = esc(m.group(1))
                e = f'<strong><em>{head}.</em></strong>' + esc(p[m.end(1)+1:])
        out.append(f'<p>{e}</p>')
    flush()
    return ''.join(out)

def block_html(lines, runin=True, width=None):
    return html_paras(reflow(lines, width), runin)
