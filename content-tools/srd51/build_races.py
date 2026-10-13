"""Races, subraces and racial traits (SRD 5.1)."""
import re, sys
sys.path.insert(0, '.')
from lib import *

ev = events()
SPELL_DAMAGE = {'Acid': 'ACID', 'Cold': 'COLD', 'Fire': 'FIRE', 'Lightning': 'LIGHTNING', 'Poison': 'POISON'}

def res(dt):
    return grant(f'ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_{dt.upper()}', 'Condition')

def race_range(name, size=27):
    a = find_head(name, size)
    # next size-27 heading or class heading
    b = a + 1
    while b < len(ev) and not (ev[b]['k'] == 'H' and ev[b]['size'] in (27, 39)):
        b += 1
    return a, b

def traits_in(a, b):
    """-> (intro_html, ordered dict label -> html) for run-in labelled paragraphs between event idx a and b.
    Sub-race sections are separated out by size-18 headings: returns dict sections: name -> (intro, traits)"""
    sections = {}
    cur = ('_race', [], [])  # name, intro paragraphs, trait list
    cur_name = '_race'
    intro, traits = [], []
    last = None
    def close():
        sections[cur_name] = (to_html(intro), traits[:])
    for e in ev[a:b]:
        if e['k'] == 'H':
            if e['size'] == 18:
                close(); cur_name = clean(e['text']); intro, traits, last = [], [], None
            continue
        if e['k'] not in 'PB':
            continue
        m = re.match(r'^\s*<i><b>\s*(.*?)\s*</b></i>(.*)$', e['html'], flags=re.S)
        if m and e['k'] == 'P':
            label = clean(re.sub(r'<[^>]+>', '', m.group(1))).rstrip('.')
            e2 = dict(e); e2['html'] = m.group(2).strip()
            traits.append([label, [e2]]); last = traits[-1]
        elif last is not None:
            last[1].append(e)
        else:
            intro.append(e)
    close()
    out = {}
    for k, (i, t) in sections.items():
        out[k] = (i, {lab: to_html(evs) for lab, evs in t}, {lab: text_of(evs) for lab, evs in t})
    return out

def tr(id_, name, text_html, text_plain, rules=None, sheet_text=True, supports=None, display=None, spellcasting=None, extra_desc=''):
    sh = {}
    if display is not None: sh['display'] = display
    if sheet_text and text_plain: sh['descriptions'] = [{'text': text_plain}]
    return element(id_, name, 'Racial Trait', text_html + extra_desc, supports=supports, rules=rules, sheet=sh or None,
                   spellcasting=spellcasting)

def build():
    els = []
    ASI_ABIL = ['Strength', 'Dexterity', 'Constitution', 'Intelligence', 'Wisdom', 'Charisma']

    def race(id_, name, hdr, data, rules, intro_extra=''):
        intro, traits_html, traits_plain = data[hdr]
        desc = ''.join(f'<p><strong><em>{k}.</em></strong> {v[3:] if v.startswith("<p>") else v}' if not v.startswith('<p>') else f'<p><strong><em>{k}.</em></strong> {v[3:]}' for k, v in traits_html.items())
        return element(id_, name, 'Race', intro + desc + intro_extra, rules=rules, sheet={'display': False})

    def subrace(id_, name, parent, data, key, rules):
        intro, traits_html, _ = data[key]
        desc = intro + ''.join(f'<p><strong><em>{k}.</em></strong> {v[3:] if v.startswith("<p>") else v}' for k, v in traits_html.items())
        return element(id_, name, 'Sub Race', desc, supports=[parent], rules=rules, sheet={'display': False})

    def T(data, key, label, id_, name=None, **kw):
        _, th, tp = data[key]
        return tr(id_, name or label, th[label], tp[label], **kw)

    # ------------------------------------------------------------ Dwarf
    a, b = race_range('Dwarf'); D = traits_in(a, b)
    els.append(race('ID_SRD_RACE_DWARF', 'Dwarf', '_race', D, [
        stat('constitution', 2),
        stat('innate speed', 25, bonus='base'),
        grant('ID_SIZE_MEDIUM', 'Size'), grant('ID_VISION_DARKVISION', 'Vision'),
        grant('ID_LANGUAGE_COMMON', 'Language'), grant('ID_LANGUAGE_DWARVISH', 'Language'),
        grant('ID_RACIAL_TRAIT_DWARVEN_RESILIENCE', 'Racial Trait'),
        grant('ID_RACIAL_TRAIT_DWARVEN_COMBAT_TRAINING', 'Racial Trait'),
        grant('ID_RACIAL_TRAIT_DWARVEN_TOOL_PROFICIENCY', 'Racial Trait'),
        grant('ID_RACIAL_TRAIT_STONECUNNING', 'Racial Trait'),
        grant('ID_RACIAL_TRAIT_DWARVEN_SUBRACE', 'Racial Trait'),
    ]))
    els.append(T(D, '_race', 'Dwarven Resilience', 'ID_RACIAL_TRAIT_DWARVEN_RESILIENCE', rules=[res('Poison')]))
    els.append(T(D, '_race', 'Dwarven Combat Training', 'ID_RACIAL_TRAIT_DWARVEN_COMBAT_TRAINING', rules=[
        grant('ID_PROFICIENCY_WEAPON_PROFICIENCY_' + w, 'Proficiency') for w in ('BATTLEAXE', 'HANDAXE', 'LIGHT_HAMMER', 'WARHAMMER')]))
    els.append(T(D, '_race', 'Tool Proficiency', 'ID_RACIAL_TRAIT_DWARVEN_TOOL_PROFICIENCY', rules=[
        select('Proficiency', 'Dwarven Tool Proficiency', supports='ID_PROFICIENCY_TOOL_PROFICIENCY_SMITHS_TOOLS|ID_PROFICIENCY_TOOL_PROFICIENCY_BREWERS_SUPPLIES|ID_PROFICIENCY_TOOL_PROFICIENCY_MASONS_TOOLS')]))
    els.append(T(D, '_race', 'Stonecunning', 'ID_RACIAL_TRAIT_STONECUNNING'))
    els.append(element('ID_RACIAL_TRAIT_DWARVEN_SUBRACE', 'Dwarven Subrace', 'Racial Trait',
                       '<p>Choose a dwarf subrace. The SRD describes the hill dwarf.</p>',
                       rules=[select('Sub Race', 'Dwarven Subrace', supports='Dwarf')], sheet={'display': False}))
    els.append(subrace('ID_SUB_RACE_HILL_DWARF', 'Hill Dwarf', 'Dwarf', D, 'Hill Dwarf', [
        stat('wisdom', 1), grant('ID_RACIAL_TRAIT_DWARVEN_TOUGHNESS', 'Racial Trait')]))
    els.append(T(D, 'Hill Dwarf', 'Dwarven Toughness', 'ID_RACIAL_TRAIT_DWARVEN_TOUGHNESS', rules=[stat('hp', 'level')]))

    # ------------------------------------------------------------ Elf
    a, b = race_range('Elf'); D = traits_in(a, b)
    els.append(race('ID_RACE_ELF', 'Elf', '_race', D, [
        stat('dexterity', 2), stat('innate speed', 30, bonus='base'),
        grant('ID_SIZE_MEDIUM', 'Size'), grant('ID_VISION_DARKVISION', 'Vision'),
        grant('ID_LANGUAGE_COMMON', 'Language'), grant('ID_LANGUAGE_ELVISH', 'Language'),
        grant('ID_RACIAL_TRAIT_KEEN_SENSES', 'Racial Trait'), grant('ID_RACIAL_TRAIT_FEY_ANCESTRY', 'Racial Trait'),
        grant('ID_RACIAL_TRAIT_TRANCE', 'Racial Trait'), grant('ID_RACIAL_TRAIT_ELVEN_SUBRACE', 'Racial Trait'),
    ]))
    els.append(T(D, '_race', 'Keen Senses', 'ID_RACIAL_TRAIT_KEEN_SENSES', rules=[grant('ID_PROFICIENCY_SKILL_PERCEPTION', 'Proficiency')]))
    els.append(T(D, '_race', 'Fey Ancestry', 'ID_RACIAL_TRAIT_FEY_ANCESTRY'))
    els.append(T(D, '_race', 'Trance', 'ID_RACIAL_TRAIT_TRANCE'))
    els.append(element('ID_RACIAL_TRAIT_ELVEN_SUBRACE', 'Elven Subrace', 'Racial Trait',
                       '<p>Choose an elf subrace. The SRD describes the high elf.</p>',
                       rules=[select('Sub Race', 'Elven Subrace', supports='Elf')], sheet={'display': False}))
    els.append(subrace('ID_SUB_RACE_HIGH_ELF', 'High Elf', 'Elf', D, 'High Elf', [
        stat('intelligence', 1), grant('ID_RACIAL_TRAIT_ELF_WEAPON_TRAINING', 'Racial Trait'),
        grant('ID_RACIAL_TRAIT_CANTRIP', 'Racial Trait'), grant('ID_RACIAL_TRAIT_EXTRA_LANGUAGE', 'Racial Trait')]))
    els.append(T(D, 'High Elf', 'Elf Weapon Training', 'ID_RACIAL_TRAIT_ELF_WEAPON_TRAINING', rules=[
        grant('ID_PROFICIENCY_WEAPON_PROFICIENCY_' + w, 'Proficiency') for w in ('LONGSWORD', 'SHORTSWORD', 'SHORTBOW', 'LONGBOW')]))
    els.append(tr('ID_RACIAL_TRAIT_CANTRIP', 'Cantrip', '<p>You know one cantrip of your choice from the wizard spell list. Intelligence is your spellcasting ability for it.</p>',
                  'You know one cantrip of your choice from the wizard spell list. Intelligence is your spellcasting ability for it.',
                  rules=[select('Spell', 'Wizard Cantrip (High Elf)', supports='Wizard,0', spellcasting='High Elf')],
                  spellcasting={'name': 'High Elf', 'ability': 'Intelligence'}))
    els.append(tr('ID_RACIAL_TRAIT_EXTRA_LANGUAGE', 'Extra Language', D['High Elf'][1]['Extra Language'], D['High Elf'][2]['Extra Language'],
                  rules=[select('Language', 'Language (Elf)', supports='Starting')], display=False))

    # ------------------------------------------------------------ Halfling
    a, b = race_range('Halfling'); D = traits_in(a, b)
    els.append(race('ID_RACE_HALFLING', 'Halfling', '_race', D, [
        stat('dexterity', 2), stat('innate speed', 25, bonus='base'),
        grant('ID_SIZE_SMALL', 'Size'),
        grant('ID_LANGUAGE_COMMON', 'Language'), grant('ID_LANGUAGE_HALFLING', 'Language'),
        grant('ID_RACIAL_TRAIT_LUCKY', 'Racial Trait'), grant('ID_RACIAL_TRAIT_BRAVE', 'Racial Trait'),
        grant('ID_RACIAL_TRAIT_HALFLING_NIMBLENESS', 'Racial Trait'), grant('ID_RACIAL_TRAIT_HALFLING_SUBRACE', 'Racial Trait'),
    ]))
    els.append(T(D, '_race', 'Lucky', 'ID_RACIAL_TRAIT_LUCKY'))
    els.append(T(D, '_race', 'Brave', 'ID_RACIAL_TRAIT_BRAVE'))
    els.append(T(D, '_race', 'Halfling Nimbleness', 'ID_RACIAL_TRAIT_HALFLING_NIMBLENESS'))
    els.append(element('ID_RACIAL_TRAIT_HALFLING_SUBRACE', 'Halfling Subrace', 'Racial Trait',
                       '<p>Choose a halfling subrace. The SRD describes the lightfoot.</p>',
                       rules=[select('Sub Race', 'Halfling Subrace', supports='Halfling')], sheet={'display': False}))
    els.append(subrace('ID_SUB_RACE_LIGHTFOOT_HALFLING', 'Lightfoot Halfling', 'Halfling', D, 'Lightfoot', [
        stat('charisma', 1), grant('ID_RACIAL_TRAIT_NATURALLY_STEALTHY', 'Racial Trait')]))
    els.append(T(D, 'Lightfoot', 'Naturally Stealthy', 'ID_RACIAL_TRAIT_NATURALLY_STEALTHY'))

    # ------------------------------------------------------------ Human
    a, b = race_range('Human'); D = traits_in(a, b)
    els.append(race('ID_RACE_HUMAN', 'Human', '_race', D, [
        stat('strength', 1), stat('dexterity', 1), stat('constitution', 1), stat('intelligence', 1),
        stat('wisdom', 1), stat('charisma', 1),
        stat('innate speed', 30, bonus='base'), grant('ID_SIZE_MEDIUM', 'Size'),
        grant('ID_LANGUAGE_COMMON', 'Language'),
        select('Language', 'Language (Human)', supports='Starting'),
    ]))

    # ------------------------------------------------------------ Dragonborn
    a, b = race_range('Dragonborn'); D = traits_in(a, b)
    anc = [('Black', 'Acid', '5 by 30 ft. line (Dex. save)'), ('Blue', 'Lightning', '5 by 30 ft. line (Dex. save)'),
           ('Brass', 'Fire', '5 by 30 ft. line (Dex. save)'), ('Bronze', 'Lightning', '5 by 30 ft. line (Dex. save)'),
           ('Copper', 'Acid', '5 by 30 ft. line (Dex. save)'), ('Gold', 'Fire', '15 ft. cone (Dex. save)'),
           ('Green', 'Poison', '15 ft. cone (Dex. save)'), ('Red', 'Fire', '15 ft. cone (Dex. save)'),
           ('Silver', 'Cold', '15 ft. cone (Dex. save)'), ('White', 'Cold', '15 ft. cone (Dex. save)')]
    els.append(race('ID_RACE_DRAGONBORN', 'Dragonborn', '_race', D, [
        stat('strength', 2), stat('charisma', 1), stat('innate speed', 30, bonus='base'),
        grant('ID_SIZE_MEDIUM', 'Size'),
        grant('ID_LANGUAGE_COMMON', 'Language'), grant('ID_LANGUAGE_DRACONIC', 'Language'),
        grant('ID_RACIAL_TRAIT_DRACONIC_ANCESTRY', 'Racial Trait'),
        grant('ID_RACIAL_TRAIT_BREATH_WEAPON', 'Racial Trait'),
        grant('ID_RACIAL_TRAIT_DAMAGE_RESISTANCE', 'Racial Trait'),
    ]))
    els.append(element('ID_RACIAL_TRAIT_DRACONIC_ANCESTRY', 'Draconic Ancestry', 'Racial Trait',
                       D['_race'][1]['Draconic Ancestry'], rules=[select('Racial Trait', 'Draconic Ancestry', supports='Draconic Ancestry')],
                       sheet={'display': False}))
    for col, dt, br in anc:
        els.append(element(f'ID_RACIAL_TRAIT_DRACONIC_ANCESTRY_{col.upper()}', f'Draconic Ancestry ({col})', 'Racial Trait',
                           f'<p>{col} dragon ancestry: {dt} damage; breath weapon is a {br.split(" (")[0]}.</p>',
                           supports=['Draconic Ancestry'],
                           rules=[stat('draconic-ancestry', col, inline=True), stat('draconic-ancestry:damage type', dt, inline=True),
                                  stat('draconic-ancestry:breath', br, inline=True)],
                           sheet={'display': False}))
    els.append(tr('ID_RACIAL_TRAIT_BREATH_WEAPON', 'Breath Weapon', D['_race'][1]['Breath Weapon'], D['_race'][2]['Breath Weapon'], rules=[
        stat('breath-weapon:dc', 8), stat('breath-weapon:dc', 'constitution:modifier'), stat('breath-weapon:dc', 'proficiency'),
        stat('breath-weapon:dice size', 6, bonus='dice'),
        stat('breath-weapon:dice count', 2, level=1), stat('breath-weapon:dice count', 1, level=6),
        stat('breath-weapon:dice count', 1, level=11), stat('breath-weapon:dice count', 1, level=16)]))
    rr = []
    groups = {'Acid': ['BLACK', 'COPPER'], 'Lightning': ['BLUE', 'BRONZE'], 'Fire': ['BRASS', 'GOLD', 'RED'],
              'Poison': ['GREEN'], 'Cold': ['SILVER', 'WHITE']}
    for dt, cols in groups.items():
        rr.append(grant(f'ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_{dt.upper()}', 'Condition',
                        requirements='||'.join(f'ID_RACIAL_TRAIT_DRACONIC_ANCESTRY_{c}' for c in cols)))
    els.append(tr('ID_RACIAL_TRAIT_DAMAGE_RESISTANCE', 'Damage Resistance', D['_race'][1]['Damage Resistance'], D['_race'][2]['Damage Resistance'], rules=rr))

    # ------------------------------------------------------------ Gnome
    a, b = race_range('Gnome'); D = traits_in(a, b)
    els.append(race('ID_RACE_GNOME', 'Gnome', '_race', D, [
        stat('intelligence', 2), stat('innate speed', 25, bonus='base'),
        grant('ID_SIZE_SMALL', 'Size'), grant('ID_VISION_DARKVISION', 'Vision'),
        grant('ID_LANGUAGE_COMMON', 'Language'), grant('ID_LANGUAGE_GNOMISH', 'Language'),
        grant('ID_RACIAL_TRAIT_GNOME_CUNNING', 'Racial Trait'), grant('ID_RACIAL_TRAIT_GNOME_SUBRACE', 'Racial Trait'),
    ]))
    els.append(T(D, '_race', 'Gnome Cunning', 'ID_RACIAL_TRAIT_GNOME_CUNNING'))
    els.append(element('ID_RACIAL_TRAIT_GNOME_SUBRACE', 'Gnome Subrace', 'Racial Trait',
                       '<p>Choose a gnome subrace. The SRD describes the rock gnome.</p>',
                       rules=[select('Sub Race', 'Gnome Subrace', supports='Gnome')], sheet={'display': False}))
    els.append(subrace('ID_SUB_RACE_ROCK_GNOME', 'Rock Gnome', 'Gnome', D, 'Rock Gnome', [
        stat('constitution', 1), grant('ID_RACIAL_TRAIT_ARTIFICERS_LORE', 'Racial Trait'), grant('ID_RACIAL_TRAIT_TINKER', 'Racial Trait')]))
    els.append(T(D, 'Rock Gnome', 'Artificer’s Lore', 'ID_RACIAL_TRAIT_ARTIFICERS_LORE'))
    tink = T(D, 'Rock Gnome', 'Tinker', 'ID_RACIAL_TRAIT_TINKER', rules=[grant('ID_PROFICIENCY_TOOL_PROFICIENCY_TINKERS_TOOLS', 'Proficiency')])
    # include the clockwork toy / fire starter / music box options that follow the Tinker paragraph
    els.append(tink)

    # ------------------------------------------------------------ Half-Elf
    a, b = race_range('Half-Elf'); D = traits_in(a, b)
    els.append(race('ID_RACE_HALFELF', 'Half-Elf', '_race', D, [
        stat('charisma', 2),
        select('Racial Trait', 'Ability Score Increase (Half-Elf)', supports='Half-Elf', number=2),
        stat('innate speed', 30, bonus='base'), grant('ID_SIZE_MEDIUM', 'Size'), grant('ID_VISION_DARKVISION', 'Vision'),
        grant('ID_LANGUAGE_COMMON', 'Language'), grant('ID_LANGUAGE_ELVISH', 'Language'),
        select('Language', 'Language (Half-Elf)', supports='Starting'),
        grant('ID_RACIAL_TRAIT_HALFELF_FEY_ANCESTRY', 'Racial Trait'),
        grant('ID_RACIAL_TRAIT_SKILL_VERSATILITY', 'Racial Trait'),
    ]))
    for ab in ('Strength', 'Dexterity', 'Constitution', 'Intelligence', 'Wisdom'):
        els.append(element(f'ID_RACIAL_TRAIT_HALFELF_{ab.upper()}', f'Ability Score Increase ({ab})', 'Racial Trait',
                           f'<p>Your {ab} score increases by 1.</p>', supports=['Half-Elf'], rules=[stat(ab.lower(), 1)],
                           sheet={'display': False}))
    els.append(T(D, '_race', 'Fey Ancestry', 'ID_RACIAL_TRAIT_HALFELF_FEY_ANCESTRY'))
    els.append(T(D, '_race', 'Skill Versatility', 'ID_RACIAL_TRAIT_SKILL_VERSATILITY', rules=[
        select('Proficiency', 'Skill Proficiency (Half-Elf)', supports='Skill', number=2)]))

    # ------------------------------------------------------------ Half-Orc
    a, b = race_range('Half-Orc'); D = traits_in(a, b)
    els.append(race('ID_RACE_HALFORC', 'Half-Orc', '_race', D, [
        stat('strength', 2), stat('constitution', 1), stat('innate speed', 30, bonus='base'),
        grant('ID_SIZE_MEDIUM', 'Size'), grant('ID_VISION_DARKVISION', 'Vision'),
        grant('ID_LANGUAGE_COMMON', 'Language'), grant('ID_LANGUAGE_ORC', 'Language'),
        grant('ID_RACIAL_TRAIT_MENACING', 'Racial Trait'), grant('ID_RACIAL_TRAIT_RELENTLESS_ENDURANCE', 'Racial Trait'),
        grant('ID_RACIAL_TRAIT_SAVAGE_ATTACKS', 'Racial Trait'),
    ]))
    els.append(T(D, '_race', 'Menacing', 'ID_RACIAL_TRAIT_MENACING', rules=[grant('ID_PROFICIENCY_SKILL_INTIMIDATION', 'Proficiency')]))
    els.append(T(D, '_race', 'Relentless Endurance', 'ID_RACIAL_TRAIT_RELENTLESS_ENDURANCE'))
    els.append(T(D, '_race', 'Savage Attacks', 'ID_RACIAL_TRAIT_SAVAGE_ATTACKS'))

    # ------------------------------------------------------------ Tiefling
    a, b = race_range('Tiefling'); D = traits_in(a, b)
    els.append(race('ID_RACE_TIEFLING', 'Tiefling', '_race', D, [
        stat('intelligence', 1), stat('charisma', 2), stat('innate speed', 30, bonus='base'),
        grant('ID_SIZE_MEDIUM', 'Size'), grant('ID_VISION_DARKVISION', 'Vision'),
        grant('ID_LANGUAGE_COMMON', 'Language'), grant('ID_LANGUAGE_INFERNAL', 'Language'),
        grant('ID_RACIAL_TRAIT_HELLISH_RESISTANCE', 'Racial Trait'), grant('ID_RACIAL_TRAIT_INFERNAL_LEGACY', 'Racial Trait'),
    ]))
    els.append(T(D, '_race', 'Hellish Resistance', 'ID_RACIAL_TRAIT_HELLISH_RESISTANCE', rules=[res('Fire')]))
    els.append(T(D, '_race', 'Infernal Legacy', 'ID_RACIAL_TRAIT_INFERNAL_LEGACY', rules=[
        grant('ID_PHB_SPELL_THAUMATURGY', 'Spell', spellcasting='Tiefling'),
        grant('ID_PHB_SPELL_HELLISH_REBUKE', 'Spell', level=3, spellcasting='Tiefling'),
        grant('ID_PHB_SPELL_DARKNESS', 'Spell', level=5, spellcasting='Tiefling')],
        spellcasting={'name': 'Tiefling', 'ability': 'Charisma'}))
    return els

if __name__ == '__main__':
    els = build()
    p = write_content('races.json', els, name='SRD 5.1 Races', description='Races, subraces and racial traits from the SRD 5.1.')
    print(len(els), 'elements ->', p)
