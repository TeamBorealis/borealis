"""Local helpers for the Barbarian/Bard/Cleric/Druid modules (fork C)."""
import os, sys, json
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_core import *

_SPELLS = json.load(open(os.path.join(HERE, 'spell_index.json')))
_BYNAME = {norm_name(v['name']): k for k, v in _SPELLS.items()}

def sid(name):
    """spell element id for an SRD spell name"""
    k = _BYNAME.get(norm_name(name))
    if not k:
        raise KeyError(f'unknown SRD spell {name!r}')
    return k

def cum(pairs):
    """[(level, absolute value)] -> {1..20: value}"""
    return level_table(pairs, 0)

def spellcasting_feature(cls, ability, html, slots, cantrips, known=None, prepare=False, focus_id=None,
                         ritual=True, tag=None, extra_rules=None, sheet_text=None, id_=None):
    low = cls.lower()
    rules = []
    if ritual:
        rules.append(grant('ID_INTERNAL_RITUAL_CASTING', 'Grants'))
    if focus_id:
        rules.append(grant(focus_id, 'Proficiency'))
    rules.append(grant('ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_FULL', 'Grants', requirements='ID_INTERNAL_GRANT_MULTICLASS'))
    rules += slot_rules(cls, slots)
    if prepare:
        rules += [stat(f'{low}:spellcasting:prepare', f'{ability.lower()}:modifier'), stat(f'{low}:spellcasting:prepare', f'level:{low}')]
    sup = '$(spellcasting:list), 0'
    rules += count_selects('Spell', f'Cantrip ({cls})', cantrips, sup, cls)
    if known:
        sup2 = '$(spellcasting:list), $(spellcasting:slots)'
        # first picks are level 1 only is not required: slots-based filter covers it
        rules += count_selects('Spell', f'Spell Known ({cls})', known, sup2, cls, extra={'allow_replace': True} if False else None)
    rules += extra_rules or []
    sc = {'name': cls, 'ability': ability}
    if prepare:
        sc['prepare'] = True
        sc['list'] = {'supports': cls, 'known': True}
    else:
        sc['allow_replace'] = True
        sc['list'] = {'supports': cls}
    text = sheet_text
    return element(id_ or cid(cls, 'Class Feature', 'Spellcasting', tag), 'Spellcasting', 'Class Feature', html, rules=rules,
                   sheet={'display': False, 'descriptions': [{'text': text}]} if text else {'display': False}, spellcasting=sc)

def spell_grants(pairs, cls, level_of=None):
    """[(class_level, spell name), ...] -> always-prepared spell grants"""
    return [grant(sid(n), 'Spell', level=l, spellcasting=cls, prepared=True) for l, n in pairs]

def ids_not_in_fixtures(els):
    known = {r[2] for r in fixture_rows()}
    return [e['id'] for e in els if e['id'] not in known]

def spell_html(R, skip=('Equipment',)):
    """Spellcasting feature html: intro + nested sections, leaving out sections that belong to the class intro."""
    i = find_in(R, 'Spellcasting', 21)
    out, chunk, j, skipping = [], [], i + 1, False
    def flush():
        nonlocal chunk
        if chunk:
            out.append(to_html(chunk)); chunk = []
    while j < len(ev):
        e = ev[j]
        if e['k'] == 'H':
            if e['size'] >= 21:
                break
            flush()
            skipping = clean(e['text']) in skip
            if not skipping:
                out.append(f"<h6>{esc_html(clean(e['text']))}</h6>")
        elif e['k'] in ('P', 'B') and not skipping:
            chunk.append(e)
        j += 1
    flush()
    return ''.join(out)

SKILLS_LOW = ['acrobatics', 'animal handling', 'arcana', 'athletics', 'deception', 'history', 'insight', 'intimidation',
              'investigation', 'medicine', 'nature', 'perception', 'performance', 'persuasion', 'religion',
              'sleight of hand', 'stealth', 'survival']
def skill_id(low):
    return 'ID_PROFICIENCY_SKILL_' + low.upper().replace(' ', '')
