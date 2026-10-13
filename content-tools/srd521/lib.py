"""Element/rule construction helpers for the srd-5.2.1 pack (all dnd2024)."""
import sys, os, json, re
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *

SRC = 'SRD 5.2.1'
RS = 'dnd2024'

def el(id, name, type, description='', supports=None, rules=None, setters=None, **kw):
    e = {'id': id, 'name': name, 'type': type, 'source': SRC, 'ruleset': RS}
    if supports: e['supports'] = list(supports)
    if description: e['description'] = description
    if setters: e['setters'] = setters
    if rules: e['rules'] = rules
    for k, v in kw.items():
        if v is not None: e[k] = v
    return e

def setter(name, value, **attrs):
    s = {'name': name, 'value': str(value)}
    if attrs: s['attributes'] = {k: str(v) for k, v in attrs.items()}
    return s

def grant(id, type, level=None, requirements=None, **kw):
    r = {'kind': 'grant', 'id': id, 'type': type}
    if level and level > 1: r['level'] = level
    if requirements: r['requirements'] = requirements
    r.update(kw)
    return r

def select(type, name, supports=None, number=None, level=None, requirements=None, optional=None, **kw):
    r = {'kind': 'select', 'type': type, 'name': name}
    if number and number != 1: r['number'] = number
    if level and level > 1: r['level'] = level
    if supports: r['supports'] = supports
    if requirements: r['requirements'] = requirements
    if optional: r['optional'] = True
    r.update(kw)
    return r

def stat(name, value, bonus=None, level=None, requirements=None, equipped=None, alt=None, maximum=None, inline=None):
    r = {'kind': 'stat', 'name': name, 'value': str(value)}
    if bonus: r['bonus'] = bonus
    if level and level > 1: r['level'] = level
    if requirements: r['requirements'] = requirements
    if equipped: r['equipped'] = equipped
    if alt: r['alt'] = alt
    if maximum is not None: r['maximum'] = str(maximum)
    if inline: r['inline'] = True
    return r

def custom(kind, level=None, requirements=None, **kw):
    r = {'kind': kind}
    if level and level > 1: r['level'] = level
    if requirements: r['requirements'] = requirements
    r.update(kw)
    return r

def html_from_lines(lines):
    return chunk_html2(lines)

RUNIN = re.compile(r"^((?:[A-Z][\w’'\-]*|of|the|a|an|and|or|to|in|for|with|your|on)(?: (?:[A-Z][\w’'\-]*|of|the|a|an|and|or|to|in|for|with|your|on|\([^)]*\))){0,6}\.) (?=[A-Z“\"(0-9])")
def bold_runin(p):
    m = RUNIN.match(p)
    if m and len(m.group(1).split()) <= 8 and not m.group(1).startswith(('You ','The ','This ','That ')):
        return '<b>' + esc(m.group(1)) + '</b> ' + esc(p[m.end():])
    return esc(p)
def para_html(paras):
    out = []; ul = []
    for p in paras:
        if p.startswith('•'):
            ul.append('<li>' + esc(p.lstrip('• ').strip()) + '</li>')
            continue
        if ul: out.append('<ul>' + ''.join(ul) + '</ul>'); ul = []
        out.append('<p>' + bold_runin(p) + '</p>')
    if ul: out.append('<ul>' + ''.join(ul) + '</ul>')
    return '\n'.join(out)

def paragraphs2(lines, headrx=r"^[A-Z][A-Za-z’' ,\-/()0-9]{1,60}\. "):
    paras = []; cur = []
    for ln in lines:
        if not ln.strip():
            if cur: paras.append(cur); cur = []
            continue
        ind = len(ln) - len(ln.lstrip()); st = ln.strip()
        newp = False
        if cur:
            pend = cur[-1].rstrip()
            if st.startswith('•'): newp = True
            elif cur[0].lstrip().startswith('•') and ind == 0 and pend.endswith(TERM): newp = True
            elif pend.endswith(TERM):
                if 1 <= ind <= 4: newp = True
                elif re.match(headrx, st) and ind == 0: newp = True
        if newp: paras.append(cur); cur = []
        cur.append(ln)
    if cur: paras.append(cur)
    return [join_lines(p) for p in paras]

def _is_tab(line):
    return bool(re.search(r'\S {3,}\S', line.strip('\n'))) 
def split_segments(ch):
    """split a blank-line-free chunk into ('text'|'table', lines) segments"""
    segs = []; cur = []; kind = None; prev_tab = False
    for ln in ch:
        ind = len(ln) - len(ln.lstrip())
        st = ln.strip()
        tab = _is_tab(ln) and not st.startswith('•')
        if not tab and prev_tab and (re.match(r'^\d{1,3}([–-]\d{1,3})?\s+\S', st) or ind >= 5):
            tab = True    # numbered row / wrapped cell of the current table
        if not tab and not prev_tab and re.match(r'^\d{1,3}([–-]\d{1,3})?\s{1,2}[A-Z]', st) and ind >= 1 and False:
            tab = True
        k = 'table' if tab else 'text'
        if k != kind and cur:
            segs.append((kind, cur)); cur = []
        kind = k; cur.append(ln); prev_tab = tab
    if cur: segs.append((kind, cur))
    return segs
def chunk_html2(lines):
    """lines -> html; prose is reflowed into paragraphs, table runs are kept verbatim in <pre>"""
    chunks = []; cur = []
    for ln in lines:
        if not ln.strip():
            if cur: chunks.append(cur); cur = []
        else: cur.append(ln)
    if cur: chunks.append(cur)
    html = []; textbuf = []
    def flush():
        nonlocal textbuf
        if textbuf:
            html.append(para_html(paragraphs2(textbuf))); textbuf = []
    for ch in chunks:
        for kind, seg in split_segments(ch):
            if kind == 'table':
                flush(); html.append('<pre>' + esc('\n'.join(seg)) + '</pre>')
            else:
                textbuf.extend(seg)
        # a blank line inside prose (page/column break) does not end the paragraph; keep buffer
    flush()
    return '\n'.join(h for h in html if h)

OUT = {}   # relative path -> list of elements
def emit(path, e):
    OUT.setdefault(path, []).append(e)

def idof(kind, *parts):
    return 'ID_2024_' + kind + '_' + '_'.join(slug(p) for p in parts)

CODE_ABIL = {'Strength': 'strength', 'Dexterity': 'dexterity', 'Constitution': 'constitution',
             'Intelligence': 'intelligence', 'Wisdom': 'wisdom', 'Charisma': 'charisma'}
SKILL_ID = lambda n: 'ID_PROFICIENCY_SKILL_' + n.upper().replace(' ', '')
SAVE_ID = lambda n: 'ID_PROFICIENCY_SAVINGTHROW_' + n.upper()
ALL_SKILLS = ['Acrobatics','Animal Handling','Arcana','Athletics','Deception','History','Insight','Intimidation',
              'Investigation','Medicine','Nature','Perception','Performance','Persuasion','Religion','Sleight of Hand','Stealth','Survival']
def skill_range(names):
    return '|'.join(SKILL_ID(n) for n in names)
