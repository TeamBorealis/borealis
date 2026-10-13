"""Structured extraction from pdftohtml -xml output of the SRD 5.1 PDF.

Produces, per page, an ordered list of Line records (left column then right column)."""
import re, os, json, html
import xml.etree.ElementTree as ET
HERE = os.path.dirname(os.path.abspath(__file__))
RIGHT_X = 480       # items with left >= RIGHT_X belong to the right column
LEFT_MARGIN, RIGHT_MARGIN = 86, 492

def _inner(t):
    s = ET.tostring(t, encoding='unicode')
    s = re.sub(r'^<text[^>]*>', '', s); s = re.sub(r'</text>\s*$', '', s)
    return s

def load():
    root = ET.parse(os.path.join(HERE, 'srd.xml')).getroot()
    fs = {}
    for f in root.iter('fontspec'):
        fs.setdefault(f.get('id'), (int(f.get('size')), f.get('family').split('+')[-1], f.get('color')))
    pages = []
    for pg in root.findall('page'):
        n = int(pg.get('number'))
        items = []
        for t in pg.findall('text'):
            size, fam, color = fs[t.get('font')]
            inner = _inner(t)
            plain = html.unescape(re.sub(r'<[^>]+>', '', inner))
            if not plain.strip():
                continue
            items.append(dict(top=int(t.get('top')), left=int(t.get('left')), width=int(t.get('width')),
                              size=size, fam=fam, color=color, inner=inner, plain=plain))
        pages.append((n, items))
    return pages

def norm(s):
    s = s.replace('\xa0', ' ')
    s = re.sub(r'[ \t]+', ' ', s)
    s = s.replace('­', '').replace('‐', '-').replace('‑', '-')
    return s

def lines_of_page(items):
    out = []
    for col in (0, 1):
        its = [i for i in items if (i['left'] >= RIGHT_X) == bool(col)]
        its.sort(key=lambda i: (i['top'], i['left']))
        cur = []
        for it in its:
            if cur and abs(it['top'] - cur[0]['top']) <= 5:
                cur.append(it)
            else:
                if cur: out.append(_mk(cur, col))
                cur = [it]
        if cur: out.append(_mk(cur, col))
    return out

def _mk(cur, col):
    cur.sort(key=lambda i: i['left'])
    html_s = ''.join(i['inner'] for i in cur)
    return dict(col=col, top=min(i['top'] for i in cur), left=cur[0]['left'],
                right=max(i['left'] + i['width'] for i in cur),
                size=max(i['size'] for i in cur), fam=cur[0]['fam'], color=cur[0]['color'],
                inner=html_s, plain=norm(''.join(i['plain'] for i in cur)).strip(),
                fams=set(i['fam'] for i in cur))

HEAD_FAMS = ('GillSans',)

def events():
    """flatten the document to ordered events:
    ('H', size, text) heading; ('P', html, plain, label) paragraph; ('B', html, plain) bullet; ('T', plain) table/statblock text"""
    ev = []
    for n, items in load():
        lines = lines_of_page(items)
        for col in (0, 1):
            cl = [l for l in lines if l['col'] == col]
            if not cl: continue
            colmax = max((l['right'] for l in cl if l['size'] == 15 and l['fam'] == 'Cambria'), default=0)
            prev = None
            cur = None  # current paragraph dict
            def flush():
                nonlocal cur
                if cur:
                    ev.append(cur); cur = None
            for l in cl:
                if l['plain'].startswith('System Reference Document') or re.fullmatch(r'\d+', l['plain']) and l['size'] == 16:
                    continue
                if l['size'] == 16 or (l['size'] == 15 and l['fam'] == 'Calibri'):
                    # page furniture / stat block
                    flush(); ev.append(dict(k='T', plain=l['plain'], page=n, col=col, top=l['top'])); prev = None; continue
                if l['fam'] == 'Calibri' or l['size'] in (13,):
                    flush(); ev.append(dict(k='T', plain=l['plain'], page=n, col=col, top=l['top'])); prev = None; continue
                head_only = l['fams'] <= {'GillSans-SemiBold'} or l['fams'] <= {'GillSans-SemiBold', 'MrsEavesBoldItalic'}
                if head_only and l['size'] >= 18:
                    flush()
                    # merge consecutive heading lines of same size (wrapped heading)
                    if ev and ev[-1]['k'] == 'H' and ev[-1]['size'] == l['size'] and ev[-1]['page'] == n and ev[-1]['col'] == col and l['top'] - ev[-1]['top'] < l['size'] * 2.6:
                        ev[-1]['text'] += ' ' + l['plain']; ev[-1]['top'] = l['top']
                    else:
                        ev.append(dict(k='H', size=l['size'], text=l['plain'], page=n, col=col, top=l['top']))
                    prev = None
                    continue
                bullet = l['plain'].startswith('•')
                text_html = norm(l['inner'])
                # run-in heading: starts with GillSans 18 chunk
                is_indent = (l['left'] - (LEFT_MARGIN if col == 0 else RIGHT_MARGIN)) >= 10 and not bullet
                newp = False
                if cur is None: newp = True
                elif bullet: newp = True
                elif re.match(r'^(?:<[ib]>)+\s*[^<]{1,50}?[.:]\s*(?:</[ib]>)+', l['inner'].strip()): newp = True
                elif prev is not None:
                    gap = l['top'] - prev['top']
                    short = prev['right'] < colmax - 45
                    endp = prev['plain'].endswith(('.', '!', '?', ':', ')', '”', '"')) 
                    if gap > 24: newp = True
                    elif short and endp: newp = True
                    elif cur['k'] == 'B' and l['left'] - (LEFT_MARGIN if col == 0 else RIGHT_MARGIN) < 8: newp = True
                if newp:
                    flush()
                    cur = dict(k='B' if bullet else 'P', html='', plain='', page=n, col=col, top=l['top'])
                    if bullet:
                        text_html = re.sub(r'^<[^>]*>•</[^>]*>\s*•?', '', text_html).lstrip('• ')
                piece = text_html if not bullet else text_html
                lp = l['plain'].lstrip('•').strip() if bullet else l['plain']
                cur['html'] += (' ' if cur['html'] else '') + piece.strip()
                cur['plain'] += (' ' if cur['plain'] else '') + lp
                prev = l
            flush()
    return _merge(ev)

def _merge(ev):
    out = []
    for e in ev:
        if e['k'] == 'P' and out and out[-1]['k'] == 'P' and out[-1]['page'] >= e['page'] - 1:
            pl = out[-1]['plain'].rstrip()
            first = e['plain'].lstrip()[:1]
            if first.islower() and not pl.endswith(('.', '!', '?', ':', '”', '"', ')', ';')):
                out[-1]['html'] += ' ' + e['html']; out[-1]['plain'] += ' ' + e['plain']; continue
        out.append(e)
    return out
