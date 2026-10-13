"""Helpers shared by the class modules (cls_*.py): SRD text lookup, fixture ids, rule macros, tables."""
import re, sys, os, json, glob
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

ev = events()

# ------------------------------------------------------------------ id lookup (identifiers only)
_FIXC = None
def _fixclass():
    """class name -> {(type, normname): id} read from the legacy class files (identifiers only)."""
    global _FIXC
    if _FIXC is None:
        _FIXC = {}
        for f, t, i, n in fixture_rows():
            m = re.match(r'elements\.system_reference_document\.classes\.class-(\w+)\.xml', f)
            if m and t in ('Class', 'Class Feature', 'Archetype', 'Archetype Feature'):
                _FIXC.setdefault(m.group(1), {}).setdefault((t, norm_name(n)), i)
    return _FIXC

TYPE_PREFIX = {'Class Feature': 'ID_WOTC_PHB_CLASS_FEATURE', 'Archetype': 'ID_WOTC_PHB_ARCHETYPE',
               'Archetype Feature': 'ID_WOTC_PHB_ARCHETYPE_FEATURE', 'Class': 'ID_WOTC_PHB_CLASS'}

def cid(cls, type_, name, fallback_tag=None):
    """id the legacy SRD data used for this class element, else a derived one."""
    i = _fixclass().get(cls.lower(), {}).get((type_, norm_name(name)))
    if i:
        return i
    tag = fallback_tag or cls.upper()
    return make_id(f'{TYPE_PREFIX[type_]}_{tag}', name)

# ------------------------------------------------------------------ SRD text lookup
def class_range(name):
    a = find_head(name, 39)
    b = a + 1
    while b < len(ev) and not (ev[b]['k'] == 'H' and ev[b]['size'] == 39):
        b += 1
    return a, b

def find_in(rng, text, size=None, after=None):
    a, b = rng
    i = find_head(text, size, after if after is not None else a, b)
    if i < 0:
        raise KeyError(f'heading {text!r} (size {size}) not found in range {rng}')
    return i

def html_h(level, text):
    tag = {27: 'h4', 21: 'h5', 18: 'h6'}.get(level, 'h6')
    return f'<{tag}>{esc_html(text)}</{tag}>'

def esc_html(s):
    return s.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;')

def feature_html(i, include_sub=False, sub_heads=True):
    """html for heading i: body paragraphs; with include_sub also the nested size-18 sections."""
    size = ev[i]['size']
    out = []
    j = i + 1
    chunk = []
    def flush():
        nonlocal chunk
        if chunk:
            out.append(to_html(chunk)); chunk = []
    while j < len(ev):
        e = ev[j]
        if e['k'] == 'H':
            if e['size'] >= size or not include_sub:
                break
            flush()
            if sub_heads:
                out.append(f"<h6>{esc_html(clean(e['text']))}</h6>")
        elif e['k'] in ('P', 'B'):
            chunk.append(e)
        j += 1
    flush()
    return ''.join(out)

def sec(rng, text, size=None, include_sub=False, after=None):
    return feature_html(find_in(rng, text, size, after), include_sub)

def first_sentences(html, limit=320):
    t = re.sub(r'<[^>]+>', ' ', html)
    t = re.sub(r'\s+', ' ', t).strip()
    if len(t) <= limit:
        return t
    cut = t[:limit]
    k = max(cut.rfind('. '), cut.rfind('; '))
    return (cut[:k + 1] if k > 80 else cut.rsplit(' ', 1)[0] + '…')

def class_description(name, table_html=''):
    rng = class_range(name)
    parts = []
    i = find_in(rng, 'Class Features', 27)
    parts.append(feature_html(i))
    for h in ('Hit Points', 'Proficiencies', 'Equipment'):
        try:
            j = find_in(rng, h, 18)
        except KeyError:
            continue
        parts.append(f'<h5>{h.upper()}</h5>' + feature_html(j))
    parts.append(table_html)
    return ''.join(parts)

# ------------------------------------------------------------------ rule macros
ABIL = ['strength', 'dexterity', 'constitution', 'intelligence', 'wisdom', 'charisma']
SKILL_IDS = {s: 'ID_PROFICIENCY_SKILL_' + s.upper().replace(' ', '') for s in
             ['Acrobatics', 'Animal Handling', 'Arcana', 'Athletics', 'Deception', 'History', 'Insight', 'Intimidation',
              'Investigation', 'Medicine', 'Nature', 'Perception', 'Performance', 'Persuasion', 'Religion',
              'Sleight of Hand', 'Stealth', 'Survival']}
ARMOR = {'light': 'ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR', 'medium': 'ID_PROFICIENCY_ARMOR_PROFICIENCY_MEDIUM_ARMOR',
         'heavy': 'ID_PROFICIENCY_ARMOR_PROFICIENCY_HEAVY_ARMOR', 'shields': 'ID_PROFICIENCY_ARMOR_PROFICIENCY_SHIELDS'}
WEAPON = {'simple': 'ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS', 'martial': 'ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_WEAPONS'}
def wprof(w):
    return 'ID_PROFICIENCY_WEAPON_PROFICIENCY_' + w.upper().replace(' ', '_')
def save(a):
    return 'ID_PROFICIENCY_SAVINGTHROW_' + a.upper()

def dc_stats(prefix, ability, extra_base=8):
    """<prefix>:dc = 8 + proficiency + ability modifier"""
    return [stat(f'{prefix}:dc', extra_base), stat(f'{prefix}:dc', 'proficiency'), stat(f'{prefix}:dc', f'{ability}:modifier')]

def step_stats(name, table, bonus=None, start_level=1):
    """absolute per-level table {level: value} -> incremental `stat` rules (same-name rules add)."""
    rules, prev = [], 0
    for lvl in sorted(table):
        v = table[lvl]
        if v != prev:
            rules.append(stat(name, v - prev, level=lvl, bonus=bonus))
            prev = v
    return rules

def level_table(pairs, default=None):
    """[(from_level, value), ...] -> {level 1..20: value}"""
    t = {}
    cur = default
    pairs = sorted(pairs)
    k = 0
    for lvl in range(1, 21):
        while k < len(pairs) and pairs[k][0] <= lvl:
            cur = pairs[k][1]; k += 1
        t[lvl] = cur
    return t

# spell slot tables ------------------------------------------------------------------
FULL_SLOTS = {1: [2], 2: [3], 3: [4, 2], 4: [4, 3], 5: [4, 3, 2], 6: [4, 3, 3], 7: [4, 3, 3, 1], 8: [4, 3, 3, 2],
              9: [4, 3, 3, 3, 1], 10: [4, 3, 3, 3, 2], 11: [4, 3, 3, 3, 2, 1], 12: [4, 3, 3, 3, 2, 1],
              13: [4, 3, 3, 3, 2, 1, 1], 14: [4, 3, 3, 3, 2, 1, 1], 15: [4, 3, 3, 3, 2, 1, 1, 1],
              16: [4, 3, 3, 3, 2, 1, 1, 1], 17: [4, 3, 3, 3, 2, 1, 1, 1, 1], 18: [4, 3, 3, 3, 3, 1, 1, 1, 1],
              19: [4, 3, 3, 3, 3, 2, 1, 1, 1], 20: [4, 3, 3, 3, 3, 2, 2, 1, 1]}
HALF_SLOTS = {2: [2], 3: [3], 4: [3], 5: [4, 2], 6: [4, 2], 7: [4, 3], 8: [4, 3], 9: [4, 3, 2], 10: [4, 3, 2],
              11: [4, 3, 3], 12: [4, 3, 3], 13: [4, 3, 3, 1], 14: [4, 3, 3, 1], 15: [4, 3, 3, 2], 16: [4, 3, 3, 2],
              17: [4, 3, 3, 3, 1], 18: [4, 3, 3, 3, 1], 19: [4, 3, 3, 3, 2], 20: [4, 3, 3, 3, 2]}

def slot_rules(cls, table):
    """`<cls>:spellcasting:slots:N` incremental rules from an absolute slots table {level: [n1, n2, ...]}"""
    rules = []
    for n in range(1, 10):
        per = {lvl: (row[n - 1] if len(row) >= n else 0) for lvl, row in table.items()}
        full = {l: per.get(l, 0) for l in range(1, 21)}
        rules += step_stats(f'{cls.lower()}:spellcasting:slots:{n}', full)
    return rules

def count_selects(type_, name_fmt, table, supports, spellcasting, extra=None):
    """select rules adding `delta` picks at each level where the cumulative count table grows."""
    rules, prev = [], 0
    for lvl in sorted(table):
        v = table[lvl]
        if v > prev:
            kw = dict(spellcasting=spellcasting)
            if extra: kw.update(extra)
            rules.append(select(type_, name_fmt, supports=supports, number=v - prev, level=lvl, **kw))
        prev = v
    return rules

def ordinal(n):
    return f'{n}{"th" if 10 <= n % 100 <= 20 else {1: "st", 2: "nd", 3: "rd"}.get(n % 10, "th")}'

# ------------------------------------------------------------------ element makers
def feat_sheet(text=None, display=None, usage=None, action=None, entries=None):
    return sheet(text=text, display=display, usage=usage, action=action, entries=entries)

def class_feature(cls, name, level_note=None, html=None, rules=None, sheet_=None, supports=None, id_=None,
                  requirements=None, spellcasting=None, ty='Class Feature', compendium=None, setters=None, auto_sheet=True,
                  tag=None):
    id_ = id_ or cid(cls, ty, name, tag)
    if sheet_ is None and auto_sheet and html:
        sheet_ = {'descriptions': [{'text': first_sentences(html)}]}
    return element(id_, name, ty, html or '', supports=supports, rules=rules, sheet=sheet_,
                   requirements=requirements, spellcasting=spellcasting, compendium=compendium, setters=setters)

def asi_feature(cls, levels, html, tag=None, id_=None):
    id_ = id_ or cid(cls, 'Class Feature', 'Ability Score Improvement', tag)
    rules = [select('Class Feature', f'Improvement Option ({cls} {lv})', supports=f'Improvement Option,{cls},{lv}', level=lv)
             for lv in levels]
    return element(id_, 'Ability Score Improvement', 'Class Feature', html, rules=rules, sheet={'display': False},
                   compendium={'display': False})

ASI_LEVELS = [4, 8, 12, 16, 19]
ASI_LEVELS_F = [4, 6, 8, 12, 14, 16, 19]
ASI_LEVELS_R = [4, 8, 10, 12, 16, 19]

def multiclass_node(id_, requirements, prerequisite, profs_text, grants, extra_rules=None, description=''):
    rules = [grant('ID_INTERNAL_GRANT_MULTICLASS', 'Grants')] + grants + (extra_rules or [])
    d = {'id': id_, 'prerequisite': prerequisite, 'requirements': requirements,
         'setters': [setter('multiclass proficiencies', profs_text)], 'rules': rules}
    if description: d['description'] = description
    return d

def class_element(name, id_, hd, short, start_grants, mc, feature_grants, skill_n, skill_tag, html, extra_rules=None, tool_selects=None):
    """Class element. start_grants are gated with !<multiclass id>."""
    gate = f'!{mc["id"]}'
    rules = [dict(g, requirements=gate) if 'requirements' not in g else g for g in start_grants]
    rules += tool_selects or []
    if skill_n:
        rules.append(select('Proficiency', f'Skill Proficiency ({name})', supports=f'Skill,{skill_tag or name}', number=skill_n, requirements=gate))
    rules += extra_rules or []
    for lvl, fid_ in feature_grants:
        rules.append(grant(fid_, 'Class Feature', level=lvl))
    return element(id_, name, 'Class', html, rules=rules, sheet={'display': False, 'descriptions': [{'text': short}]},
                   setters=[setter('short', short), setter('hd', hd)], multiclass=mc)

def archetype_element(cls, name, supports, html, feature_grants, ty='Archetype', extra_rules=None, tag=None, id_=None):
    id_ = id_ or cid(cls, 'Archetype', name, tag)
    rules = (extra_rules or []) + [grant(fid_, 'Archetype Feature', level=lvl) for lvl, fid_ in feature_grants]
    return element(id_, name, 'Archetype', html, supports=[supports], rules=rules, sheet={'display': False})

def levels_with(feature_grants):
    out = {}
    for lvl, fid_ in feature_grants:
        out.setdefault(lvl, []).append(fid_)
    return out

PROF_BONUS = {l: 2 + (l - 1) // 4 for l in range(1, 21)}
def ord_col(l): return ordinal(l)

def class_table(name, feature_names_by_level, columns):
    """HTML progression table: columns = [(header, {level: text})]."""
    heads = ['Level', 'Proficiency Bonus', 'Features'] + [h for h, _ in columns]
    rows = []
    for l in range(1, 21):
        feats = ', '.join(feature_names_by_level.get(l, [])) or '—'
        cells = [ordinal(l), f'+{PROF_BONUS[l]}', esc_html(feats)] + [esc_html(str(c.get(l, '—'))) for _, c in columns]
        rows.append('<tr>' + ''.join(f'<td>{c}</td>' for c in cells) + '</tr>')
    return (f'<h5 class="caption">THE {name.upper()}</h5><table class="class-features"><thead><tr>' +
            ''.join(f'<th>{h}</th>' for h in heads) + '</tr></thead><tbody>' + ''.join(rows) + '</tbody></table>')
