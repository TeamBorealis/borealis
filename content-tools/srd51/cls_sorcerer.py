"""Sorcerer and Draconic Bloodline (SRD 5.1)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_core import *

CLS = 'Sorcerer'
TAG = 'Sorcerous Origin'
CANTRIPS = level_table([(1, 4), (4, 5), (10, 6)])
KNOWN = dict(zip(range(1, 21), [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 12, 13, 13, 14, 14, 15, 15, 15, 15]))
ANCESTRY = [('Black', 'Acid'), ('Blue', 'Lightning'), ('Brass', 'Fire'), ('Bronze', 'Lightning'), ('Copper', 'Acid'),
            ('Gold', 'Fire'), ('Green', 'Poison'), ('Red', 'Fire'), ('Silver', 'Cold'), ('White', 'Cold')]
METAMAGIC = ['Careful Spell', 'Distant Spell', 'Empowered Spell', 'Extended Spell', 'Heightened Spell',
             'Quickened Spell', 'Subtle Spell', 'Twinned Spell']

def build():
    R = class_range(CLS)
    els = []
    H = lambda t, size=21, sub=False: sec(R, t, size, sub)
    F = lambda n: cid(CLS, 'Class Feature', n)
    grants = [(1, F('Spellcasting')), (1, F('Sorcerous Origin')), (2, F('Font of Magic')), (3, F('Metamagic')),
              (4, F('Ability Score Improvement')), (20, F('Sorcerous Restoration'))]
    names = {l: [] for l in range(1, 21)}
    names[1] = ['Spellcasting', 'Sorcerous Origin']; names[2] = ['Font of Magic']; names[3] = ['Metamagic']
    for l in ASI_LEVELS: names[l] = names[l] + ['Ability Score Improvement']
    names[20] = names[20] + ['Sorcerous Restoration']
    cols = [('Sorcery Points', {l: (l if l >= 2 else '—') for l in range(1, 21)}),
            ('Cantrips Known', CANTRIPS), ('Spells Known', KNOWN)] + \
           [(f'{ordinal(n)}', {l: (FULL_SLOTS[l][n - 1] if len(FULL_SLOTS[l]) >= n else '—') for l in range(1, 21)}) for n in range(1, 10)]
    table = class_table(CLS, names, cols)
    mc = multiclass_node('ID_WOTC_PHB_MULTICLASS_SORCERER', '[cha:13]', 'Charisma 13', 'None',
                         [grant(ARMOR['light'], 'Proficiency')][:0])
    start = [grant(wprof(w), 'Proficiency') for w in ('dagger', 'dart', 'sling', 'quarterstaff', 'crossbow light')] + \
            [grant(save('constitution'), 'Proficiency'), grant(save('charisma'), 'Proficiency')]
    els.append(class_element(CLS, cid(CLS, 'Class', CLS), 'd6',
                             'An innately magical spellcaster whose power comes from a draconic bloodline or wild magic.',
                             start, mc, grants, 2, 'Sorcerer', class_description(CLS, table)))

    # Spellcasting
    sc_rules = [grant('ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_FULL', 'Grants', requirements='ID_INTERNAL_GRANT_MULTICLASS')]
    sc_rules += slot_rules(CLS, FULL_SLOTS)
    sc_rules += step_stats('sorcerer:spellcasting:cantrips known', CANTRIPS)
    sc_rules += step_stats('sorcerer:spellcasting:spells known', KNOWN)
    sc_rules += count_selects('Spell', 'Cantrip (Sorcerer)', CANTRIPS, '$(spellcasting:list),0', CLS)
    sc_rules += count_selects('Spell', 'Spell (Sorcerer)', KNOWN, '$(spellcasting:list),$(spellcasting:slots)', CLS,
                              extra={'allow_replace': True})
    els.append(class_feature(CLS, 'Spellcasting', html=H('Spellcasting', 21, True), rules=sc_rules,
                             spellcasting={'name': 'Sorcerer', 'ability': 'Charisma', 'allow_replace': True,
                                           'list': {'supports': 'Sorcerer'}},
                             sheet_=sheet(display=False, text='Your spellcasting ability is Charisma. You use an arcane focus for your sorcerer spells.'),
                             ))
    els[-1]['rules'].append(grant('ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_ARCANE_FOCUS', 'Proficiency'))

    els.append(class_feature(CLS, 'Sorcerous Origin', html=H('Sorcerous Origin'), sheet_={'display': False},
                             rules=[select('Archetype', 'Sorcerous Origin', supports=TAG, level=1)]))
    els.append(class_feature(CLS, 'Font of Magic', html=H('Font of Magic', 21, True), rules=[
        stat('sorcerer:sorcery points', 'level:sorcerer', level=2)],
        sheet_=sheet(usage='{{sorcerer:sorcery points}} Sorcery Points', text='You can spend sorcery points to create spell slots or convert spell slots to sorcery points (Flexible Casting).')))
    mm_rules = [select('Class Feature', 'Metamagic (Sorcerer)', supports='Metamagic', level=3, number=2),
                select('Class Feature', 'Metamagic (Sorcerer)', supports='Metamagic', level=10),
                select('Class Feature', 'Metamagic (Sorcerer)', supports='Metamagic', level=17)]
    els.append(class_feature(CLS, 'Metamagic', html=H('Metamagic'), rules=mm_rules, sheet_={'display': False}))
    mm = find_in(R, 'Metamagic', 21)
    for n in METAMAGIC:
        els.append(class_feature(CLS, n, html=feature_html(find_in(R, n, 18, mm)), supports=['Metamagic']))
    els.append(asi_feature(CLS, ASI_LEVELS, H('Ability Score Improvement'), id_=F('Ability Score Improvement')))
    els.append(class_feature(CLS, 'Sorcerous Restoration', html=H('Sorcerous Restoration'),
                             sheet_=sheet(usage='1/Long Rest', text='Regain 4 expended sorcery points whenever you finish a short rest.')))

    # Draconic Bloodline
    ar = find_in(R, 'Draconic Bloodline', 21)
    ARC = lambda t: feature_html(find_in(R, t, 18, ar))
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    cg = [(1, af('Dragon Ancestor')), (1, af('Draconic Resilience')), (6, af('Elemental Affinity')),
          (14, af('Dragon Wings')), (18, af('Draconic Presence'))]
    els.append(archetype_element(CLS, 'Draconic Bloodline', TAG, feature_html(ar), cg, tag='SORCERER'))
    els.append(class_feature(CLS, 'Dragon Ancestor', html=ARC('Dragon Ancestor'), ty='Archetype Feature', rules=[
        grant('ID_LANGUAGE_DRACONIC', 'Language'),
        select('Archetype Feature', 'Draconic Ancestor', supports='Draconic Ancestor', level=1)]))
    for col, dt in ANCESTRY:
        els.append(element(f'ID_WOTC_PHB_ARCHETYPE_FEATURE_SORCERER_DRACONIC_ANCESTOR_{col.upper()}',
                           f'Dragon Ancestor ({col})', 'Archetype Feature',
                           f'<p>Your dragon ancestor is a {col.lower()} dragon. The associated damage type is {dt.lower()}.</p>',
                           supports=['Draconic Ancestor'],
                           rules=[stat('draconic-ancestor', col, inline=True), stat('draconic-ancestor:damage type', dt, inline=True)],
                           sheet={'display': False}))
    els.append(class_feature(CLS, 'Draconic Resilience', html=ARC('Draconic Resilience'), ty='Archetype Feature', rules=[
        stat('hp', 'level:sorcerer'),
        stat('ac:draconic resilience:base', 13),
        stat('ac:draconic resilience:dexterity', 'dexterity:modifier'),
        stat('ac:draconic resilience', 'ac:draconic resilience:base'),
        stat('ac:draconic resilience', 'ac:draconic resilience:dexterity'),
        stat('ac:calculation', 'ac:draconic resilience', bonus='calculation', equipped='[armor:none]', alt='Draconic Resilience')]))
    els.append(class_feature(CLS, 'Elemental Affinity', html=ARC('Elemental Affinity'), ty='Archetype Feature'))
    els.append(class_feature(CLS, 'Dragon Wings', html=ARC('Dragon Wings'), ty='Archetype Feature', sheet_=sheet(action='Bonus Action')))
    els.append(class_feature(CLS, 'Draconic Presence', html=ARC('Draconic Presence'), ty='Archetype Feature',
                             sheet_=sheet(action='Action')))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/sorcerer.json', els, name='SRD 5.1 Sorcerer'))
