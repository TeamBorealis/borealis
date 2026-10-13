"""Rogue and Thief (SRD 5.1)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_core import *

CLS = 'Rogue'
TAG = 'Roguish Archetype'

def build():
    R = class_range(CLS)
    els = []
    H = lambda t, size=21, sub=False: sec(R, t, size, sub)
    F = lambda n: cid(CLS, 'Class Feature', n)
    grants = [(1, F('Expertise')), (1, F('Sneak Attack')), (1, F("Thieves' Cant")), (2, F('Cunning Action')),
              (3, F('Roguish Archetype')), (4, F('Ability Score Improvement')), (5, F('Uncanny Dodge')),
              (7, F('Evasion')), (11, F('Reliable Talent')), (14, F('Blindsense')), (15, F('Slippery Mind')),
              (18, F('Elusive')), (20, F('Stroke of Luck'))]
    names = {1: ['Expertise', 'Sneak Attack', "Thieves' Cant"], 2: ['Cunning Action'], 3: ['Roguish Archetype'],
             4: ['Ability Score Improvement'], 5: ['Uncanny Dodge'], 6: ['Expertise'], 7: ['Evasion'],
             8: ['Ability Score Improvement'], 9: ['Roguish Archetype feature'], 10: ['Ability Score Improvement'],
             11: ['Reliable Talent'], 12: ['Ability Score Improvement'], 13: ['Roguish Archetype feature'],
             14: ['Blindsense'], 15: ['Slippery Mind'], 16: ['Ability Score Improvement'],
             17: ['Roguish Archetype feature'], 18: ['Elusive'], 19: ['Ability Score Improvement'],
             20: ['Stroke of Luck']}
    sneak = {l: f'{(l + 1) // 2}d6' for l in range(1, 21)}
    table = class_table(CLS, names, [('Sneak Attack', sneak)])
    mc = multiclass_node('ID_WOTC_PHB_MULTICLASS_ROGUE', '[dex:13]', 'Dexterity 13',
                         "Light armor, one skill from the class's skill list, thieves' tools",
                         [grant(ARMOR['light'], 'Proficiency'),
                          grant('ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS', 'Proficiency')],
                         extra_rules=[select('Proficiency', 'Skill Proficiency (Rogue)', supports='Skill,Rogue', number=1)])
    start = [grant(ARMOR['light'], 'Proficiency'), grant(WEAPON['simple'], 'Proficiency')] + \
            [grant(wprof(w), 'Proficiency') for w in ('crossbow hand', 'longsword', 'rapier', 'shortsword')] + \
            [grant('ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS', 'Proficiency'),
             grant(save('dexterity'), 'Proficiency'), grant(save('intelligence'), 'Proficiency')]
    els.append(class_element(CLS, cid(CLS, 'Class', CLS), 'd8',
                             'A scoundrel who uses stealth and trickery to overcome obstacles and enemies.',
                             start, mc, grants, 4, 'Rogue', class_description(CLS, table)))

    els.append(class_feature(CLS, 'Expertise', html=H('Expertise'), sheet_={'display': False}, rules=[
        select('Class Feature', 'Expertise (Rogue)', supports='ID_INTERNAL_SUPPORT_EXPERTISE', level=1, number=2),
        select('Class Feature', 'Expertise (Rogue)', supports='ID_INTERNAL_SUPPORT_EXPERTISE', level=6, number=2)]))
    sa = [stat('sneak-attack:count', 1, level=l) for l in range(1, 20, 2)] + [stat('sneak-attack:die', 6, bonus='die')]
    els.append(class_feature(CLS, 'Sneak Attack', html=H('Sneak Attack'), rules=sa, sheet_=sheet(
        text='Once per turn, deal an extra {{sneak-attack:count}}d{{sneak-attack:die}} damage to one creature you hit with an attack if you have advantage on the attack roll, or if another enemy of the target is within 5 feet of it.')))
    els.append(class_feature(CLS, "Thieves' Cant", html=H("Thieves’ Cant"),
                             rules=[grant('ID_WOTC_PHB_LANGUAGE_THIEVES_CANT', 'Language')]))
    els.append(class_feature(CLS, 'Cunning Action', html=H('Cunning Action'), sheet_=sheet(
        action='Bonus Action', text='Take the Dash, Disengage, or Hide action as a bonus action on each of your turns.')))
    els.append(class_feature(CLS, 'Roguish Archetype', html=H('Roguish Archetype'), sheet_={'display': False},
                             rules=[select('Archetype', 'Roguish Archetype', supports=TAG, level=3)]))
    els.append(asi_feature(CLS, ASI_LEVELS_R, H('Ability Score Improvement'), id_=F('Ability Score Improvement')))
    els.append(class_feature(CLS, 'Uncanny Dodge', html=H('Uncanny Dodge'), sheet_=sheet(action='Reaction')))
    els.append(class_feature(CLS, 'Evasion', html=H('Evasion')))
    els.append(class_feature(CLS, 'Reliable Talent', html=H('Reliable Talent')))
    els.append(class_feature(CLS, 'Blindsense', html=H('Blindsense')))
    els.append(class_feature(CLS, 'Slippery Mind', html=H('Slippery Mind'),
                             rules=[grant(save('wisdom'), 'Proficiency')]))
    els.append(class_feature(CLS, 'Elusive', html=H('Elusive')))
    els.append(class_feature(CLS, 'Stroke of Luck', html=H('Stroke of Luck'), sheet_=sheet(usage='1/Short Rest')))

    ar = find_in(R, 'Thief', 21)
    ARC = lambda t: feature_html(find_in(R, t, 18, ar))
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    cg = [(3, af('Fast Hands')), (3, af('Second-Story Work')), (9, af('Supreme Sneak')),
          (13, af('Use Magic Device')), (17, af('Thief’s Reflexes'))]
    els.append(archetype_element(CLS, 'Thief', TAG, feature_html(ar), cg))
    for n, lv in [('Fast Hands', 3), ('Second-Story Work', 3), ('Supreme Sneak', 9), ('Use Magic Device', 13),
                  ('Thief’s Reflexes', 17)]:
        els.append(class_feature(CLS, n, html=ARC(n), ty='Archetype Feature',
                                 sheet_=sheet(action='Bonus Action') if n == 'Fast Hands' else None))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/rogue.json', els, name='SRD 5.1 Rogue'))
