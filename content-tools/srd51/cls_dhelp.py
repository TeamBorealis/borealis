"""Small helpers private to the Monk/Paladin/Ranger modules."""
import os, sys, json, re
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_core import *

_SPELLS = None
def spell_id(name):
    global _SPELLS
    if _SPELLS is None:
        _SPELLS = {v['name'].lower(): k for k, v in json.load(open(os.path.join(HERE, 'spell_index.json'))).items()}
    return _SPELLS[name.lower()]

def runin_split(i):
    """-> (intro_html, [(label, html)]) for heading i; paragraphs beginning with a bold-italic label start options."""
    intro, opts = [], []
    for e in body(i):
        m = re.match(r'^\s*<i><b>\s*(.*?)\s*</b></i>(.*)$', e['html'], flags=re.S)
        if m and e['k'] == 'P':
            e2 = dict(e); e2['html'] = m.group(2).strip()
            opts.append([clean(re.sub(r'<[^>]+>', '', m.group(1))).rstrip('.'), [e2]])
        elif opts:
            opts[-1][1].append(e)
        else:
            intro.append(e)
    return fx(to_html(intro)), [(l, fx(to_html(es))) for l, es in opts]

def names_table(fn):
    """feature names by level for class_table"""
    return fn

def fx(h):
    h = re.sub(r'([a-z,])</p><p>([a-z])', r'\1 \2', h)
    return re.sub(r'(\w)- ([a-z])', r'\1-\2', h).replace('( a )', '(a)').replace('( b )', '(b)')

SKIP_HEADS = ('Hit Points', 'Proficiencies', 'Equipment')
PROF_RX = re.compile(r'^(Armor|Weapons|Tools|Saving Throws|Skills):')

def feat2(R, name, size=21, after=None, include_sub=False, start=None):
    """Like sec() but robust to class-table page breaks: skips Hit Points/Proficiencies/Equipment blocks and
    proficiency lines that the PDF flow interleaves with feature text."""
    i = start if start is not None else find_in(R, name, size, after)
    own = ev[i]['size']
    state, chunk, out = 'feature', [], []
    def flush():
        nonlocal chunk
        if chunk:
            out.append(to_html(chunk)); chunk = []
    j = i + 1
    while j < len(ev):
        e = ev[j]
        if e['k'] == 'H':
            t = clean(e['text'])
            if e['size'] >= own and not (e['size'] == 18 and t in SKIP_HEADS):
                break
            if e['size'] == 18 and t in SKIP_HEADS:
                state = 'skip'
            else:
                if not include_sub:
                    break
                state = 'feature'
                if include_sub:
                    flush(); out.append(f'<h6>{esc_html(t)}</h6>')
        elif e['k'] == 'T':
            if state == 'skip':
                state = 'feature'
        elif e['k'] in ('P', 'B') and state == 'feature':
            if not (e['k'] == 'P' and PROF_RX.match(e['plain'])):
                chunk.append(e)
        j += 1
    flush()
    return fx(''.join(out))

def class_desc2(name, table_html=''):
    R = class_range(name)
    parts = [feature_html(find_in(R, 'Class Features', 27))]
    parts.append('<h5>HIT POINTS</h5>' + feature_html(find_in(R, 'Hit Points', 18)))
    profs = [e for k in range(*R) for e in [ev[k]] if e['k'] == 'P' and PROF_RX.match(e['plain'])]
    parts.append('<h5>PROFICIENCIES</h5>' + to_html(profs))
    i = find_in(R, 'Equipment', 18)
    eq = []
    j = i + 1
    while ev[j]['k'] in ('P', 'B'):
        eq.append(ev[j]); j += 1
    parts.append('<h5>EQUIPMENT</h5>' + to_html(eq))
    parts.append(table_html)
    return fx(''.join(parts))
