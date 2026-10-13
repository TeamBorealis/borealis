"""Warlock, The Fiend, Pact Boons and Eldritch Invocations (SRD 5.1)."""
import os, sys, json
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_core import *

CLS = 'Warlock'
TAG = 'Otherworldly Patron'
CANTRIPS = level_table([(1, 2), (4, 3), (10, 4)])
KNOWN = dict(zip(range(1, 21), [2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15]))
SLOT_COUNT = level_table([(1, 1), (2, 2), (11, 3), (17, 4)])
SLOT_LEVEL = level_table([(1, 1), (3, 2), (5, 3), (7, 4), (9, 5)])
INVOCATIONS = level_table([(2, 2), (5, 3), (7, 4), (9, 5), (12, 6), (15, 7), (18, 8)], default=0)
_SPELLS = None

def spell_id(name):
    global _SPELLS
    if _SPELLS is None:
        idx = json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'spell_index.json')))
        _SPELLS = {v['name'].lower(): k for k, v in idx.items()}
    return _SPELLS[name.lower()]

# invocation -> spell it grants (cast at will or once per long rest)
INV_SPELL = {'Armor of Shadows': 'Mage Armor', 'Ascendant Step': 'Levitate', 'Beast Speech': 'Speak with Animals',
             'Bewitching Whispers': 'Compulsion', 'Chains of Carceri': 'Hold Monster', 'Dreadful Word': 'Confusion',
             'Eldritch Sight': 'Detect Magic', 'Fiendish Vigor': 'False Life', 'Mask of Many Faces': 'Disguise Self',
             'Master of Myriad Forms': 'Alter Self', 'Minions of Chaos': 'Conjure Elemental', 'Mire the Mind': 'Slow',
             'Misty Visions': 'Silent Image', 'Otherworldly Leap': 'Jump', 'Sculptor of Flesh': 'Polymorph',
             'Sign of Ill Omen': 'Bestow Curse', 'Thief of Five Fates': 'Bane', 'Visions of Distant Realms': 'Arcane Eye',
             'Whispers of the Grave': 'Speak with Dead'}
PACT_NAMES = {'Pact of the Chain': 'Chain', 'Pact of the Blade': 'Blade', 'Pact of the Tome': 'Tome'}

def build():
    R = class_range(CLS)
    els = []
    H = lambda t, size=21, sub=False: sec(R, t, size, sub)
    F = lambda n: cid(CLS, 'Class Feature', n)
    pact_id = {p: cid(CLS, 'Class Feature', p) for p in PACT_NAMES}
    grants = [(1, F('Otherworldly Patron')), (1, F('Pact Magic')), (2, F('Eldritch Invocations')), (3, F('Pact Boon')),
              (4, F('Ability Score Improvement')), (11, F('Mystic Arcanum')), (20, F('Eldritch Master'))]
    names = {l: [] for l in range(1, 21)}
    names[1] = ['Otherworldly Patron', 'Pact Magic']; names[2] = ['Eldritch Invocations']; names[3] = ['Pact Boon']
    for l in ASI_LEVELS: names[l] = names[l] + ['Ability Score Improvement']
    for l, n in [(6, 'Otherworldly Patron feature'), (10, 'Otherworldly Patron feature'), (14, 'Otherworldly Patron feature'),
                 (11, 'Mystic Arcanum (6th level)'), (13, 'Mystic Arcanum (7th level)'), (15, 'Mystic Arcanum (8th level)'),
                 (17, 'Mystic Arcanum (9th level)'), (20, 'Eldritch Master')]:
        names[l] = names[l] + [n]
    cols = [('Cantrips Known', CANTRIPS), ('Spells Known', KNOWN), ('Spell Slots', SLOT_COUNT),
            ('Slot Level', {l: ordinal(SLOT_LEVEL[l]) for l in SLOT_LEVEL}),
            ('Invocations Known', {l: (INVOCATIONS[l] or '—') for l in INVOCATIONS})]
    table = class_table(CLS, names, cols)
    mc = multiclass_node('ID_WOTC_PHB_MULTICLASS_WARLOCK', '[cha:13]', 'Charisma 13', 'Light armor, simple weapons',
                         [grant(ARMOR['light'], 'Proficiency'), grant(WEAPON['simple'], 'Proficiency'),
                          grant('ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_SOLO', 'Grants')])
    start = [grant(ARMOR['light'], 'Proficiency'), grant(WEAPON['simple'], 'Proficiency'),
             grant(save('wisdom'), 'Proficiency'), grant(save('charisma'), 'Proficiency')]
    els.append(class_element(CLS, cid(CLS, 'Class', CLS), 'd8',
                             'A wielder of magic that is derived from a bargain with an extraplanar entity.',
                             start, mc, grants, 2, 'Warlock', class_description(CLS, table)))

    els.append(class_feature(CLS, 'Otherworldly Patron', html=H('Otherworldly Patron'), sheet_={'display': False},
                             rules=[select('Archetype', 'Otherworldly Patron', supports=TAG, level=1)]))

    # Pact Magic: slot count/level pattern, slots:N = count while slot level == N
    pm = [stat('warlock:spellcasting:slots:count', d, level=l) for l, d in
          [(1, 1), (2, 1), (11, 1), (17, 1)]]
    pm += step_stats('warlock:spellcasting:slot', SLOT_LEVEL)
    bounds = [(1, 3), (3, 5), (5, 7), (7, 9), (9, None)]
    for n, (lo, hi) in enumerate(bounds, 1):
        pm.append(stat(f'warlock:spellcasting:slots:{n}', 'warlock:spellcasting:slots:count', level=lo))
        if hi:
            pm.append(stat(f'warlock:spellcasting:slots:{n}', '-warlock:spellcasting:slots:count', level=hi))
    pm += step_stats('warlock:spellcasting:cantrips known', CANTRIPS)
    pm += step_stats('warlock:spellcasting:spells known', KNOWN)
    pm.append(grant('ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_SOLO', 'Grants', requirements='ID_INTERNAL_GRANT_MULTICLASS'))
    pm.append(grant('ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_ARCANE_FOCUS', 'Proficiency'))
    pm += count_selects('Spell', 'Cantrip (Warlock)', CANTRIPS, '$(spellcasting:list),0', CLS)
    prev = 0
    for l in range(1, 21):
        if KNOWN[l] > prev:
            m = SLOT_LEVEL[l]
            sup = '$(spellcasting:list),' + (str(m) if m == 1 else '(' + '||'.join(str(i) for i in range(1, m + 1)) + ')')
            pm.append(select('Spell', 'Spell (Warlock)', supports=sup, level=l, number=KNOWN[l] - prev,
                             spellcasting=CLS, allow_replace=True))
        prev = KNOWN[l]
    els.append(class_feature(CLS, 'Pact Magic', html=H('Pact Magic', 21, True), rules=pm,
                             spellcasting={'name': 'Warlock', 'ability': 'Charisma', 'allow_replace': True,
                                           'list': {'supports': 'Warlock'}},
                             sheet_=sheet(display=False, text='Your spellcasting ability is Charisma. You use an arcane focus for your warlock spells.')))

    inv_rules = []
    prev = 0
    for l in range(1, 21):
        if INVOCATIONS[l] > prev:
            inv_rules.append(select('Class Feature', f'Eldritch Invocation (Level {l})', supports='Eldritch Invocation',
                                    level=l, number=INVOCATIONS[l] - prev))
        prev = INVOCATIONS[l]
    els.append(class_feature(CLS, 'Eldritch Invocations', html=H('Eldritch Invocations'), rules=inv_rules, sheet_={'display': False}))

    els.append(class_feature(CLS, 'Pact Boon', html=H('Pact Boon'), sheet_={'display': False},
                             rules=[select('Class Feature', 'Pact Boon', supports='Pact Boon', level=3)]))
    for p in PACT_NAMES:
        rules = None
        if p == 'Pact of the Chain':
            rules = [grant(spell_id('Find Familiar'), 'Spell', spellcasting=CLS)]
        elif p == 'Pact of the Tome':
            rules = [select('Spell', 'Cantrip (Pact of the Tome)', supports='0', number=3, spellcasting=CLS)]
        els.append(class_feature(CLS, p, html=feature_html(find_in(R, p, 18)), supports=['Pact Boon'], rules=rules))

    els.append(asi_feature(CLS, ASI_LEVELS, H('Ability Score Improvement'), id_=F('Ability Score Improvement')))
    els.append(class_feature(CLS, 'Mystic Arcanum', html=H('Mystic Arcanum'), sheet_={'display': False}, rules=[
        select('Spell', 'Arcanum Spell (Mystic Arcanum)', supports=f'Warlock,{sl}', level=lv, spellcasting=CLS)
        for lv, sl in [(11, 6), (13, 7), (15, 8), (17, 9)]]))
    els.append(class_feature(CLS, 'Eldritch Master', html=H('Eldritch Master'), sheet_=sheet(usage='1/Long Rest', action='1 minute')))

    # ---- invocations
    ec = find_in(R, 'Eldritch Invocations', 27)
    ebl = spell_id('Eldritch Blast')
    i = ec + 1
    while not (ev[i]['k'] == 'H' and ev[i]['size'] == 27):
        e = ev[i]
        if e['k'] == 'H' and e['size'] == 18:
            name = clean(e['text'])
            html = feature_html(i)
            plain = re.sub(r'<[^>]+>', ' ', html)
            m = re.search(r'Prerequisite:\s*(.*?)(?:\s{2,}|$)', plain)
            pre = re.match(r'\s*Prerequisite:\s*([^.]*?)(?=[A-Z][a-z]+ (?:can|can’t|gain|learn|know|you)|\s{2}|$)', plain)
            reqs, supports = [], ['Eldritch Invocation']
            ptxt = re.match(r'\s*Prerequisite:\s*(.*?)\s*(?=You |When |Choose |As |Whenever |If |Your |Starting )', plain)
            ptxt = ptxt.group(1) if ptxt else ''
            lm = re.search(r'(\d+)(?:st|nd|rd|th) level', ptxt)
            if lm: reqs.append(f'[level:{lm.group(1)}]')
            for p, tag in PACT_NAMES.items():
                if p in ptxt:
                    reqs.append(pact_id[p]); supports.append(p)
            if 'eldritch blast' in ptxt: reqs.append(ebl)
            rules = []
            if name in INV_SPELL:
                rules.append(grant(spell_id(INV_SPELL[name]), 'Spell', spellcasting=CLS))
            if name == 'Agonizing Blast':
                rules.append(stat('agonizing blast:damage', 'charisma:modifier', bonus='base'))
            if name == 'Beguiling Influence':
                rules += [grant(SKILL_IDS['Deception'], 'Proficiency'), grant(SKILL_IDS['Persuasion'], 'Proficiency')]
            if name == 'Lifedrinker':
                rules += [stat('lifedrinker:damage', 1, bonus='modifier'), stat('lifedrinker:damage', 'charisma:modifier', bonus='modifier')]
            if name == 'Book of Ancient Secrets':
                rules.append(select('Spell', 'Book of Ancient Secrets', supports='1,Ritual', number=2, spellcasting=CLS))
            id_ = cid(CLS, 'Class Feature', name).upper()
            els.append(class_feature(CLS, name, html=html, supports=supports, id_=id_, rules=rules or None,
                                     requirements=','.join(reqs) or None))
        i += 1

    # ---- The Fiend
    ar = find_in(R, 'The Fiend', 21)
    ARC = lambda t: feature_html(find_in(R, t, 18, ar))
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    cg = [(1, af('Expanded Spell List')), (1, af('Dark One’s Blessing')), (6, af('Dark One’s Own Luck')),
          (10, af('Fiendish Resilience')), (14, af('Hurl Through Hell'))]
    els.append(archetype_element(CLS, 'The Fiend', TAG, feature_html(ar), cg, tag='OTHERWORLDLY_PATRON'))
    fiend = ['Burning Hands', 'Command', 'Blindness/Deafness', 'Scorching Ray', 'Fireball', 'Stinking Cloud',
             'Fire Shield', 'Wall of Fire', 'Flame Strike', 'Hallow']
    exp_html = ARC('Expanded Spell List') + '<table><thead><tr><th>Spell Level</th><th>Spells</th></tr></thead><tbody>' + ''.join(
        f'<tr><td>{ordinal(n + 1)}</td><td>{fiend[2 * n].lower()}, {fiend[2 * n + 1].lower()}</td></tr>' for n in range(5)) + '</tbody></table>'
    els.append(class_feature(CLS, 'Expanded Spell List', html=exp_html, ty='Archetype Feature', sheet_={'display': False},
                             spellcasting={'name': 'Warlock', 'extend': True,
                                           'extends': [{'supports': spell_id(s)} for s in fiend]}))
    els.append(class_feature(CLS, 'Dark One’s Blessing', html=ARC('Dark One’s Blessing'), ty='Archetype Feature',
                             rules=[stat('dark ones blessing:temp hp', 'charisma:modifier'),
                                    stat('dark ones blessing:temp hp', 'level:warlock')]))
    els.append(class_feature(CLS, 'Dark One’s Own Luck', html=ARC('Dark One’s Own Luck'), ty='Archetype Feature',
                             sheet_=sheet(usage='1/Short Rest')))
    els.append(class_feature(CLS, 'Fiendish Resilience', html=ARC('Fiendish Resilience'), ty='Archetype Feature',
                             rules=[select('Condition', 'Fiendish Resilience', supports='Resistance', optional=True, level=10)]))
    els.append(class_feature(CLS, 'Hurl Through Hell', html=ARC('Hurl Through Hell'), ty='Archetype Feature',
                             sheet_=sheet(usage='1/Long Rest')))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/warlock.json', els, name='SRD 5.1 Warlock'))
