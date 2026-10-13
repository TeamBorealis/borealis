import sys, re, json, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

ev = events()
LISTS = find_head('Spell Lists', 27)
DESC = find_head('Spell Descriptions')
CLASSES = ['Bard', 'Cleric', 'Druid', 'Paladin', 'Ranger', 'Sorcerer', 'Warlock', 'Wizard']
ORD = {'1st': 1, '2nd': 2, '3rd': 3, '4th': 4, '5th': 5, '6th': 6, '7th': 7, '8th': 8, '9th': 9}
SCHOOLS = 'abjuration|conjuration|divination|enchantment|evocation|illusion|necromancy|transmutation'
HDR = re.compile(r'^(?:(\d)(?:st|nd|rd|th)-level (%s)|(%s) cantrip)(?: \((ritual)\))?$' % (SCHOOLS, SCHOOLS), re.I)

# ---- spell entries
entries = []
j = DESC + 1
while j < len(ev) and not (ev[j]['k'] == 'H' and ev[j]['size'] == 39):
    e = ev[j]
    if e['k'] == 'H' and e['size'] == 18 and j + 1 < len(ev) and ev[j+1]['k'] == 'P':
        m = HDR.match(clean(ev[j+1]['plain']))
        if m:
            entries.append((j, clean(e['text']), m))
    j += 1
END = j
names = [n for _, n, _ in entries]

# ---- class lists
def split_names(text, known):
    text = clean(text).replace("'", '’')
    toks = text.split(' ')
    n = len(toks)
    best = [None] * (n + 1); best[n] = []
    for s in range(n - 1, -1, -1):
        for L in range(min(6, n - s), 0, -1):
            nm = ' '.join(toks[s:s+L])
            if nm in known and best[s+L] is not None:
                best[s] = [nm] + best[s+L]; break
    return best[0]

class_of = {}
k = LISTS + 1
cur = None
nameset = set(names)
unparsed = []
while k < len(ev) and k < DESC:
    e = ev[k]
    if e['k'] == 'H' and e['size'] == 21 and e['text'].endswith(' Spells'):
        cur = e['text'].split()[0]
    elif e['k'] == 'P' and cur:
        r = split_names(e['plain'], nameset)
        if r is None:
            unparsed.append((cur, e['plain'][:80]))
        else:
            for nm in r: class_of.setdefault(nm, []).append(cur)
    k += 1

DUR = re.compile(r'(Instantaneous(?: or [\w ]+)?|Concentration,? up to (?:\d+|one) \w+|Up to (?:\d+|one) \w+|\d+ (?:round|minute|hour|day)s?(?: \(.*?\))?|Until dispelled(?: or triggered)?|Special)')
FAILS = []

def parse(name, hdr_m, idx):
    nxt = next((x for x in range(idx + 1, END) if ev[x]['k'] == 'H'), END)
    evs = [x for x in ev[idx+2:nxt] if x['k'] in ('P', 'B')]
    plain = ' '.join(x['plain'] for x in evs)
    level = int(hdr_m.group(1)) if hdr_m.group(1) else 0
    school = (hdr_m.group(2) or hdr_m.group(3)).capitalize()
    ritual = bool(hdr_m.group(4))
    # metadata via plain text of the first paragraphs
    flat = clean(plain)
    m = re.match(r'Casting Time: (.*?) Range: (.*?) Components?: (.*?) Duration:\s*(.*)$', flat)
    if not m:
        FAILS.append((name, 'meta')); return None
    ct, rng, comp, rest = m.groups()
    dm = DUR.match(rest)
    if not dm:
        FAILS.append((name, 'duration: ' + rest[:40])); return None
    dur = dm.group(1); rawdur = dur
    dur = dur.replace('Concentration up', 'Concentration, up')
    # components: V, S, M (...) -- M text may contain parens; trailing text belongs to duration anyway
    mat = ''
    mm = re.search(r'M \((.*)\)', comp)
    if mm: mat = mm.group(1)
    v = bool(re.search(r'\bV\b', comp.split('M (')[0])); s = bool(re.search(r'\bS\b', comp.split('M (')[0]))
    hasm = 'M' in re.sub(r'\(.*\)', '', comp).split(', ') or bool(mm)
    # body: drop metadata text from the event stream
    body_evs = []
    consumed = False
    skip_len = 0
    # locate end-of-metadata marker: the duration text
    marker = rawdur
    out = []
    state = 0
    for x in evs:
        h = clean_inline(x['html'])
        if state == 0:
            if 'Duration:' in x['plain'] or 'Duration:' in h:
                # find duration text in the html and keep what follows
                pos = h.find(marker, h.find('Duration:'))
                rem = h[pos + len(marker):].strip() if pos >= 0 else ''
                state = 1
                if rem:
                    out.append(dict(x, html=rem))
            continue
        out.append(x)
    desc = to_html(out).replace('<strong> <em>', '<strong><em>').replace('<em> ', '<em>')
    return dict(level=level, school=school, ritual=ritual, ct=ct.strip(), rng=rng.strip(),
                comp=comp, v=v, s=s, m=hasm, mat=mat, dur=dur, conc=dur.startswith('Concentration'), desc=desc)

by_level = {l: [] for l in range(10)}
index = {}
for idx, name, m in entries:
    d = parse(name, m, idx)
    if not d: continue
    cls = sorted(set(class_of.get(name, [])))
    sid = fixture_id('Spell', name) or make_id('ID_PHB_SPELL', name)
    kw = d['school'].lower()
    sets = [setter('keywords', kw), setter('level', d['level']), setter('school', d['school']),
            setter('time', d['ct']), setter('duration', d['dur']), setter('range', d['rng']),
            setter('hasVerbalComponent', str(d['v']).lower()), setter('hasSomaticComponent', str(d['s']).lower()),
            setter('hasMaterialComponent', str(d['m']).lower())]
    if d['mat']: sets.append(setter('materialComponent', d['mat']))
    sets += [setter('isConcentration', str(d['conc']).lower()), setter('isRitual', str(d['ritual']).lower())]
    el = element(sid, name, 'Spell', d['desc'], supports=cls, setters=sets)
    by_level[d['level']].append(el)
    index[sid] = dict(name=name, level=d['level'], classes=cls)
    if not cls: FAILS.append((name, 'no class'))

for l, els in by_level.items():
    write_content(f'spells/spells-{l}.json', els)
json.dump(index, open(os.path.join(HERE, 'spell_index.json'), 'w'), indent=1)
print('entries', len(entries), 'written', len(index))
print('per level', {l: len(v) for l, v in by_level.items()})
print('unparsed lists', unparsed)
print('FAILS', FAILS)
