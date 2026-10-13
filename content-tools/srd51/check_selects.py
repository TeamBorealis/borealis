"""Semantic check: every select rule must have at least one candidate (core + srd-5.1 loaded).
Approximate supports evaluator (comma/&& = AND, ||/| = OR, ! = NOT, ID-range mode) with derived tags."""
import json, os, re, sys, glob
HERE = os.path.dirname(os.path.abspath(__file__))
CONTENT = os.path.normpath(os.path.join(HERE, '..', '..', 'content'))

def load():
    els = []
    for pack in ('core', 'srd-5.1'):
        man = json.load(open(os.path.join(CONTENT, pack, 'pack.json')))
        for f in man['files']:
            els += json.load(open(os.path.join(CONTENT, pack, f['path'])))['elements']
    return els

def tags_of(e, by_id):
    t = list(e.get('supports', []))
    sup_names = []
    for x in t:
        if x.startswith('ID_INTERNAL_SUPPORT') and x in by_id:
            sup_names.append(by_id[x]['name'])
    t += sup_names
    st = {s['name'].lower(): s['value'] for s in e.get('setters', [])}
    if e['type'] == 'Spell':
        t += [st.get('level', ''), st.get('school', ''), st.get('school', '').lower()]
        if st.get('isritual') == 'true': t += ['Ritual']
    if e['type'] in ('Weapon', 'Armor'):
        t += [e['name'], e['name'].lower()]
    if e['type'] == 'Language':
        for k in ('standard', 'exotic', 'secret', 'monster'):
            if k in st: t.append(k.capitalize())
    return set(t)

def sanitize(s):
    s = s.replace(',', '&&').replace('&amp;', '&')
    s = re.sub(r'&+', '&&', s)
    s = re.sub(r'\|+', '||', s)
    return s.strip()

def evaluate(expr, tags, eid, idmode):
    toks = re.findall(r'\|\||&&|!|\(|\)|[^|&!()]+', expr)
    pos = 0
    def peek(): return toks[pos] if pos < len(toks) else None
    def parse_or():
        nonlocal pos
        v = parse_and()
        while peek() == '||':
            pos += 1; r = parse_and(); v = v or r
        return v
    def parse_and():
        nonlocal pos
        v = parse_un()
        while peek() == '&&':
            pos += 1; r = parse_un(); v = v and r
        return v
    def parse_un():
        nonlocal pos
        t = peek()
        if t == '!':
            pos += 1; return not parse_un()
        if t == '(':
            pos += 1; v = parse_or(); pos += 1; return v
        pos += 1
        t = t.strip()
        if idmode and t.startswith('ID_'):
            return t == eid
        return t in tags
    return parse_or()

def main():
    els = load()
    by_id = {e['id']: e for e in els}
    by_type = {}
    for e in els:
        by_type.setdefault(e['type'], []).append(e)
    tagcache = {e['id']: tags_of(e, by_id) for e in els}
    bad = 0
    for e in els:
        if e['source'] in ('Core', 'Internal') and not e['id'].startswith('ID_SRD'):
            continue
        for r in e.get('rules', []) + e.get('multiclass', {}).get('rules', []):
            if r['kind'] != 'select' or r['type'] == 'List':
                continue
            sup = r.get('supports')
            if sup and sup.startswith('Improvement Option'):
                continue  # candidates are synthesized by the loader (content-format 10.3)
            cands = by_type.get(r['type'], [])
            if sup and '$(' in sup:
                sup = re.sub(r'\$\([^)]*\)', '', sup)
                sup = re.sub(r'^[\s,|&]+|[\s,|&]+$', '', sup)
                if not sup or sup == '0':
                    sup = sup or None
            if sup:
                raw = sup
                idmode = '|' in raw and '||' not in raw
                ex = sanitize(raw)
                n = sum(1 for c in cands if evaluate(ex, tagcache[c['id']], c['id'], idmode))
            else:
                n = len(cands)
            if n == 0:
                bad += 1
                print(f'NO CANDIDATES: {e["id"]} select {r["type"]!r} {r["name"]!r} supports={r.get("supports")!r}')
    print('selects without candidates:', bad)
    return bad

if __name__ == '__main__':
    sys.exit(1 if main() else 0)
