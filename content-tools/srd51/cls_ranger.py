"""Ranger and Hunter (SRD 5.1)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_dhelp import *

CLS = 'Ranger'
TAG = 'Ranger Archetype'
KNOWN = {2: 2, 3: 3, 4: 3, 5: 4, 6: 4, 7: 5, 8: 5, 9: 6, 10: 6, 11: 7, 12: 7, 13: 8, 14: 8, 15: 9, 16: 9, 17: 10, 18: 10, 19: 11, 20: 11}

# Favored Enemy / Natural Explorer options: the lists the SRD 5.1 text names. The humanoid races are the
# SRD 5.1 player races plus the humanoid kinds of the SRD 5.1 monster stat blocks; Underdark is not an SRD
# 5.1 terrain. Ids align with the legacy SRD data (identifiers only) so Aurora files resolve.
FE_TAG, HUMANOID_TAG, TERRAIN_TAG = 'Favored Enemy', 'Humanoid Favored Enemy', 'Favored Terrain'
FE_TYPES = ['Aberrations', 'Beasts', 'Celestials', 'Constructs', 'Dragons', 'Elementals', 'Fey', 'Fiends', 'Giants',
            'Monstrosities', 'Oozes', 'Plants', 'Undead']
HUMANOIDS = 'Humanoids'
HUMANOID_RACES = ['Dragonborn', 'Dwarves', 'Elves', 'Gnomes', 'Half-Elves', 'Half-Orcs', 'Halflings', 'Humans', 'Tieflings',
                  'Bugbears', 'Gnolls', 'Goblins', 'Grimlock', 'Hobgoblins', 'Kobolds', 'Lizardfolk', 'Merfolk', 'Orcs',
                  'Sahuagin']
TERRAINS = ['Arctic', 'Coast', 'Desert', 'Forest', 'Grassland', 'Mountain', 'Swamp']
_LEGACY = 'elements.system_reference_document.classes.class-ranger.xml'

def option_id(key):
    """Legacy SRD id of a favored enemy/terrain option (`Humanoids_2` = the second Humanoids pick)."""
    rows = [(i, n) for f, t, i, n in fixture_rows() if f == _LEGACY and t == 'Class Feature']
    name, _, nth = key.partition('_')
    hits = [i for i, n in rows if n == name and ('FAVORED_ENEMY' in i or 'NATURAL_EXPLORER_FT' in i)]
    hits = [i for i in hits if not nth or i.endswith(f'_{nth}')]
    if len(hits) != 1:
        raise SystemExit(f'ranger option {key!r}: expected one legacy id, found {hits}')
    return hits[0]

def favored_options():
    opt = lambda name, tag, id_, **kw: class_feature(CLS, name, id_=id_, supports=[tag], auto_sheet=False, **kw)
    els = [opt(n, FE_TAG, option_id(n), sheet_=sheet(text=f'Favored enemy: {n.lower()}.')) for n in FE_TYPES]
    for n in range(1, 4):
        req = option_id(f'{HUMANOIDS}_{n - 1}') if n > 1 else None
        els.append(opt(HUMANOIDS, FE_TAG, option_id(f'{HUMANOIDS}_{n}'), requirements=req,
                       sheet_={'display': False}, compendium={'display': False} if n > 1 else None))
    els += [opt(n, HUMANOID_TAG, option_id(n), sheet_=sheet(text=f'Favored enemy: {n.lower()} (humanoid).')) for n in HUMANOID_RACES]
    els += [opt(n, TERRAIN_TAG, option_id(n), sheet_=sheet(text=f'Favored terrain: {n.lower()}.')) for n in TERRAINS]
    return els

def build():
    R = class_range(CLS)
    els = []
    mc_id = 'ID_WOTC_PHB_MULTICLASS_RANGER'
    H = lambda t, sub=False: feat2(R, t, 21, include_sub=sub)
    F = lambda n: cid(CLS, 'Class Feature', n)
    ASI_ID = F('Ability Score Improvement')
    grants = [(1, F('Favored Enemy')), (1, F('Natural Explorer')), (2, F('Fighting Style')), (2, F('Spellcasting')),
              (3, F('Ranger Archetype')), (3, F('Primeval Awareness')), (4, ASI_ID), (5, F('Extra Attack')),
              (8, F('Land’s Stride')), (10, F('Hide in Plain Sight')), (14, F('Vanish')), (18, F('Feral Senses')),
              (20, F('Foe Slayer'))]
    na = {1: ['Favored Enemy', 'Natural Explorer'], 2: ['Fighting Style', 'Spellcasting'],
          3: ['Ranger Archetype', 'Primeval Awareness'], 4: ['Ability Score Improvement'], 5: ['Extra Attack'],
          6: ['Favored Enemy and Natural Explorer improvements'], 7: ['Ranger Archetype feature'],
          8: ['Ability Score Improvement', 'Land’s Stride'], 10: ['Natural Explorer improvement', 'Hide in Plain Sight'],
          11: ['Ranger Archetype feature'], 12: ['Ability Score Improvement'], 14: ['Favored Enemy improvement', 'Vanish'],
          15: ['Ranger Archetype feature'], 16: ['Ability Score Improvement'], 18: ['Feral Senses'],
          19: ['Ability Score Improvement'], 20: ['Foe Slayer']}
    cols = [('Spells Known', {l: KNOWN.get(l, '—') for l in range(1, 21)})] + \
           [(ordinal(n), {l: HALF_SLOTS[l][n - 1] if l in HALF_SLOTS and len(HALF_SLOTS[l]) >= n else '—' for l in range(1, 21)})
            for n in range(1, 6)]
    table = class_table(CLS, na, cols)
    mc = multiclass_node(mc_id, '([dex:13],[wis:13])', 'Dexterity 13 and Wisdom 13',
                         'Light armor, medium armor, shields, simple weapons, martial weapons',
                         [grant(ARMOR['light'], 'Proficiency'), grant(ARMOR['medium'], 'Proficiency'), grant(ARMOR['shields'], 'Proficiency'),
                          grant(WEAPON['simple'], 'Proficiency'), grant(WEAPON['martial'], 'Proficiency')],
                         extra_rules=[select('Proficiency', 'Skill Proficiency (Ranger)', supports='Skill,Ranger', number=1)])
    els.append(class_element(
        CLS, cid(CLS, 'Class', CLS), 'd10', 'A warrior who uses martial prowess and nature magic to combat threats on the edges of civilization.',
        [grant(ARMOR[k], 'Proficiency') for k in ('light', 'medium', 'shields')] +
        [grant(WEAPON[k], 'Proficiency') for k in ('simple', 'martial')] +
        [grant(save('strength'), 'Proficiency'), grant(save('dexterity'), 'Proficiency')],
        mc, grants, 3, 'Ranger', class_desc2(CLS, table)))

    fe_levels, ne_levels = (1, 6, 14), (1, 6, 10)
    els.append(class_feature(CLS, 'Favored Enemy', html=H('Favored Enemy'),
                             rules=[stat('favored enemy:count', 1), stat('favored enemy:count', 1, level=6), stat('favored enemy:count', 1, level=14)] +
                                   [select('Class Feature', 'Favored Enemy', supports=FE_TAG, level=l) for l in fe_levels] +
                                   [select('Class Feature', 'Favored Enemy, Humanoid', supports=HUMANOID_TAG, number=2, level=l,
                                           requirements=option_id(f'{HUMANOIDS}_{n}'))
                                    for n, l in enumerate(fe_levels, 1)] +
                                   [select('Language', 'Language (Favored Enemy)', level=l, optional=True) for l in fe_levels],
                             sheet_=sheet(text='Favored enemies: {{favored enemy:count}}. Advantage on Survival checks to track them and Intelligence checks to recall information about them.')))
    els.append(class_feature(CLS, 'Natural Explorer', html=H('Natural Explorer'),
                             rules=[stat('natural explorer:count', 1), stat('natural explorer:count', 1, level=6), stat('natural explorer:count', 1, level=10)] +
                                   [select('Class Feature', 'Favored Terrain', supports=TERRAIN_TAG, level=l) for l in ne_levels],
                             sheet_=sheet(text='Favored terrains: {{natural explorer:count}}.')))
    els += favored_options()
    els.append(class_feature(CLS, 'Fighting Style', html=H('Fighting Style'), sheet_={'display': False},
                             rules=[select('Class Feature', 'Fighting Style', supports='Fighting Style,Ranger', level=2)]))
    els.append(class_feature(CLS, 'Spellcasting', html=H('Spellcasting', True),
                             rules=[grant('ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF', 'Grants', requirements='ID_INTERNAL_GRANT_MULTICLASS')] +
                                   slot_rules(CLS, HALF_SLOTS) +
                                   count_selects('Spell', 'Spells Known (Ranger)', KNOWN, '$(spellcasting:list),$(spellcasting:slots)', 'Ranger'),
                             spellcasting={'name': 'Ranger', 'ability': 'Wisdom', 'allow_replace': True, 'list': {'supports': 'Ranger'}},
                             sheet_=sheet(text='Wisdom is your spellcasting ability for your ranger spells.')))
    els.append(class_feature(CLS, 'Ranger Archetype', html=H('Ranger Archetype'), sheet_={'display': False},
                             rules=[select('Archetype', 'Ranger Archetype', supports=TAG, level=3)]))
    els.append(class_feature(CLS, 'Primeval Awareness', html=H('Primeval Awareness'), sheet_=sheet(action='Action', usage='1 spell slot')))
    els.append(asi_feature(CLS, ASI_LEVELS, H('Ability Score Improvement'), id_=ASI_ID))
    els.append(class_feature(CLS, 'Extra Attack', html=H('Extra Attack'),
                             rules=[stat('extra attack:count', 2, level=5, bonus='extra attack')]))
    els.append(class_feature(CLS, 'Land’s Stride', html=H('Land’s Stride')))
    els.append(class_feature(CLS, 'Hide in Plain Sight', html=H('Hide in Plain Sight')))
    els.append(class_feature(CLS, 'Vanish', html=H('Vanish')))
    els.append(class_feature(CLS, 'Feral Senses', html=H('Feral Senses')))
    els.append(class_feature(CLS, 'Foe Slayer', html=H('Foe Slayer')))

    # ---- Hunter
    ar = find_in(R, 'Hunter', 21)
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    groups = [(3, "Hunter’s Prey", 'Hunter Prey'), (7, 'Defensive Tactics', 'Hunter Defensive Tactics'),
              (11, 'Multiattack', 'Hunter Multiattack'), (15, "Superior Hunter’s Defense", 'Hunter Superior Defense')]
    cg = [(lvl, af(name)) for lvl, name, _ in groups]
    els.append(archetype_element(CLS, 'Hunter', TAG, feature_html(ar), cg))
    for lvl, name, tag in groups:
        intro, opts = runin_split(find_in(R, name, 18, ar))
        els.append(class_feature(CLS, name, html=intro, ty='Archetype Feature', sheet_={'display': False},
                                 rules=[select('Archetype Feature', name, supports=tag, level=lvl)]))
        for label, h in opts:
            oid = make_id('ID_WOTC_PHB_ARCHETYPE_FEATURE_RANGER_HUNTER', label)
            rules = []
            els.append(class_feature(CLS, label, html=h, ty='Archetype Feature', id_=oid, supports=[tag], rules=rules or None))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/ranger.json', els, name='SRD 5.1 Ranger'))
