"""Druid and Circle of the Land (SRD 5.1)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_caster import *

CLS = 'Druid'
TAG = 'Druid Circle'
LAND_TAG = 'Circle of the Land Terrain'

LANDS = {
    'Arctic': [(3, 'Hold Person'), (3, 'Spike Growth'), (5, 'Sleet Storm'), (5, 'Slow'), (7, 'Freedom of Movement'), (7, 'Ice Storm'), (9, 'Commune with Nature'), (9, 'Cone of Cold')],
    'Coast': [(3, 'Mirror Image'), (3, 'Misty Step'), (5, 'Water Breathing'), (5, 'Water Walk'), (7, 'Control Water'), (7, 'Freedom of Movement'), (9, 'Conjure Elemental'), (9, 'Scrying')],
    'Desert': [(3, 'Blur'), (3, 'Silence'), (5, 'Create Food and Water'), (5, 'Protection from Energy'), (7, 'Blight'), (7, 'Hallucinatory Terrain'), (9, 'Insect Plague'), (9, 'Wall of Stone')],
    'Forest': [(3, 'Barkskin'), (3, 'Spider Climb'), (5, 'Call Lightning'), (5, 'Plant Growth'), (7, 'Divination'), (7, 'Freedom of Movement'), (9, 'Commune with Nature'), (9, 'Tree Stride')],
    'Grassland': [(3, 'Invisibility'), (3, 'Pass without Trace'), (5, 'Daylight'), (5, 'Haste'), (7, 'Divination'), (7, 'Freedom of Movement'), (9, 'Dream'), (9, 'Insect Plague')],
    'Mountain': [(3, 'Spider Climb'), (3, 'Spike Growth'), (5, 'Lightning Bolt'), (5, 'Meld into Stone'), (7, 'Stone Shape'), (7, 'Stoneskin'), (9, 'Passwall'), (9, 'Wall of Stone')],
    'Swamp': [(3, 'Acid Arrow'), (3, 'Darkness'), (5, 'Water Walk'), (5, 'Stinking Cloud'), (7, 'Freedom of Movement'), (7, 'Locate Creature'), (9, 'Insect Plague'), (9, 'Scrying')],
}

def build():
    R = class_range(CLS)
    H = lambda t, size=21, sub=False: sec(R, t, size, sub)
    mc_id = 'ID_WOTC_PHB_MULTICLASS_DRUID'
    els = []
    fid = lambda n: cid(CLS, 'Class Feature', n)
    SPELL = fid('Spellcasting')
    grants = [(1, fid('Druidic')), (1, SPELL), (2, fid('Wild Shape')), (2, fid('Druid Circle')),
              (4, fid('Ability Score Improvement')), (18, fid('Timeless Body')), (18, fid('Beast Spells')), (20, fid('Archdruid'))]
    cant = cum([(1, 2), (4, 3), (10, 4)])
    names = {1: ['Druidic', 'Spellcasting'], 2: ['Wild Shape', 'Druid Circle'], 3: [], 4: ['Wild Shape improvement', 'Ability Score Improvement'],
             6: ['Druid Circle feature'], 8: ['Wild Shape improvement', 'Ability Score Improvement'], 10: ['Druid Circle feature'],
             12: ['Ability Score Improvement'], 14: ['Druid Circle feature'], 16: ['Ability Score Improvement'],
             18: ['Timeless Body', 'Beast Spells'], 19: ['Ability Score Improvement'], 20: ['Archdruid']}
    cols = [('Cantrips Known', cant)]
    for n in range(1, 10):
        cols.append((f'{ordinal(n)}', {l: (FULL_SLOTS[l][n - 1] if len(FULL_SLOTS[l]) >= n else '—') for l in range(1, 21)}))
    mc = multiclass_node(mc_id, '[wis:13]', 'Wisdom 13', 'Light armor, medium armor, shields (druids will not wear armor or use shields made of metal)',
                         [grant(ARMOR['light'], 'Proficiency'), grant(ARMOR['medium'], 'Proficiency'), grant(ARMOR['shields'], 'Proficiency')])
    weapons = ['club', 'dagger', 'dart', 'javelin', 'mace', 'quarterstaff', 'scimitar', 'sickle', 'sling', 'spear']
    els.append(class_element(CLS, cid(CLS, 'Class', CLS), 'd8', 'A priest of the Old Faith, wielding the powers of nature and adopting animal forms.',
        [grant(ARMOR[k], 'Proficiency') for k in ('light', 'medium', 'shields')] + [grant(wprof(w), 'Proficiency') for w in weapons] +
        [grant('ID_PROFICIENCY_TOOL_PROFICIENCY_HERBALISM_KIT', 'Proficiency'),
         grant(save('intelligence'), 'Proficiency'), grant(save('wisdom'), 'Proficiency')],
        mc, grants, 2, 'Druid', class_description(CLS, class_table(CLS, names, cols))))

    els.append(class_feature(CLS, 'Druidic', html=H('Druidic'), rules=[grant('ID_LANGUAGE_DRUIDIC', 'Language')]))
    els.append(spellcasting_feature(CLS, 'Wisdom', spell_html(R), FULL_SLOTS, cant, prepare=True,
        focus_id='ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_DRUIDIC_FOCUS',
        sheet_text='You can prepare {{druid:spellcasting:prepare}} druid spells each day. Wisdom is your spellcasting ability; you can use a druidic focus and cast druid spells as rituals.',
        id_=SPELL))
    els.append(class_feature(CLS, 'Wild Shape', html=H('Wild Shape', 21, True), sheet_=sheet(
        action='Action', usage='2/Short Rest', text='Assume the shape of a beast of CR 1/4 or lower with no flying or swimming speed. You can stay in beast shape for {{level:druid:half}} hours.',
        entries=[{'text': 'Assume the shape of a beast of CR 1/2 or lower with no flying speed. You can stay in beast shape for {{level:druid:half}} hours.', 'level': 4},
                 {'text': 'Assume the shape of a beast of CR 1 or lower. You can stay in beast shape for {{level:druid:half}} hours.', 'level': 8}])))
    els.append(class_feature(CLS, 'Druid Circle', html=H('Druid Circle'), sheet_={'display': False},
                             rules=[select('Archetype', 'Druid Circle', supports=TAG, level=2)]))
    els.append(asi_feature(CLS, ASI_LEVELS, H('Ability Score Improvement')))
    els.append(class_feature(CLS, 'Timeless Body', html=H('Timeless Body')))
    els.append(class_feature(CLS, 'Beast Spells', html=H('Beast Spells')))
    els.append(class_feature(CLS, 'Archdruid', html=H('Archdruid'), sheet_=sheet(usage='Unlimited', text=first_sentences(H('Archdruid')))))

    ar = find_in(R, 'Circle of the Land', 21)
    A = lambda t: feature_html(find_in(R, t, 18, ar))
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    cg = [(2, af('Bonus Cantrip')), (2, af('Natural Recovery')), (3, af('Circle Spells')), (6, af('Land’s Stride')),
          (10, af('Nature’s Ward')), (14, af('Nature’s Sanctuary'))]
    els.append(archetype_element(CLS, 'Circle of the Land', TAG, feature_html(ar), cg))
    els.append(class_feature(CLS, 'Bonus Cantrip', html=A('Bonus Cantrip'), ty='Archetype Feature',
        rules=[select('Spell', 'Druid Cantrip (Circle of the Land)', supports='Druid,0', spellcasting=CLS)]))
    els.append(class_feature(CLS, 'Natural Recovery', html=A('Natural Recovery'), ty='Archetype Feature',
        sheet_=sheet(usage='1/Long Rest', text='During a short rest, recover spell slots with a combined level up to {{level:druid:half:up}} (none 6th level or higher).')))
    els.append(class_feature(CLS, 'Circle Spells', html=A('Circle Spells'), ty='Archetype Feature',
        rules=[select('Archetype Feature', 'Circle Land', supports=LAND_TAG, level=3)]))
    for land, sp in LANDS.items():
        rows = {}
        for l, n in sp: rows.setdefault(l, []).append(n.lower())
        text = f'<p>{land} circle spells: ' + '; '.join(f'{ordinal(l)} level {", ".join(v)}' for l, v in rows.items()) + '.</p>'
        els.append(element(f'ID_WOTC_PHB_ARCHETYPE_FEATURE_DRUID_LAND_{land.upper()}', f'Circle Spells ({land})', 'Archetype Feature', text,
                           supports=[LAND_TAG], rules=spell_grants(sp, CLS), sheet={'display': False}))
    els.append(class_feature(CLS, 'Land’s Stride', html=A('Land’s Stride'), ty='Archetype Feature'))
    els.append(class_feature(CLS, 'Nature’s Ward', html=A('Nature’s Ward'), ty='Archetype Feature'))
    els.append(class_feature(CLS, 'Nature’s Sanctuary', html=A('Nature’s Sanctuary'), ty='Archetype Feature', sheet_=sheet(action='Reaction', text=first_sentences(A('Nature’s Sanctuary')))))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/druid.json', els, name='SRD 5.1 Druid'))
    print('non-legacy ids:', ids_not_in_fixtures(els))
