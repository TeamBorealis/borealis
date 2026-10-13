"""Cleric and Life Domain (SRD 5.1)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_caster import *

CLS = 'Cleric'
TAG = 'Divine Domain'

def build():
    R = class_range(CLS)
    H = lambda t, size=21, sub=False: sec(R, t, size, sub)
    mc_id = 'ID_WOTC_PHB_MULTICLASS_CLERIC'
    els = []
    fid = lambda n: cid(CLS, 'Class Feature', n)
    SPELL = fid('Spellcasting')
    grants = [(1, SPELL), (1, fid('Divine Domain')), (2, fid('Channel Divinity')), (4, fid('Ability Score Improvement')),
              (5, fid('Destroy Undead')), (10, fid('Divine Intervention'))]
    cant = cum([(1, 3), (4, 4), (10, 5)])
    cd = level_table([(1, 0), (2, 1), (6, 2), (18, 3)])
    names = {1: ['Spellcasting', 'Divine Domain'], 2: ['Channel Divinity (1/rest)', 'Divine Domain feature'],
             4: ['Ability Score Improvement'], 5: ['Destroy Undead (CR 1/2)'], 6: ['Channel Divinity (2/rest)', 'Divine Domain feature'],
             8: ['Ability Score Improvement', 'Destroy Undead (CR 1)', 'Divine Domain feature'], 10: ['Divine Intervention'],
             11: ['Destroy Undead (CR 2)'], 12: ['Ability Score Improvement'], 14: ['Destroy Undead (CR 3)'],
             16: ['Ability Score Improvement'], 17: ['Destroy Undead (CR 4)', 'Divine Domain feature'],
             18: ['Channel Divinity (3/rest)'], 19: ['Ability Score Improvement'], 20: ['Divine Intervention improvement']}
    cols = [('Cantrips Known', cant)]
    for n in range(1, 10):
        cols.append((f'{ordinal(n)}', {l: (FULL_SLOTS[l][n - 1] if len(FULL_SLOTS[l]) >= n else '—') for l in range(1, 21)}))
    mc = multiclass_node(mc_id, '[wis:13]', 'Wisdom 13', 'Light armor, medium armor, shields',
                         [grant(ARMOR['light'], 'Proficiency'), grant(ARMOR['medium'], 'Proficiency'), grant(ARMOR['shields'], 'Proficiency')])
    els.append(class_element(CLS, cid(CLS, 'Class', CLS), 'd8', 'A priestly champion who wields divine magic in service of a higher power.',
        [grant(ARMOR[k], 'Proficiency') for k in ('light', 'medium', 'shields')] + [grant(WEAPON['simple'], 'Proficiency'),
         grant(save('wisdom'), 'Proficiency'), grant(save('charisma'), 'Proficiency')],
        mc, grants, 2, 'Cleric', class_description(CLS, class_table(CLS, names, cols))))

    els.append(spellcasting_feature(CLS, 'Wisdom', spell_html(R), FULL_SLOTS, cant, prepare=True,
        focus_id='ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_HOLY_SYMBOL',
        sheet_text='You can prepare {{cleric:spellcasting:prepare}} cleric spells each day. Wisdom is your spellcasting ability; you can use a holy symbol as a spellcasting focus and cast cleric spells as rituals.',
        id_=SPELL))
    els.append(class_feature(CLS, 'Divine Domain', html=H('Divine Domain', 21, True), sheet_={'display': False},
                             rules=[select('Archetype', 'Divine Domain', supports=TAG, level=1)]))
    els.append(class_feature(CLS, 'Channel Divinity', html=H('Channel Divinity', 21, True),
        rules=step_stats('channel divinity:count', cd, bonus='base'),
        sheet_=sheet(action='Action', usage='{{channel divinity:count}}/Short or Long Rest',
                     text='Turn Undead: each undead within 30 feet that can see or hear you must make a Wisdom saving throw (DC {{spellcasting:dc:wis}}) or be turned for 1 minute.')))
    els.append(asi_feature(CLS, ASI_LEVELS, H('Ability Score Improvement')))
    crs = {5: '1/2', 8: '1', 11: '2', 14: '3', 17: '4'}
    els.append(class_feature(CLS, 'Destroy Undead', html=H('Destroy Undead'), sheet_=sheet(
        entries=[{'text': f'Undead of CR {c} or lower are destroyed when they fail the saving throw against Turn Undead.', 'level': l} for l, c in crs.items()])))
    els.append(class_feature(CLS, 'Divine Intervention', html=H('Divine Intervention'), sheet_=sheet(
        action='Action', usage='1/7 days', text='Implore your deity for aid; the chance of success equals your cleric level in percent.',
        entries=[{'text': 'Your call for intervention succeeds automatically, no roll required.', 'level': 20}])))

    ar = find_in(R, 'Life Domain', 21)
    A = lambda t: feature_html(find_in(R, t, 18, ar))
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    dom = [(1, 'Bless'), (1, 'Cure Wounds'), (3, 'Lesser Restoration'), (3, 'Spiritual Weapon'), (5, 'Beacon of Hope'),
           (5, 'Revivify'), (7, 'Death Ward'), (7, 'Guardian of Faith'), (9, 'Mass Cure Wounds'), (9, 'Raise Dead')]
    cg = [(1, af('Bonus Proficiency')), (1, af('Disciple of Life')), (1, 'ID_WOTC_PHB_ARCHETYPE_FEATURE_LIFE_DOMAIN_SPELLS'),
          (2, af('Channel Divinity: Preserve Life')), (6, af('Blessed Healer')), (8, af('Divine Strike')), (17, af('Supreme Healing'))]
    els.append(archetype_element(CLS, 'Life Domain', TAG, feature_html(ar), cg))
    els.append(element('ID_WOTC_PHB_ARCHETYPE_FEATURE_LIFE_DOMAIN_SPELLS', 'Domain Spells', 'Archetype Feature',
        sec(R, 'Domain Spells', 18, False) + '<p>Life domain spells: 1st bless, cure wounds; 3rd lesser restoration, spiritual weapon; 5th beacon of hope, revivify; 7th death ward, guardian of faith; 9th mass cure wounds, raise dead.</p>',
        rules=spell_grants(dom, CLS), sheet={'display': False}))
    els.append(class_feature(CLS, 'Bonus Proficiency', html=A('Bonus Proficiency'), ty='Archetype Feature', rules=[grant(ARMOR['heavy'], 'Proficiency')]))
    els.append(class_feature(CLS, 'Disciple of Life', html=A('Disciple of Life'), ty='Archetype Feature'))
    els.append(class_feature(CLS, 'Channel Divinity: Preserve Life', html=A('Channel Divinity: Preserve Life'), ty='Archetype Feature',
        sheet_=sheet(action='Action', text='Restore up to {{level:cleric}}x5 hit points, divided among creatures within 30 feet (no more than half their maximum).')))
    els.append(class_feature(CLS, 'Blessed Healer', html=A('Blessed Healer'), ty='Archetype Feature'))
    els.append(class_feature(CLS, 'Divine Strike', html=A('Divine Strike'), ty='Archetype Feature', sheet_=sheet(
        text='Once on each of your turns, a weapon attack deals an extra 1d8 radiant damage.',
        entries=[{'text': 'Once on each of your turns, a weapon attack deals an extra 2d8 radiant damage.', 'level': 14}])))
    els.append(class_feature(CLS, 'Supreme Healing', html=A('Supreme Healing'), ty='Archetype Feature'))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/cleric.json', els, name='SRD 5.1 Cleric'))
    print('non-legacy ids:', ids_not_in_fixtures(els))
