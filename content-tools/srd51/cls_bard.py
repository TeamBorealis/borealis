"""Bard and College of Lore (SRD 5.1)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_caster import *

CLS = 'Bard'
TAG = 'Bard College'

def build():
    R = class_range(CLS)
    H = lambda t, size=21, sub=False: sec(R, t, size, sub)
    mc_id = 'ID_WOTC_PHB_MULTICLASS_BARD'
    els = []
    fid = lambda n: cid(CLS, 'Class Feature', n)
    SPELL = fid('Spellcasting')
    grants = [(1, SPELL), (1, fid('Bardic Inspiration')), (2, fid('Jack of All Trades')), (2, fid('Song of Rest')),
              (3, fid('Bard College')), (3, fid('Expertise')), (4, fid('Ability Score Improvement')),
              (5, fid('Font of Inspiration')), (6, fid('Countercharm')), (10, fid('Magical Secrets')),
              (20, fid('Superior Inspiration'))]
    die = level_table([(1, 6), (5, 8), (10, 10), (15, 12)])
    rest = level_table([(1, 6), (9, 8), (13, 10), (17, 12)])
    cant = cum([(1, 2), (4, 3), (10, 4)])
    known = cum([(i + 1, v) for i, v in enumerate([4, 5, 6, 7, 8, 9, 10, 11, 12, 14, 15, 15, 16, 18, 19, 19, 20, 22, 22, 22])])
    slots = FULL_SLOTS
    names = {1: ['Spellcasting', 'Bardic Inspiration (d6)'], 2: ['Jack of All Trades', 'Song of Rest (d6)'],
             3: ['Bard College', 'Expertise'], 4: ['Ability Score Improvement'], 5: ['Bardic Inspiration (d8)', 'Font of Inspiration'],
             6: ['Countercharm', 'Bard College feature'], 8: ['Ability Score Improvement'], 9: ['Song of Rest (d8)'],
             10: ['Bardic Inspiration (d10)', 'Expertise', 'Magical Secrets'], 12: ['Ability Score Improvement'],
             13: ['Song of Rest (d10)'], 14: ['Magical Secrets', 'Bard College feature'], 15: ['Bardic Inspiration (d12)'],
             16: ['Ability Score Improvement'], 17: ['Song of Rest (d12)'], 18: ['Magical Secrets'],
             19: ['Ability Score Improvement'], 20: ['Superior Inspiration']}
    cols = [('Cantrips Known', cant), ('Spells Known', known)]
    for n in range(1, 10):
        cols.append((f'{ordinal(n)}', {l: (slots[l][n - 1] if len(slots[l]) >= n else '—') for l in range(1, 21)}))
    mc = multiclass_node(mc_id, '[cha:13]', 'Charisma 13', 'Light armor, one skill of your choice, one musical instrument of your choice',
                         [grant(ARMOR['light'], 'Proficiency'),
                          select('Proficiency', 'Skill Proficiency (Bard Multiclass)', supports='Skill,Bard'),
                          select('Proficiency', 'Tool Proficiency (Bard Multiclass)', supports='Musical Instrument')])
    els.append(class_element(CLS, cid(CLS, 'Class', CLS), 'd8', 'An inspiring magician whose power echoes the music of creation.',
        [grant(ARMOR['light'], 'Proficiency'), grant(WEAPON['simple'], 'Proficiency')] +
        [grant(wprof(w), 'Proficiency') for w in ('crossbow hand', 'longsword', 'rapier', 'shortsword')] +
        [grant(save('dexterity'), 'Proficiency'), grant(save('charisma'), 'Proficiency')],
        mc, grants, 3, 'Bard', class_description(CLS, class_table(CLS, names, cols)),
        tool_selects=[select('Proficiency', 'Tool Proficiency (Bard)', supports='Musical Instrument', number=3, requirements=f'!{mc_id}')]))

    # The Spells Known column counts the two Magical Secrets spells gained at 10, 14 and 18; those
    # are picked in the Magical Secrets selects, so the Spell Known selects leave them out.
    secrets = (10, 14, 18)
    known_picks = {l: v - 2 * sum(1 for m in secrets if l >= m) for l, v in known.items()}
    els.append(spellcasting_feature(CLS, 'Charisma', spell_html(R), slots, cant, known=known_picks,
        focus_id='ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_MUSICAL_INSTRUMENT',
        sheet_text='Charisma is your spellcasting ability; you can use a musical instrument as a spellcasting focus.', id_=SPELL))
    els.append(class_feature(CLS, 'Bardic Inspiration', html=H('Bardic Inspiration'),
        rules=step_stats('bardic inspiration:die', die, bonus='bardic inspiration') + [stat('bardic inspiration:count', 'charisma:modifier')],
        sheet_=sheet(action='Bonus Action', usage='{{bardic inspiration:count}}/Long Rest',
                     text='Give a creature within 60 feet a Bardic Inspiration die (d{{bardic inspiration:die}}) to add to one ability check, attack roll or saving throw.',
                     entries=[{'text': 'Give a creature within 60 feet a Bardic Inspiration die (d{{bardic inspiration:die}}) to add to one ability check, attack roll or saving throw.', 'usage': '{{bardic inspiration:count}}/Short or Long Rest', 'level': 5}])))
    joat = [stat(f'{s}:misc', 'proficiency:half', bonus='jack of all trades', requirements=f'!{skill_id(s)}', level=2,
                 alt='Jack of All Trades') for s in SKILLS_LOW]
    joat.append(stat('initiative:misc', 'proficiency:half', bonus='jack of all trades', level=2, alt='Jack of All Trades'))
    els.append(class_feature(CLS, 'Jack of All Trades', html=H('Jack of All Trades'), rules=joat))
    els.append(class_feature(CLS, 'Song of Rest', html=H('Song of Rest'), rules=step_stats('song of rest:die', rest, bonus='song-of-rest'),
        sheet_=sheet(text='Allies who regain hit points at the end of a short rest regain an extra d{{song of rest:die}}.')))
    els.append(class_feature(CLS, 'Bard College', html=H('Bard College'), sheet_={'display': False},
                             rules=[select('Archetype', 'Bard College', supports=TAG, level=3)]))
    els.append(class_feature(CLS, 'Expertise', html=H('Expertise'), rules=[
        select('Class Feature', 'Expertise (Bard 3)', supports='ID_INTERNAL_SUPPORT_EXPERTISE', number=2, level=3),
        select('Class Feature', 'Expertise (Bard 10)', supports='ID_INTERNAL_SUPPORT_EXPERTISE', number=2, level=10)]))
    els.append(asi_feature(CLS, ASI_LEVELS, H('Ability Score Improvement')))
    els.append(class_feature(CLS, 'Font of Inspiration', html=H('Font of Inspiration')))
    els.append(class_feature(CLS, 'Countercharm', html=H('Countercharm'), sheet_=sheet(action='Action', text=first_sentences(H('Countercharm')))))
    ms = lambda lv, nm: select('Spell', nm, supports='$(spellcasting:slots)', number=2, level=lv, spellcasting=CLS)
    els.append(class_feature(CLS, 'Magical Secrets', html=H('Magical Secrets'), rules=[
        ms(10, 'Magical Secrets (Bard 10)'), ms(14, 'Magical Secrets (Bard 14)'), ms(18, 'Magical Secrets (Bard 18)')]))
    els.append(class_feature(CLS, 'Superior Inspiration', html=H('Superior Inspiration')))

    ar = find_in(R, 'College of Lore', 21)
    A = lambda t: feature_html(find_in(R, t, 18, ar))
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    cg = [(3, af('Bonus Proficiencies')), (3, af('Cutting Words')), (6, af('Additional Magical Secrets')), (14, af('Peerless Skill'))]
    els.append(archetype_element(CLS, 'College of Lore', TAG, feature_html(ar), cg))
    els.append(class_feature(CLS, 'Bonus Proficiencies', html=A('Bonus Proficiencies'), ty='Archetype Feature',
        rules=[select('Proficiency', 'Skill Proficiency (College of Lore)', supports='Skill', number=3)]))
    els.append(class_feature(CLS, 'Cutting Words', html=A('Cutting Words'), ty='Archetype Feature', sheet_=sheet(action='Reaction', text=first_sentences(A('Cutting Words')))))
    els.append(class_feature(CLS, 'Additional Magical Secrets', html=A('Additional Magical Secrets'), ty='Archetype Feature',
        rules=[select('Spell', 'Additional Magical Secrets (College of Lore)', supports='$(spellcasting:slots)', number=2, level=6, spellcasting=CLS)]))
    els.append(class_feature(CLS, 'Peerless Skill', html=A('Peerless Skill'), ty='Archetype Feature'))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/bard.json', els, name='SRD 5.1 Bard'))
    print('non-legacy ids:', ids_not_in_fixtures(els))
