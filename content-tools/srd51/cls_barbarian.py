"""Barbarian and Path of the Berserker (SRD 5.1)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_caster import *

CLS = 'Barbarian'
TAG = 'Primal Path'

def build():
    R = class_range(CLS)
    H = lambda t, size=21, sub=False: sec(R, t, size, sub)
    mc_id = 'ID_WOTC_PHB_MULTICLASS_BARBARIAN'
    els = []
    fid = lambda n: cid(CLS, 'Class Feature', n)
    PERSIST = 'ID_WOTC_PHB_CLASS_FEATURE_BARBARIAN_PERSISTANT_RAGE'
    grants = [(1, fid('Rage')), (1, fid('Unarmored Defense')), (2, fid('Reckless Attack')), (2, fid('Danger Sense')),
              (3, fid('Primal Path')), (4, fid('Ability Score Improvement')), (5, fid('Extra Attack')),
              (5, fid('Fast Movement')), (7, fid('Feral Instinct')), (9, fid('Brutal Critical')),
              (11, fid('Relentless Rage')), (15, PERSIST), (18, fid('Indomitable Might')), (20, fid('Primal Champion'))]
    rage_n = level_table([(1, 2), (3, 3), (6, 4), (12, 5), (17, 6)])
    rage_d = level_table([(1, 2), (9, 3), (16, 4)])
    names = {1: ['Rage', 'Unarmored Defense'], 2: ['Reckless Attack', 'Danger Sense'], 3: ['Primal Path'],
             4: ['Ability Score Improvement'], 5: ['Extra Attack', 'Fast Movement'], 6: ['Path feature'],
             7: ['Feral Instinct'], 8: ['Ability Score Improvement'], 9: ['Brutal Critical (1 die)'],
             10: ['Path feature'], 11: ['Relentless Rage'], 12: ['Ability Score Improvement'],
             13: ['Brutal Critical (2 dice)'], 14: ['Path feature'], 15: ['Persistent Rage'],
             16: ['Ability Score Improvement'], 17: ['Brutal Critical (3 dice)'], 18: ['Indomitable Might'],
             19: ['Ability Score Improvement'], 20: ['Primal Champion']}
    cols = [('Rages', {l: ('Unlimited' if l == 20 else rage_n[l]) for l in range(1, 21)}),
            ('Rage Damage', {l: f'+{rage_d[l]}' for l in range(1, 21)})]
    mc = multiclass_node(mc_id, '[str:13]', 'Strength 13', 'Shields, simple weapons, martial weapons',
                         [grant(ARMOR['shields'], 'Proficiency'), grant(WEAPON['simple'], 'Proficiency'), grant(WEAPON['martial'], 'Proficiency')])
    els.append(class_element(CLS, cid(CLS, 'Class', CLS), 'd12', 'A fierce warrior of primitive background who can enter a battle rage.',
        [grant(ARMOR[k], 'Proficiency') for k in ('light', 'medium', 'shields')] + [grant(WEAPON[k], 'Proficiency') for k in ('simple', 'martial')] +
        [grant(save('strength'), 'Proficiency'), grant(save('constitution'), 'Proficiency')],
        mc, grants, 2, 'Barbarian', class_description(CLS, class_table(CLS, names, cols))))

    rage_rules = step_stats('barbarian rage:count', rage_n) + step_stats('barbarian rage:damage', rage_d)
    els.append(class_feature(CLS, 'Rage', html=H('Rage'), rules=rage_rules,
        sheet_=sheet(action='Bonus Action', usage='{{barbarian rage:count}}/Long Rest',
                     text='Advantage on Strength checks and Strength saving throws; +{{barbarian rage:damage}} damage on Strength-based melee weapon attacks; resistance to bludgeoning, piercing and slashing damage. Lasts 1 minute.',
                     entries=[{'text': 'Advantage on Strength checks and Strength saving throws; +{{barbarian rage:damage}} damage on Strength-based melee weapon attacks; resistance to bludgeoning, piercing and slashing damage. Lasts 1 minute.', 'usage': 'Unlimited', 'level': 20}])))
    ud = 'ac:unarmored defense barbarian'
    els.append(class_feature(CLS, 'Unarmored Defense', html=H('Unarmored Defense'), rules=[
        stat(f'{ud}:base', 10), stat(f'{ud}:dexterity', 'dexterity:modifier'), stat(f'{ud}:constitution', 'constitution:modifier'),
        stat(ud, f'{ud}:base'), stat(ud, f'{ud}:dexterity'), stat(ud, f'{ud}:constitution'),
        stat('ac:calculation', ud, bonus='calculation', equipped='[armor:none]', alt='Unarmored Defense (Barbarian)')]))
    els.append(class_feature(CLS, 'Reckless Attack', html=H('Reckless Attack')))
    els.append(class_feature(CLS, 'Danger Sense', html=H('Danger Sense')))
    els.append(class_feature(CLS, 'Primal Path', html=H('Primal Path'), sheet_={'display': False},
                             rules=[select('Archetype', 'Primal Path', supports=TAG, level=3)]))
    els.append(asi_feature(CLS, ASI_LEVELS, H('Ability Score Improvement')))
    els.append(class_feature(CLS, 'Extra Attack', html=H('Extra Attack'), rules=[stat('extra attack:count', 2, level=5, bonus='extra attack')]))
    sp = ['', ':climb', ':fly', ':swim', ':burrow']
    els.append(class_feature(CLS, 'Fast Movement', html=H('Fast Movement'), rules=[
        stat(f'innate speed{s}:misc', 10, equipped='![armor:heavy]', requirements=f'[innate speed{s}:1]', level=5) for s in sp]))
    els.append(class_feature(CLS, 'Feral Instinct', html=H('Feral Instinct'), rules=[grant('ID_INTERNAL_GRANTS_INITIATIVE_ADVANTAGE', 'Grants')]))
    els.append(class_feature(CLS, 'Brutal Critical', html=H('Brutal Critical'), sheet_=sheet(entries=[
        {'text': 'Roll one additional weapon damage die on a melee critical hit.', 'level': 9},
        {'text': 'Roll two additional weapon damage dice on a melee critical hit.', 'level': 13},
        {'text': 'Roll three additional weapon damage dice on a melee critical hit.', 'level': 17}])))
    els.append(class_feature(CLS, 'Relentless Rage', html=H('Relentless Rage')))
    els.append(class_feature(CLS, 'Persistent Rage', html=H('Persistent Rage'), id_=PERSIST))
    els.append(class_feature(CLS, 'Indomitable Might', html=H('Indomitable Might')))
    els.append(class_feature(CLS, 'Primal Champion', html=H('Primal Champion'), rules=[
        stat('strength', 4), stat('constitution', 4), stat('strength:max', 4), stat('constitution:max', 4)]))

    ar = find_in(R, 'Path of the Berserker', 21)
    A = lambda t: feature_html(find_in(R, t, 18, ar))
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    cg = [(3, af('Frenzy')), (6, af('Mindless Rage')), (10, af('Intimidating Presence')), (14, af('Retaliation'))]
    els.append(archetype_element(CLS, 'Path of the Berserker', TAG, feature_html(ar), cg))
    els.append(class_feature(CLS, 'Frenzy', html=A('Frenzy'), ty='Archetype Feature'))
    els.append(class_feature(CLS, 'Mindless Rage', html=A('Mindless Rage'), ty='Archetype Feature'))
    els.append(class_feature(CLS, 'Intimidating Presence', html=A('Intimidating Presence'), ty='Archetype Feature',
        rules=dc_stats('intimidating presence', 'charisma'),
        sheet_=sheet(action='Action', text='Frighten a creature within 30 feet (Wisdom save DC {{intimidating presence:dc}}).')))
    els.append(class_feature(CLS, 'Retaliation', html=A('Retaliation'), ty='Archetype Feature', sheet_=sheet(action='Reaction', text=first_sentences(A('Retaliation')))))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/barbarian.json', els, name='SRD 5.1 Barbarian'))
    print('non-legacy ids:', ids_not_in_fixtures(els))
