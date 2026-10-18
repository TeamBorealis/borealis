"""Fighter and Champion (SRD 5.1)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_core import *

CLS = 'Fighter'
TAG = 'Martial Archetype'

def build():
    R = class_range(CLS)
    els = []
    mc_id = 'ID_WOTC_PHB_MULTICLASS_FIGHTER'
    H = lambda t, size=21, sub=False: sec(R, t, size, sub)

    # ---- feature ids
    F = {n: cid(CLS, 'Class Feature', n) for n in ['Fighting Style', 'Second Wind', 'Action Surge', 'Martial Archetype',
                                                  'Ability Score Improvement', 'Extra Attack', 'Indomitable']}
    F['Ability Score Improvement'] = 'ID_WOTC_PHB_CLASS_FEATURE_ABILITYSCOREIMPROVEMENT_FIGHTER'
    grants = [(1, F['Fighting Style']), (1, F['Second Wind']), (2, F['Action Surge']), (3, F['Martial Archetype']),
              (4, F['Ability Score Improvement']), (5, F['Extra Attack']), (9, F['Indomitable'])]

    # ---- class element
    names_by_level = {1: ['Fighting Style', 'Second Wind'], 2: ['Action Surge (one use)'], 3: ['Martial Archetype'],
                      4: ['Ability Score Improvement'], 5: ['Extra Attack'], 6: ['Ability Score Improvement'],
                      7: ['Martial Archetype feature'], 8: ['Ability Score Improvement'], 9: ['Indomitable (one use)'],
                      10: ['Martial Archetype feature'], 11: ['Extra Attack (2)'], 12: ['Ability Score Improvement'],
                      13: ['Indomitable (two uses)'], 14: ['Ability Score Improvement'], 15: ['Martial Archetype feature'],
                      16: ['Ability Score Improvement'], 17: ['Action Surge (two uses)', 'Indomitable (three uses)'],
                      18: ['Martial Archetype feature'], 19: ['Ability Score Improvement'], 20: ['Extra Attack (3)']}
    table = class_table(CLS, names_by_level, [])
    mc = multiclass_node(mc_id, '([str:13]||[dex:13])', 'Strength 13 or Dexterity 13',
                         'Light armor, medium armor, shields, simple weapons, martial weapons',
                         [grant(ARMOR['light'], 'Proficiency'), grant(ARMOR['medium'], 'Proficiency'), grant(ARMOR['shields'], 'Proficiency'),
                          grant(WEAPON['simple'], 'Proficiency'), grant(WEAPON['martial'], 'Proficiency')])
    els.append(class_element(
        CLS, cid(CLS, 'Class', CLS), 'd10', 'A master of martial combat, skilled with a variety of weapons and armor.',
        [grant(ARMOR[k], 'Proficiency') for k in ('light', 'medium', 'heavy', 'shields')] +
        [grant(WEAPON[k], 'Proficiency') for k in ('simple', 'martial')] +
        [grant(save('strength'), 'Proficiency'), grant(save('constitution'), 'Proficiency')],
        mc, grants, 2, 'Fighter', class_description(CLS, table)))

    # ---- features
    els.append(class_feature(CLS, 'Fighting Style', html=H('Fighting Style', 21, False),
                             rules=[select('Class Feature', 'Fighting Style', supports='Fighting Style,Fighter', level=1)], sheet_={'display': False}))
    # (fighting style options are shared with Paladin and Ranger through the support tags)
    style_html = {}
    i = find_in(R, 'Fighting Style', 21)
    j = i + 1
    while not (ev[j]['k'] == 'H' and ev[j]['size'] >= 21):
        if ev[j]['k'] == 'H' and ev[j]['size'] == 18:
            style_html[clean(ev[j]['text'])] = feature_html(j)
        j += 1
    styles = {
        'Archery': (['Fighter', 'Ranger'], [stat('ranged:attack', 2)]),
        'Defense': (['Fighter', 'Paladin', 'Ranger'], [stat('ac:misc', 1, bonus='fighting style', equipped='[armor:any]', alt='Defense (Fighting Style)')]),
        'Dueling': (['Fighter', 'Paladin', 'Ranger'], [stat('melee:damage', 2, bonus='fighting style', equipped='[main:weapon],([offhand:none]||[shield:any])', alt='Dueling (Fighting Style)')]),
        'Great Weapon Fighting': (['Fighter', 'Paladin'], []),
        'Protection': (['Fighter', 'Paladin'], []),
        'Two-Weapon Fighting': (['Fighter', 'Ranger'], []),
    }
    for sname, (classes, rules) in styles.items():
        els.append(class_feature(CLS, sname, html=style_html[sname], ty='Class Feature', rules=rules,
                                 id_=f'ID_WOTC_PHB_CLASS_FEATURE_FIGHTINGSTYLE_{sname.upper().replace(" ", "_").replace("-", "_")}',
                                 supports=['Fighting Style'] + classes))
    els.append(class_feature(CLS, 'Second Wind', html=H('Second Wind'),
                             sheet_=sheet(usage='1/Short Rest', action='Bonus Action',
                                          text='You regain 1d10+{{level:fighter}} hit points.')))
    els.append(class_feature(CLS, 'Action Surge', html=H('Action Surge'),
                             sheet_=sheet(usage='1/Short Rest', text='On your turn, you can take one additional action on top of your regular action and a possible bonus action.',
                                          entries=[{'text': 'On your turn, you can take one additional action on top of your regular action and a possible bonus action.', 'usage': '2/Short Rest', 'level': 17}])))
    els.append(class_feature(CLS, 'Martial Archetype', html=H('Martial Archetype'),
                             rules=[select('Archetype', 'Martial Archetype', supports=TAG, level=3)], sheet_={'display': False}))
    els.append(asi_feature(CLS, ASI_LEVELS_F, H('Ability Score Improvement'), id_=F['Ability Score Improvement']))
    els.append(class_feature(CLS, 'Extra Attack', html=H('Extra Attack'),
                             rules=[stat('extra attack:count', 2, level=5, bonus='extra attack'),
                                    stat('extra attack:count', 3, level=11, bonus='extra attack'),
                                    stat('extra attack:count', 4, level=20, bonus='extra attack')],
                             sheet_=sheet(entries=[
                                 {'text': 'You can attack twice, instead of once, whenever you take the Attack action on your turn.', 'level': 5},
                                 {'text': 'You can attack three times whenever you take the Attack action on your turn.', 'level': 11},
                                 {'text': 'You can attack four times whenever you take the Attack action on your turn.', 'level': 20}])))
    els.append(class_feature(CLS, 'Indomitable', html=H('Indomitable'),
                             rules=[stat('indomitable:usage', 1, level=9), stat('indomitable:usage', 1, level=13), stat('indomitable:usage', 1, level=17)],
                             sheet_=sheet(usage='{{indomitable:usage}}/Long Rest', text='You can reroll a saving throw that you fail. If you do so, you must use the new roll.')))

    # ---- Champion
    # Improved / Superior Critical: the engine's per-attack critical threshold (weapon attacks and
    # unarmed strikes); Remarkable Athlete: half proficiency (rounded up) on the STR/DEX/CON skills
    # the fighter is not proficient in (the Aurora rules) and on initiative.
    def critical_range(n):
        return {'kind': 'critical_range', 'min': n, 'applies_to': ['weapon attacks', 'unarmed strikes']}

    def remarkable_athlete_rules():
        skills = [('acrobatics', 'ACROBATICS'), ('athletics', 'ATHLETICS'),
                  ('sleight of hand', 'SLEIGHTOFHAND'), ('stealth', 'STEALTH')]
        # initiative is a Dexterity check that never uses the proficiency bonus
        return [stat(f'{k}:misc', 'proficiency:half:up', requirements=f'!ID_PROFICIENCY_SKILL_{i}')
                for k, i in skills] + [stat('initiative:misc', 'proficiency:half:up', alt='Remarkable Athlete')]

    ar = find_in(R, 'Champion', 21)
    ARC = lambda t: feature_html(find_in(R, t, 18, ar))
    ch_id = cid(CLS, 'Archetype', 'Champion')
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    cg = [(3, af('Improved Critical')), (7, af('Remarkable Athlete')), (10, af('Additional Fighting Style')),
          (15, af('Superior Critical')), (18, af('Survivor'))]
    els.append(archetype_element(CLS, 'Champion', TAG, feature_html(ar), cg))
    els.append(class_feature(CLS, 'Improved Critical', html=ARC('Improved Critical'), ty='Archetype Feature',
                             rules=[critical_range(19)]))
    els.append(class_feature(CLS, 'Remarkable Athlete', html=ARC('Remarkable Athlete'), ty='Archetype Feature',
                             rules=remarkable_athlete_rules()))
    els.append(class_feature(CLS, 'Additional Fighting Style', html=ARC('Additional Fighting Style'), ty='Archetype Feature',
                             rules=[select('Class Feature', 'Additional Fighting Style (Champion)', supports='Fighting Style,Fighter', level=10)]))
    els.append(class_feature(CLS, 'Superior Critical', html=ARC('Superior Critical'), ty='Archetype Feature',
                             rules=[critical_range(18)]))
    els.append(class_feature(CLS, 'Survivor', html=ARC('Survivor'), ty='Archetype Feature'))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/fighter.json', els, name='SRD 5.1 Fighter'))
