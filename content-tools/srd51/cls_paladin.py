"""Paladin and Oath of Devotion (SRD 5.1)."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cls_dhelp import *

CLS = 'Paladin'
TAG = 'Sacred Oath'

def build():
    R = class_range(CLS)
    els = []
    mc_id = 'ID_WOTC_PHB_MULTICLASS_PALADIN'
    H = lambda t, sub=False: feat2(R, t, 21, include_sub=sub)
    F = lambda n: cid(CLS, 'Class Feature', n)
    ASI_ID = 'ID_WOTC_PHB_CLASS_FEATURE_PALADIN_ASI'
    grants = [(1, F('Divine Sense')), (1, F('Lay on Hands')), (2, F('Fighting Style')), (2, F('Spellcasting')),
              (2, F('Divine Smite')), (3, F('Divine Health')), (3, F('Sacred Oath')), (4, ASI_ID),
              (5, F('Extra Attack')), (6, F('Aura of Protection')), (10, F('Aura of Courage')),
              (11, F('Improved Divine Smite')), (14, F('Cleansing Touch'))]
    na = {1: ['Divine Sense', 'Lay on Hands'], 2: ['Fighting Style', 'Spellcasting', 'Divine Smite'],
          3: ['Divine Health', 'Sacred Oath'], 4: ['Ability Score Improvement'], 5: ['Extra Attack'],
          6: ['Aura of Protection'], 7: ['Sacred Oath feature'], 8: ['Ability Score Improvement'],
          10: ['Aura of Courage'], 11: ['Improved Divine Smite'], 12: ['Ability Score Improvement'],
          14: ['Cleansing Touch'], 15: ['Sacred Oath feature'], 16: ['Ability Score Improvement'],
          18: ['Aura improvements'], 19: ['Ability Score Improvement'], 20: ['Sacred Oath feature']}
    cols = [(f'{ordinal(n)}', {l: HALF_SLOTS[l][n - 1] if l in HALF_SLOTS and len(HALF_SLOTS[l]) >= n else '—' for l in range(1, 21)})
            for n in range(1, 6)]
    table = class_table(CLS, na, cols)
    mc = multiclass_node(mc_id, '([str:13],[cha:13])', 'Strength 13 and Charisma 13',
                         'Light armor, medium armor, shields, simple weapons, martial weapons',
                         [grant(ARMOR['light'], 'Proficiency'), grant(ARMOR['medium'], 'Proficiency'), grant(ARMOR['shields'], 'Proficiency'),
                          grant(WEAPON['simple'], 'Proficiency'), grant(WEAPON['martial'], 'Proficiency')])
    els.append(class_element(
        CLS, cid(CLS, 'Class', CLS), 'd10', 'A holy warrior bound to a sacred oath.',
        [grant(ARMOR[k], 'Proficiency') for k in ('light', 'medium', 'heavy', 'shields')] +
        [grant(WEAPON[k], 'Proficiency') for k in ('simple', 'martial')] +
        [grant(save('wisdom'), 'Proficiency'), grant(save('charisma'), 'Proficiency')],
        mc, grants, 2, 'Paladin', class_desc2(CLS, table)))

    els.append(class_feature(CLS, 'Divine Sense', html=H('Divine Sense'),
                             rules=[stat('divine sense:count', 1), stat('divine sense:count', 'charisma:modifier')],
                             sheet_=sheet(action='Action', usage='{{divine sense:count}}/Long Rest')))
    els.append(class_feature(CLS, 'Lay on Hands', html=H('Lay on Hands'),
                             rules=[stat('lay on hands:hp pool', 'level:paladin') for _ in range(5)],
                             sheet_=sheet(action='Action', usage='{{lay on hands:hp pool}} hp pool', text='Restore hit points from your pool, or spend 5 points to cure a disease or neutralize a poison.')))
    els.append(class_feature(CLS, 'Fighting Style', html=H('Fighting Style'), sheet_={'display': False},
                             rules=[select('Class Feature', 'Fighting Style', supports='Fighting Style,Paladin', level=2)]))
    els.append(class_feature(CLS, 'Spellcasting', html=H('Spellcasting', True),
                             rules=[grant('ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF', 'Grants', requirements='ID_INTERNAL_GRANT_MULTICLASS'),
                                    grant('ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_HOLY_SYMBOL', 'Proficiency')] +
                                   slot_rules(CLS, HALF_SLOTS) +
                                   [stat('paladin:spellcasting:prepare', 'charisma:modifier'),
                                    stat('paladin:spellcasting:prepare', 'level:paladin:half')],
                             spellcasting={'name': 'Paladin', 'ability': 'Charisma', 'prepare': True, 'list': {'supports': 'Paladin', 'known': True}},
                             sheet_=sheet(text='You prepare {{paladin:spellcasting:prepare}} paladin spells; Charisma is your spellcasting ability. You use a holy symbol as a spellcasting focus.')))
    els.append(class_feature(CLS, 'Divine Smite', html=H('Divine Smite')))
    els.append(class_feature(CLS, 'Divine Health', html=H('Divine Health')))
    els.append(class_feature(CLS, 'Sacred Oath', html=H('Sacred Oath', True), sheet_={'display': False},
                             rules=[select('Archetype', 'Sacred Oath', supports=TAG, level=3),
                                    stat('channel divinity:count', 1, level=3, bonus='base')]))
    els.append(asi_feature(CLS, ASI_LEVELS, H('Ability Score Improvement'), id_=ASI_ID))
    els.append(class_feature(CLS, 'Extra Attack', html=H('Extra Attack'),
                             rules=[stat('extra attack:count', 2, level=5, bonus='extra attack')]))
    els.append(class_feature(CLS, 'Aura of Protection', html=H('Aura of Protection'),
                             rules=[stat('aura of protection:range', 10), stat('aura of protection:range', 20, level=18),
                                    stat('aura of protection:save bonus', 1, bonus='base'),
                                    stat('aura of protection:save bonus', 'charisma:modifier', bonus='base')] +
                                   [stat(f'{a}:save:misc', 'aura of protection:save bonus', alt='Aura of Protection') for a in ABIL],
                             sheet_=sheet(text='You and friendly creatures within {{aura of protection:range}} feet gain +{{aura of protection:save bonus}} to saving throws while you are conscious.')))
    els.append(class_feature(CLS, 'Aura of Courage', html=H('Aura of Courage'),
                             rules=[stat('aura of courage:range', 10), stat('aura of courage:range', 20, level=18)],
                             sheet_=sheet(text='You and friendly creatures within {{aura of courage:range}} feet can’t be frightened while you are conscious.')))
    els.append(class_feature(CLS, 'Improved Divine Smite', html=H('Improved Divine Smite')))
    els.append(class_feature(CLS, 'Cleansing Touch', html=H('Cleansing Touch'),
                             rules=[stat('cleansing touch:count', 1, bonus='base'), stat('cleansing touch:count', 'charisma:modifier', bonus='base')],
                             sheet_=sheet(action='Action', usage='{{cleansing touch:count}}/Long Rest')))

    # ---- Oath of Devotion
    ar = find_in(R, 'Oath of Devotion', 21)
    ARC = lambda t: feat2(R, t, 18, start=find_in(R, t, 18, ar))
    af = lambda n: cid(CLS, 'Archetype Feature', n)
    cd_i = find_in(R, 'Channel Divinity', 18, ar)
    intro, opts = runin_split(cd_i)
    od = {l: h for l, h in opts}
    cd_id = af('Channel Divinity')
    sw_id = 'ID_WOTC_PHB_ARCHETYPE_FEATURE_PALADIN_CHANNEL_DIVINITY_SACRED_WEAPON'
    tu_id = 'ID_WOTC_PHB_ARCHETYPE_FEATURE_PALADIN_CHANNEL_DIVINITY_TURN_THE_UNHOLY'
    cg = [(3, af('Tenets of Devotion')), (3, af('Oath Spells')), (3, cd_id), (7, af('Aura of Devotion')),
          (15, af('Purity of Spirit')), (20, af('Holy Nimbus'))]
    els.append(archetype_element(CLS, 'Oath of Devotion', TAG, feature_html(ar), cg))
    els.append(class_feature(CLS, 'Tenets of Devotion', html=ARC('Tenets of Devotion'), ty='Archetype Feature', sheet_={'display': False}))
    spells = [(3, 'Protection from Evil and Good'), (3, 'Sanctuary'), (5, 'Lesser Restoration'), (5, 'Zone of Truth'),
              (9, 'Beacon of Hope'), (9, 'Dispel Magic'), (13, 'Freedom of Movement'), (13, 'Guardian of Faith'),
              (17, 'Commune'), (17, 'Flame Strike')]
    els.append(class_feature(CLS, 'Oath Spells', html=feature_html(find_in(R, 'Oath Spells', 18, ar)), ty='Archetype Feature',
                             rules=[grant(spell_id(n), 'Spell', level=l, spellcasting='Paladin', prepared=True) for l, n in spells],
                             sheet_={'display': False}))
    els.append(class_feature(CLS, 'Channel Divinity', html=intro, ty='Archetype Feature', id_=cd_id,
                             rules=[grant(sw_id, 'Archetype Feature'), grant(tu_id, 'Archetype Feature')]))
    els.append(class_feature(CLS, 'Channel Divinity: Sacred Weapon', html=od['Sacred Weapon'], ty='Archetype Feature', id_=sw_id,
                             sheet_=sheet(action='Action', usage='Channel Divinity')))
    els.append(class_feature(CLS, 'Channel Divinity: Turn the Unholy', html=od['Turn the Unholy'], ty='Archetype Feature', id_=tu_id,
                             sheet_=sheet(action='Action', usage='Channel Divinity')))
    els.append(class_feature(CLS, 'Aura of Devotion', html=ARC('Aura of Devotion'), ty='Archetype Feature',
                             rules=[stat('aura of devotion:range', 10), stat('aura of devotion:range', 20, level=18)],
                             sheet_=sheet(text='You and friendly creatures within {{aura of devotion:range}} feet can’t be charmed while you are conscious.')))
    els.append(class_feature(CLS, 'Purity of Spirit', html=ARC('Purity of Spirit'), ty='Archetype Feature'))
    els.append(class_feature(CLS, 'Holy Nimbus', html=ARC('Holy Nimbus'), ty='Archetype Feature',
                             sheet_=sheet(action='Action', usage='1/Long Rest')))
    return els

if __name__ == '__main__':
    els = build()
    print(len(els), write_content('classes/paladin.json', els, name='SRD 5.1 Paladin'))
