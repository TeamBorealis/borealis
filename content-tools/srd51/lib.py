"""Shared helpers for building the srd-5.1 content pack (clean-room, SRD 5.1 text only)."""
import json, os, pickle, re, html as _html
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, '..', '..', 'content', 'srd-5.1'))
SOURCE = 'SRD 5.1'

# ---------------------------------------------------------------- document events
_EV = None
def events():
    global _EV
    if _EV is None:
        _EV = pickle.load(open(os.path.join(HERE, 'events.pkl'), 'rb'))
    return _EV

def clean(s):
    s = s.replace('­', '').replace('---', '-').replace('--', '-')
    s = s.replace('\xa0', ' ')
    s = re.sub(r'\s+', ' ', s)
    return s.strip()

def find_head(text, size=None, after=0, before=None):
    """index of first heading event with this text (after index `after`)."""
    ev = events()
    end = before if before is not None else len(ev)
    for i in range(after, end):
        e = ev[i]
        if e['k'] == 'H' and clean(e['text']) == text and (size is None or e['size'] == size):
            return i
    return -1

def body(i, stop_sizes=None, until=None):
    """events after heading i up to next heading (any size, or sizes >= given)."""
    ev = events()
    out = []
    j = i + 1
    while j < len(ev):
        e = ev[j]
        if e['k'] == 'H':
            if stop_sizes is None or e['size'] in stop_sizes:
                break
        if until is not None and j >= until:
            break
        if e['k'] in ('P', 'B'):
            out.append(e)
        j += 1
    return out

def _inline(h):
    h = clean_inline(h)
    return h

def clean_inline(h):
    h = h.replace('\xa0', ' ')
    h = h.replace('---', '-').replace('­', '')
    h = re.sub(r'<b><i>(.*?)</i></b>', r'<strong><em>\1</em></strong>', h)
    h = re.sub(r'<i><b>(.*?)</b></i>', r'<strong><em>\1</em></strong>', h)
    h = h.replace('<b>', '<strong>').replace('</b>', '</strong>').replace('<i>', '<em>').replace('</i>', '</em>')
    h = re.sub(r'<(?!/?(strong|em)\b)[^>]*>', '', h)
    h = re.sub(r'\s+', ' ', h).strip()
    # merge adjacent identical tags
    h = h.replace('</strong> <strong>', ' ').replace('</em> <em>', ' ').replace('</strong><strong>', '').replace('</em><em>', '')
    h = re.sub(r'<(strong|em)>\s*</\1>', '', h)
    # fix whitespace inside tags
    h = re.sub(r'<(strong|em)> ', r' <\1>', h)
    h = re.sub(r' </(strong|em)>', r'</\1> ', h)
    h = re.sub(r'\s+', ' ', h).strip()
    return h

def to_html(evs):
    """list of P/B events -> html string"""
    out = []
    bullets = []
    def flush():
        nonlocal bullets
        if bullets:
            out.append('<ul>' + ''.join(f'<li>{b}</li>' for b in bullets) + '</ul>')
            bullets = []
    for e in evs:
        h = clean_inline(e['html'])
        if not h:
            continue
        if e['k'] == 'B':
            h = re.sub(r'^•\s*', '', h)
            bullets.append(h)
        else:
            flush()
            out.append(f'<p>{h}</p>')
    flush()
    return ''.join(out)

def text_of(evs):
    return clean(' '.join(e['plain'] for e in evs))

def section_html(head_text, size=None, after=0, stop_sizes=None):
    i = find_head(head_text, size, after)
    if i < 0:
        raise KeyError(f'heading not found: {head_text}')
    return to_html(body(i, stop_sizes))

# ---------------------------------------------------------------- fixture ids (names -> ids only)
_FIX = None
def fixture_rows():
    global _FIX
    if _FIX is None:
        _FIX = json.load(open(os.path.join(HERE, 'fixture_ids.json')))
    return _FIX

def norm_name(s):
    s = s.lower().replace('’', "'")
    return re.sub(r'[^a-z0-9]+', '', s)

def fixture_id(type_, name):
    """id the legacy SRD data used for (type, name), or None. Used only to align ids so imports resolve."""
    nn = norm_name(name)
    for f, t, i, n in fixture_rows():
        if t == type_ and norm_name(n) == nn:
            return i
    return None

def make_id(prefix, name):
    s = name.upper().replace('’', '').replace("'", '')
    s = re.sub(r'[^A-Z0-9]+', '_', s).strip('_')
    return f'{prefix}_{s}'

# ---------------------------------------------------------------- element builders
def setter(name, value, **attrs):
    d = {'name': name, 'value': str(value)}
    if attrs:
        d['attributes'] = {k: str(v) for k, v in attrs.items()}
    return d

def grant(id_, type_, level=None, requirements=None, **kw):
    d = {'kind': 'grant', 'id': id_, 'type': type_}
    if level is not None: d['level'] = level
    if requirements: d['requirements'] = requirements
    d.update({k: v for k, v in kw.items() if v is not None})
    return d

def select(type_, name, supports=None, number=None, level=None, requirements=None, optional=None, **kw):
    d = {'kind': 'select', 'type': type_, 'name': name}
    if number is not None: d['number'] = number
    if level is not None: d['level'] = level
    if supports: d['supports'] = supports
    if requirements: d['requirements'] = requirements
    if optional is not None: d['optional'] = optional
    d.update({k: v for k, v in kw.items() if v is not None})
    return d

def stat(name, value, bonus=None, level=None, requirements=None, equipped=None, maximum=None, alt=None, inline=None):
    d = {'kind': 'stat', 'name': name.lower(), 'value': str(value)}
    if bonus: d['bonus'] = bonus
    if level is not None: d['level'] = level
    if requirements: d['requirements'] = requirements
    if equipped: d['equipped'] = equipped
    if maximum is not None: d['maximum'] = str(maximum)
    if alt: d['alt'] = alt
    if inline: d['inline'] = True
    return d

def sheet(text=None, display=None, usage=None, action=None, alt=None, entries=None):
    d = {}
    if display is not None: d['display'] = display
    if alt: d['alt'] = alt
    if usage: d['usage'] = usage
    if action: d['action'] = action
    descs = list(entries or [])
    if text:
        descs.insert(0, {'text': text})
    if descs: d['descriptions'] = descs
    return d

def element(id_, name, type_, description='', supports=None, rules=None, setters=None,
            sheet=None, requirements=None, compendium=None, spellcasting=None, extract=None,
            prerequisite=None, multiclass=None):
    d = {'id': id_, 'name': name, 'type': type_, 'source': SOURCE}
    if supports: d['supports'] = list(supports)
    if prerequisite: d['prerequisite'] = prerequisite
    if requirements: d['requirements'] = requirements
    if description: d['description'] = description
    if sheet is not None: d['sheet'] = sheet
    if compendium is not None: d['compendium'] = compendium
    if setters: d['setters'] = setters
    if rules: d['rules'] = rules
    if spellcasting: d['spellcasting'] = spellcasting
    if multiclass: d['multiclass'] = multiclass
    if extract: d['extract'] = extract
    return d

def write_content(relpath, elements, name=None, description=None):
    path = os.path.join(OUT, relpath)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    doc = {'format': 'borealis-content/1', 'ruleset': 'dnd2014'}
    if name or description:
        info = {}
        if name: info['name'] = name
        if description: info['description'] = description
        doc['info'] = info
    doc['elements'] = elements
    with open(path, 'w', encoding='utf-8') as f:
        json.dump(doc, f, ensure_ascii=False, indent=2)
        f.write('\n')
    return path
