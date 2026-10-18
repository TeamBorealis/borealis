"""Monk and Way of the Open Hand (SRD 5.1)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_dhelp import *

CLS = 'Monk'
TAG = 'Monastic Tradition'

def build():
    R = class_range(CLS)
    els = []
    mc_id = 'ID_WOTC_PHB_MULTICLASS_MONK'
    H = lambda t, sub=False: feat2(R, t, 21, include_sub=sub)
    F = lambda n: cid(CLS, 'Class Feature', n)
    ASI_ID = 'ID_WOTC_PHB_CLASS_FEATURE_MONK_ABILITYSCOREIMPROVEMENT_MONK'
    ki_subs = ['Flurry of Blows', 'Patient Defense', 'Step of the Wind']
    grants = [(1, F('Unarmored Defense')), (1, F('Martial Arts')), (2, F('Ki')), (2, F('Unarmored Movement')),
              (3, F('Monastic Tradition')), (3, F('Deflect Missiles')), (4, ASI_ID), (4, F('Slow Fall')),
              (5, F('Extra Attack')), (5, F('Stunning Strike')), (6, F('Ki-Empowered Strikes')),
              (7, F('Evasion')), (7, F('Stillness of Mind')), (10, F('Purity of Body')),
              (13, F('Tongue of the Sun and Moon')), (14, F('Diamond Soul')), (15, F('Timeless Body')),
              (18, F('Empty Body')), (20, F('Perfect Self'))]

    na = {1: ['Unarmored Defense', 'Martial Arts'], 2: ['Ki', 'Unarmored Movement'], 3: ['Monastic Tradition', 'Deflect Missiles'],
          4: ['Ability Score Improvement', 'Slow Fall'], 5: ['Extra Attack', 'Stunning Strike'],
          6: ['Ki-Empowered Strikes', 'Tradition feature'], 7: ['Evasion', 'Stillness of Mind'], 8: ['Ability Score Improvement'],
          9: ['Unarmored Movement improvement'], 10: ['Purity of Body'], 11: ['Tradition feature'],
          12: ['Ability Score Improvement'], 13: ['Tongue of the Sun and Moon'], 14: ['Diamond Soul'], 15: ['Timeless Body'],
          16: ['Ability Score Improvement'], 17: ['Tradition feature'], 18: ['Empty Body'],
          19: ['Ability Score Improvement'], 20: ['Perfect Self']}
    ma = level_table([(1, '1d4'), (5, '1d6'), (11, '1d8'), (17, '1d10')])
    ki = {l: (l if l >= 2 else '—') for l in range(1, 21)}
    um = {l: ('—' if l < 2 else '+%d ft.' % v) for l, v in level_table([(2, 10), (6, 15), (10, 20), (14, 25), (18, 30)], 0).items()}
    table = class_table(CLS, na, [('Martial Arts', ma), ('Ki Points', ki), ('Unarmored Movement', um)])

    mc = multiclass_node(mc_id, '([dex:13],[wis:13])', 'Dexterity 13 and Wisdom 13',
                         'Simple weapons, shortswords',
                         [grant(WEAPON['simple'], 'Proficiency'), grant(wprof('Shortsword'), 'Proficiency')])
    els.append(class_element(
        CLS, cid(CLS, 'Class', CLS), 'd8', 'A master of martial arts, harnessing the power of the body in pursuit of physical and spiritual perfection.',
        [grant(WEAPON['simple'], 'Proficiency'), grant(wprof('Shortsword'), 'Proficiency'),
         grant(save('strength'), 'Proficiency'), grant(save('dexterity'), 'Proficiency')],
        mc, grants, 2, 'Monk', class_desc2(CLS, table), tool_selects=[
            select('Proficiency', 'Tool Proficiency (Monk)', supports='Musical Instrument||Artisan tools', requirements='!' + mc_id)]))
    # the Martial Arts column, read by the engine's martial_arts rule (index = monk level - 1)
    els[-1]['data'] = {'progression': {'martial_arts': [ma[l] for l in range(1, 21)]}}

    # Unarmored Defense
    els.append(class_feature(CLS, 'Unarmored Defense', html=H('Unarmored Defense'), rules=[
        stat('ac:unarmored defense monk:base', 10),
        stat('ac:unarmored defense monk:dexterity', 'dexterity:modifier'),
        stat('ac:unarmored defense monk:wisdom', 'wisdom:modifier'),
        stat('ac:unarmored defense monk', 'ac:unarmored defense monk:base'),
        stat('ac:unarmored defense monk', 'ac:unarmored defense monk:dexterity'),
        stat('ac:unarmored defense monk', 'ac:unarmored defense monk:wisdom'),
        stat('ac:calculation', 'ac:unarmored defense monk', bonus='calculation', equipped='[armor:none],[shield:none]',
             alt='Unarmored Defense (Monk)')]))
    # Martial Arts
    ma_rules = [stat('martial arts:attack', 'strength:modifier', bonus='martial arts'),
                stat('martial arts:attack', 'dexterity:modifier', bonus='martial arts'),
                stat('martial arts:damage', 'strength:modifier', bonus='martial arts'),
                stat('martial arts:damage', 'dexterity:modifier', bonus='martial arts')]
    for lvl, d in ((1, 4), (5, 6), (11, 8), (17, 10)):
        ma_rules.append(stat('martial arts:dice', d, bonus='martial arts', level=lvl))
    # SRD 5.1: monk weapons are shortswords and simple melee weapons without two-handed or heavy;
    # Dexterity for them and unarmed strikes, the Martial Arts die, a bonus unarmed strike, all while
    # wearing no armor and wielding no shield (the 5.2.1 pack's martial_arts rule, 2014 weapons).
    ma_rules.append({'kind': 'martial_arts', 'die_table': 'martial_arts', 'bonus_unarmed_strike': True,
                     'dexterous_attacks': True,
                     'monk_weapons': {'simple_melee_without': ['Two-Handed', 'Heavy'], 'named': ['Shortsword']},
                     'conditions': {'no_armor': True, 'no_shield': True}})
    els.append(class_feature(CLS, 'Martial Arts', html=H('Martial Arts'), rules=ma_rules,
                             sheet_=sheet(text='Use Dexterity for unarmed strikes and monk weapons; unarmed damage die d{{martial arts:dice}}; one unarmed strike as a bonus action after the Attack action.')))
    # Ki
    ki_html = H('Ki')
    els.append(class_feature(CLS, 'Ki', html=ki_html,
                             rules=[stat('ki:points', 'level:monk')] + dc_stats('ki', 'wisdom') +
                                   [grant(F(n), 'Class Feature') for n in ki_subs],
                             sheet_=sheet(usage='{{ki:points}}/Short Rest', text='Ki save DC {{ki:dc}}.')))
    ki_i = find_in(R, 'Ki', 21)
    sub = {}
    j = ki_i + 1
    while not (ev[j]['k'] == 'H' and ev[j]['size'] >= 21):
        if ev[j]['k'] == 'H':
            sub[clean(ev[j]['text'])] = feature_html(j)
        j += 1
    for n in ki_subs:
        els.append(class_feature(CLS, n, html=sub[n], sheet_=sheet(action='Bonus Action', usage='1 ki point',
                                                                  text=first_sentences(sub[n]))))
    # Unarmored Movement
    um_t = level_table([(2, 10), (6, 15), (10, 20), (14, 25), (18, 30)], 0)
    um_rules = [stat('monk:unarmored movement', v, bonus='unarmored movement', level=l)
                for l, v in ((2, 10), (6, 15), (10, 20), (14, 25), (18, 30))]
    for sp in ('', ':climb', ':fly', ':swim', ':burrow'):
        um_rules.append(stat(f'innate speed{sp}:misc', 'monk:unarmored movement', bonus='unarmored movement',
                             equipped='[armor:none],[shield:none]', requirements=f'[innate speed{sp}:1]'))
    els.append(class_feature(CLS, 'Unarmored Movement', html=H('Unarmored Movement'), rules=um_rules))
    els.append(class_feature(CLS, 'Monastic Tradition', html=H('Monastic Tradition'), sheet_={'display': False},
                             rules=[select('Archetype', 'Monastic Tradition', supports=TAG, level=3)]))
    els.append(
               class_feature(CLS, 'Deflect Missiles', html=H('Deflect Missiles'),
                             rules=[stat('monk:deflect missiles bonus', 'dexterity:modifier'), stat('monk:deflect missiles bonus', 'level:monk')],
                             sheet_=sheet(action='Reaction', text='Reduce ranged weapon damage by 1d10 + {{monk:deflect missiles bonus}}.')))
    els.append(asi_feature(CLS, ASI_LEVELS, H('Ability Score Improvement'), id_=ASI_ID))
    els.append(class_feature(CLS, 'Slow Fall', html=H('Slow Fall'), rules=[stat('monk:slow fall', 'level:monk') for _ in range(5)],
                             sheet_=sheet(action='Reaction', text='Reduce falling damage by {{monk:slow fall}}.')))
    els.append(class_feature(CLS, 'Extra Attack', html=H('Extra Attack'),
                             rules=[stat('extra attack:count', 2, level=5, bonus='extra attack')]))
    els.append(class_feature(CLS, 'Stunning Strike', html=H('Stunning Strike')))
    els.append(class_feature(CLS, 'Ki-Empowered Strikes', html=H('Ki-Empowered Strikes')))
    els.append(class_feature(CLS, 'Evasion', html=H('Evasion')))
    els.append(class_feature(CLS, 'Stillness of Mind', html=H('Stillness of Mind'), sheet_=sheet(action='Action')))
    els.append(class_feature(CLS, 'Purity of Body', html=H('Purity of Body'),
                             rules=[grant('ID_INTERNAL_CONDITION_DAMAGE_IMMUNITY_POISON', 'Condition')]))
    els.append(class_feature(CLS, 'Tongue of the Sun and Moon', html=H('Tongue of the Sun and Moon')))
    els.append(class_feature(CLS, 'Diamond Soul', html=H('Diamond Soul'),
                             rules=[grant(save(a), 'Proficiency') for a in ABIL]))
    els.append(class_feature(CLS, 'Timeless Body', html=H('Timeless Body')))
    els.append(class_feature(CLS, 'Empty Body', html=H('Empty Body'), sheet_=sheet(action='Action', usage='4 ki points')))
    els.append(class_feature(CLS, 'Perfect Self', html=H('Perfect Self')))

    # ---- Way of the Open Hand
    ar = find_in(R, 'Way of the Open Hand', 21)
    ARC = lambda t: feature_html(find_in(R, t, 18, ar))
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    cg = [(3, af('Open Hand Technique')), (6, af('Wholeness of Body')), (11, af('Tranquility')), (17, af('Quivering Palm'))]
    els.append(archetype_element(CLS, 'Way of the Open Hand', TAG, feature_html(ar), cg))
    els.append(class_feature(CLS, 'Open Hand Technique', html=ARC('Open Hand Technique'), ty='Archetype Feature'))
    els.append(class_feature(CLS, 'Wholeness of Body', html=ARC('Wholeness of Body'), ty='Archetype Feature',
                             rules=[stat('monk:wholeness of body', 'level:monk') for _ in range(3)],
                             sheet_=sheet(action='Action', usage='1/Long Rest', text='Regain {{monk:wholeness of body}} hit points.')))
    els.append(class_feature(CLS, 'Tranquility', html=ARC('Tranquility'), ty='Archetype Feature', rules=dc_stats('tranquility', 'wisdom'),
                             sheet_=sheet(text='Sanctuary effect after a rest, save DC {{tranquility:dc}}.')))
    els.append(class_feature(CLS, 'Quivering Palm', html=ARC('Quivering Palm'), ty='Archetype Feature'))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/monk.json', els, name='SRD 5.1 Monk'))
