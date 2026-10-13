"""Wizard and School of Evocation (SRD 5.1)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_core import *

CLS = 'Wizard'
TAG = 'Arcane Tradition'
CANTRIPS = level_table([(1, 3), (4, 4), (10, 5)])

def build():
    R = class_range(CLS)
    els = []
    H = lambda t, size=21, sub=False: sec(R, t, size, sub)
    F = lambda n: cid(CLS, 'Class Feature', n)
    grants = [(1, F('Spellcasting')), (1, F('Arcane Recovery')), (2, F('Arcane Tradition')),
              (4, F('Ability Score Improvement')), (18, F('Spell Mastery')), (20, F('Signature Spells'))]
    names = {l: [] for l in range(1, 21)}
    names[1] = ['Spellcasting', 'Arcane Recovery']; names[2] = ['Arcane Tradition']
    for l in ASI_LEVELS: names[l] = names[l] + ['Ability Score Improvement']
    for l in (6, 10, 14): names[l] = names[l] + ['Arcane Tradition feature']
    names[18] = names[18] + ['Spell Mastery']; names[20] = names[20] + ['Signature Spells']
    cols = [('Cantrips Known', CANTRIPS)] + \
           [(ordinal(n), {l: (FULL_SLOTS[l][n - 1] if len(FULL_SLOTS[l]) >= n else '—') for l in range(1, 21)}) for n in range(1, 10)]
    table = class_table(CLS, names, cols)
    mc = multiclass_node('ID_WOTC_PHB_MULTICLASS_WIZARD', '[int:13]', 'Intelligence 13', 'None', [])
    start = [grant(wprof(w), 'Proficiency') for w in ('dagger', 'dart', 'sling', 'quarterstaff', 'crossbow light')] + \
            [grant(save('intelligence'), 'Proficiency'), grant(save('wisdom'), 'Proficiency')]
    els.append(class_element(CLS, cid(CLS, 'Class', CLS), 'd6',
                             'A scholarly magic-user capable of manipulating the structures of reality.',
                             start, mc, grants, 2, 'Wizard', class_description(CLS, table)))

    sc = [grant('ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_FULL', 'Grants', requirements='ID_INTERNAL_GRANT_MULTICLASS'),
          grant('ID_INTERNAL_RITUAL_CASTING', 'Grants'),
          grant('ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_ARCANE_FOCUS', 'Proficiency')]
    sc += slot_rules(CLS, FULL_SLOTS)
    sc += [stat('wizard:spellcasting:prepare', 'intelligence:modifier'), stat('wizard:spellcasting:prepare', 'level:wizard')]
    sc += step_stats('wizard:spellcasting:cantrips known', CANTRIPS)
    sc += count_selects('Spell', 'Cantrip (Wizard)', CANTRIPS, '$(spellcasting:list),0', CLS)
    sc.append(select('Spell', 'Spellbook (Wizard)', supports='$(spellcasting:list),1', level=1, number=6, spellcasting=CLS))
    for l in range(2, 21):
        sc.append(select('Spell', 'Spellbook (Wizard)', supports='$(spellcasting:list),$(spellcasting:slots)', level=l,
                         number=2, spellcasting=CLS))
    els.append(class_feature(CLS, 'Spellcasting', html=H('Spellcasting', 21, True), rules=sc,
                             spellcasting={'name': 'Wizard', 'ability': 'Intelligence', 'prepare': True,
                                           'list': {'supports': 'Wizard'}},
                             sheet_=sheet(display=False, text='You can prepare {{wizard:spellcasting:prepare}} spells from your spellbook. You use an arcane focus to cast spells and can cast wizard spells as rituals.')))
    els.append(class_feature(CLS, 'Arcane Recovery', html=H('Arcane Recovery'),
                             sheet_=sheet(usage='1/Day', text='Once per day when you finish a short rest, recover expended spell slots with a combined level equal to or less than half your wizard level (rounded up); none can be 6th level or higher.')))
    els.append(class_feature(CLS, 'Arcane Tradition', html=H('Arcane Tradition'), sheet_={'display': False},
                             rules=[select('Archetype', 'Arcane Tradition', supports=TAG, level=2)]))
    els.append(asi_feature(CLS, ASI_LEVELS, H('Ability Score Improvement'), id_=F('Ability Score Improvement')))
    els.append(class_feature(CLS, 'Spell Mastery', html=H('Spell Mastery'),
                             rules=[select('Spell', 'Spell Mastery (1st Level)', supports='$(spellcasting:list),1', spellcasting=CLS, prepared=True, level=18),
                                    select('Spell', 'Spell Mastery (2nd Level)', supports='$(spellcasting:list),2', spellcasting=CLS, prepared=True, level=18)]))
    els.append(class_feature(CLS, 'Signature Spells', html=H('Signature Spells'), sheet_=sheet(usage='1/Short Rest'),
                             rules=[select('Spell', 'Signature Spell', supports='$(spellcasting:list),3', spellcasting=CLS, number=2, prepared=True, level=20)]))

    ar = find_in(R, 'School of Evocation', 21)
    ARC = lambda t: feature_html(find_in(R, t, 18, ar))
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    cg = [(2, af('Evocation Savant')), (2, af('Sculpt Spells')), (6, af('Potent Cantrip')),
          (10, af('Empowered Evocation')), (14, af('Overchannel'))]
    els.append(archetype_element(CLS, 'School of Evocation', TAG, feature_html(ar), cg, tag='WIZARD'))
    for n in ['Evocation Savant', 'Sculpt Spells', 'Potent Cantrip', 'Empowered Evocation', 'Overchannel']:
        els.append(class_feature(CLS, n, html=ARC(n), ty='Archetype Feature',
                                 sheet_=sheet(usage='1/Long Rest') if n == 'Overchannel' else None))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/wizard.json', els, name='SRD 5.1 Wizard'))
