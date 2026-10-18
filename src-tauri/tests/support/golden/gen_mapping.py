"""Golden spec -> Borealis build steps (acceptance 3e, see golden_characters.rs).

Writes the step list of every golden: each spec choice becomes a step that names the Borealis
option (spec name, or the option's own label such as `Skill Expertise (Stealth)`) and the select it
goes in. Ids are not written here: the Rust resolve run looks every name up among the options the
select offers, from the golden's own SRD pack, and writes the ids into mapping.json:

  python3 -I gen_mapping.py /path/steps.json
  GOLDEN_MAPPING=/path/steps.json GOLDEN_RESOLVE=../../goldens/mapping.json \
    cargo test --release -p borealis --test golden_characters

Stdlib only; reads goldens/specs and the bundled SRD spell files (spell levels, to fill the
tightest select first).
"""
import json, glob, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
G = os.path.normpath(os.path.join(HERE, '..', '..', 'goldens'))
C = os.path.normpath(os.path.join(HERE, '..', '..', '..', '..', 'content'))
ABIL = ['Strength', 'Dexterity', 'Constitution', 'Intelligence', 'Wisdom', 'Charisma']
def key(n): return n.lower().replace('’', "'")

SL = {}
for ed, pat in (('2014', C + '/srd-5.1/spells/spells-%d.json'), ('2024', C + '/srd-5.2.1/spells/level-%d.json')):
    for lv in range(10):
        for e in json.load(open(pat % lv))['elements']:
            SL[(ed, key(e['name']))] = lv

def P(name, slot=None, **kw):
    s = {'pick': name}
    if slot: s['slot'] = slot
    s.update(kw)
    return s

def asi_name(inc):
    parts = ['%s +%d' % (a, inc[a]) for a in ABIL if a in inc]
    return ', '.join(parts)

TOOL_ALIAS = {'playing cards': 'Playing card set', 'flute (musical instrument)': 'Flute'}
NAME_ALIAS = {
    '2014': {'Lightfoot': 'Lightfoot Halfling', 'Gold (fire, 15 ft. cone, Dex save)': 'Draconic Ancestry (Gold)',
             'Red (fire)': 'Dragon Ancestor (Red)',
             'Beasts (no language gained: beasts speak none)': 'Beasts'},
    '2024': {'Gold (Fire)': 'Gold Dragon', 'Stone\'s Endurance (Stone Giant)': 'Stone’s Endurance (Stone Giant)',
             'Agonizing Blast (Eldritch Blast)': 'Agonizing Blast', 'Lessons of the First Ones (Origin feat: Alert)': 'Lessons of the First Ones',
             'Prestidigitation (not replaced)': 'Prestidigitation', 'Infernal': 'Infernal'},
}
def alias(ed, n): return NAME_ALIAS[ed].get(n, n)
def tool(t): return 'Tool Proficiency (%s)' % TOOL_ALIAS.get(t.lower(), t)
SUBCLASS_FEATURES = {'Primal Path', 'Bard College', 'Divine Domain', 'Druid Circle', 'Martial Archetype', 'Monastic Tradition',
                     'Sacred Oath', 'Ranger Archetype', 'Roguish Archetype', 'Sorcerous Origin', 'Otherworldly Patron', 'Arcane Tradition'}
FEATURE_SLOT = {'Bonus Cantrip': 'Druid Cantrip', 'Circle Spells': 'Circle Land', 'Dragon Ancestor': 'Draconic Ancestor',
                'Eldritch Invocations': 'Eldritch Invocation', 'Hunter\'s Prey': 'Hunter', 'Metamagic': 'Metamagic',
                'Pact Boon': 'Pact Boon', 'Divine Order': 'Divine Order', 'Primal Order': 'Primal Order',
                'Defensive Tactics': 'Defensive Tactics', 'Weapon Mastery': 'Weapon Mastery', 'Fighting Style': 'Fighting Style',
                'Additional Fighting Style': 'Additional Fighting Style', 'Epic Boon': 'Epic Boon',
                'Favored Enemy': 'Favored Enemy', 'Natural Explorer': 'Favored Terrain'}
EXTRA_CANTRIP_SLOT = {'Primal Order': 'Magician Extra Cantrip', 'Divine Order': 'Thaumaturge'}
NOT_SELECTABLE = {  # spec choices Borealis has no select for, recorded as notes ('gap': builder gap)
    'Elemental Affinity': 'auto'}

def choice_steps(ed, cls, clv, c, spell_extra):
    f = c['feature']; out = []
    if f in NOT_SELECTABLE:
        return [{'note': '%s: %s (%s)' % (f, c.get('choice'), NOT_SELECTABLE[f]), 'gap': NOT_SELECTABLE[f] == 'gap'}]
    if f in ('Proficiencies', 'Bonus Proficiencies', 'Primal Knowledge'):
        sl = {'Proficiencies': 'Skill Proficiency (%s)' % cls, 'Bonus Proficiencies': 'College of Lore',
              'Primal Knowledge': 'Primal Knowledge'}[f]
        for s in c.get('skills', []):
            out.append(P(s, sl))
        for t in c.get('tools', []) + c.get('musical_instruments', []) + ([c['tool']] if 'tool' in c else []):
            out.append(P(tool(t), 'Proficiency (%s)' % cls))
        if 'note' in c: out.append({'note': c['note']})
        return out
    if f == 'Expertise' or 'expertise' in c:
        ex = c.get('skills', []) if f == 'Expertise' else ([c['expertise']] if isinstance(c['expertise'], str) else c['expertise'])
        for s in ex:
            out.append(P('Skill Expertise (%s)' % s, f if f == 'Scholar' else 'Expertise'))
        for l in c.get('languages', []):
            out.append(P(l, 'Language'))
        return out
    if f in ('Ability Score Improvement', 'Epic Boon'):
        feat = c.get('feat_instead') or c.get('feat')
        if ed == '2014':
            opt = 'Improvement Option (%s %d)' % (cls, clv)
            if feat:
                out.append(P('Feat (%d)' % clv, opt)); out.append(P(feat, 'Feat'))
            else:
                out.append(P('Ability Score Improvement (%d)' % clv, opt))
                for a, n in c['increases'].items():
                    for _ in range(n): out.append(P(a, 'Ability Score Increase (%s %d)' % (cls, clv)))
        else:
            out.append(P(feat, '%s (%s %d)' % (f, cls, clv) if f != 'Epic Boon' else 'Epic Boon (%s)' % cls))
            if c.get('increases'):
                out.append(P(asi_name(c['increases']), 'Ability Score Increase'))
        return out
    if f == 'Weapon Mastery' and 'add' in c:
        return [P(w, 'Weapon Mastery') for w in c['add']]
    if f in ('Magical Discoveries', 'Evocation Savant'):
        for s in c['spells']:
            out.append(P(s.split(' (')[0], f, spell=True))
        spell_extra.update(s.split(' (')[0] for s in c['spells'])
        if 'note' in c: out.append({'note': c['note']})
        return out
    if f == "Thieves' Cant":
        return [P(c['extra_language'], 'Thieves')]
    if f == 'Pact Boon' and 'book_of_shadows_cantrips' in c:
        out.append(P(c['choice'], 'Pact Boon'))
        for s in c['book_of_shadows_cantrips']:
            out.append(P(s, 'Pact of the Tome', spell=True)); spell_extra.add(s)
        return out
    if 'feat' in c:
        return [P(c['feat'], f)]
    ch = c.get('choice', c.get('gain'))
    if ch is None:
        return [{'note': c.get('note', json.dumps(c))}]
    if ed == '2024' and f.endswith(' Subclass') or f in SUBCLASS_FEATURES:
        out.append(P(ch, 'archetype'))
        return out
    for x in (ch if isinstance(ch, list) else [ch]):
        name = 'Circle Spells (%s)' % x if f == 'Circle Spells' else alias(ed, x)
        out.append(P(name, FEATURE_SLOT.get(f, f)))
        if x.startswith('Lessons of the First Ones'):
            out.append(P(x.split('feat: ')[1].rstrip(')'), 'Lessons of the First Ones'))
    if 'extra_cantrip' in c:
        out.append(P(c['extra_cantrip'], EXTRA_CANTRIP_SLOT.get(f, ''), spell=True)); spell_extra.add(c['extra_cantrip'])
    return out

RACE_SLOT = {'extra language': 'Language (%s)', 'Extra Language': 'Language (%s)', 'Skill Versatility': 'Skill Proficiency (%s)',
             'Tool Proficiency': 'Tool', 'Draconic Ancestry': 'Draconic Ancestry', 'Cantrip (wizard, Intelligence)': 'High Elf',
             'Giant Ancestry': 'Giant Ancestry', 'Skillful': 'Skillful', 'Versatile (Origin feat)': 'Versatile',
             'Skilled proficiencies': 'Skilled', 'Gnomish Lineage': 'Lineage', 'Elven Lineage': 'Lineage',
             'Fiendish Legacy': 'Legacy', 'spellcasting_ability': 'Lineage Spellcasting Ability', 'Keen Senses': 'Keen Senses',
             'High Elf cantrip': 'High Elf'}

def race_steps(ed, r):
    out = [P(r['name'], 'Race')]
    if r.get('subrace'):
        out.append(P(alias(ed, r['subrace']), 'Subrace'))
    if r.get('size') and ed == '2024':
        out.append(P(r['size'], 'Size', optional=True))
    ch = r.get('choices', {})
    for k, v in ch.items():
        if k == 'Ability Score Increase (two other scores +1)':
            for a in v: out.append(P('Ability Score Increase (%s)' % a, 'Ability Score Increase'))
            continue
        if k == 'Skilled proficiencies':
            for x in v:
                out.append(P(tool(x) if x.endswith('Kit') else x, 'Skilled'))
            continue
        if k == 'Tool Proficiency':
            out.append(P(tool(v), 'Tool')); continue
        for x in (v if isinstance(v, list) else [v]):
            sl = RACE_SLOT[k] % r['name'] if '%s' in RACE_SLOT[k] else RACE_SLOT[k]
            out.append(P(alias(ed, x), sl, optional=k in ('High Elf cantrip',)))
    return out

def gen(spec):
    gid = spec['id']; ed = spec['edition']
    spell_extra = set()
    first = spec['level_progression'][0]
    l1 = race_steps(ed, spec.get('race') or spec.get('species'))
    bg = spec['background']
    l1.append(P(bg['name'], 'Background'))
    l1.append(P(first['class'], 'Class'))
    for l in bg.get('languages', []):
        l1.append(P(l, 'Language (%s)' % bg['name']))
    if bg.get('ability_score_increases'):
        l1.append(P(asi_name(bg['ability_score_increases']), 'Ability Scores (%s)' % bg['name']))
    if bg.get('tool_proficiency'):
        l1.append(P(tool(bg['tool_proficiency']), 'Tool Proficiency (%s)' % bg['name'], optional=True))
    ofc = bg.get('origin_feat_choices') or {}
    if ofc.get('spellcasting_ability'):
        l1.append(P(ofc['spellcasting_ability'], 'Spellcasting Ability'))
    if ed == '2024':
        for l in spec.get('languages', []):
            if l != 'Common':
                l1.append(P(l, 'Languages', optional=True))
    levels = [l1]
    for lv in spec['level_progression']:
        if lv['character_level'] > 1:
            levels.append([])
            if lv['class_level'] == 1:
                levels[-1].append(P(lv['class'], 'Multiclass'))
        for c in lv['choices']:
            levels[-1].extend(choice_steps(ed, lv['class'], lv['class_level'], c, spell_extra))
    post = []
    classes = {c['class'] for c in spec['classes']}
    for src, sp in (spec.get('spells') or {}).items():
        names = []
        for k in ('cantrips', 'known', 'spellbook', 'level_1'):
            names += [n for n in sp.get(k, []) if n not in spell_extra]
        for n in names:
            if (ed, key(n)) not in SL: raise SystemExit('%s: unknown spell %s' % (gid, n))
        names.sort(key=lambda n: -SL[(ed, key(n))])
        if src in classes:
            slot, opt = src, False
        elif src.startswith('Magic Initiate'):
            slot, opt = 'Magic Initiate', False
        else:
            slot, opt = '', True   # species / race trait spells: chosen above or granted
        for n in names:
            post.append(P(n, slot, spell=True, optional=opt))
        for n in sp.get('prepared', []):
            post.append({'prepare': n, 'class': src})
    post += equipment(spec)
    out = {'levels': levels, 'post': post}
    if gid in SOURCE_ALIAS:
        out['sources'] = SOURCE_ALIAS[gid]
    if gid in RESOURCE_ALIAS:
        out['resources'] = RESOURCE_ALIAS[gid]
    return out

import re
def item_name(t):
    m = re.match(r'^(.*?)\s*\((?:x(\d+))?[^)]*\)\s*$', t)
    if m:
        return m.group(1).strip(), int(m.group(2) or 1)
    return t.strip(), 1

ITEM_ALIAS = {'2014': {'leather armor': 'Leather', 'light crossbow': 'Crossbow, Light', 'chain mail': 'Chain Mail'}, '2024': {}}
RESOURCE_ALIAS = {'srd521-paladin-3': {"Paladin's Smite (free Divine Smite)": 'Divine Smite'},
                  'srd521-ranger-11': {"Favored Enemy (Hunter's Mark without a slot)": "Hunter's Mark",
                                       'Tireless: Temporary Hit Points': 'Tireless'}}
SOURCE_ALIAS = {'srd51-ranger-5': {'High Elf Cantrip': 'High Elf'}, 'srd51-sorcerer-17': {'Infernal Legacy': 'Tiefling'}}

def equipment(spec):
    e = spec['equipment']; out = []; ia = ITEM_ALIAS[spec['edition']]
    wield = (e.get('wielded') or '')
    main = wield.split('+')[0].strip()
    main_name, _ = item_name(main.replace('Pact weapon ', ''))
    two = 'two hands' in main
    if e.get('worn_armor'):
        out.append({'item': ia.get(e['worn_armor'].lower(), e['worn_armor']), 'equip': 'worn', 'spec': e['worn_armor']})
    if e.get('shield'):
        out.append({'item': 'Shield' if 'shield' in e['shield'].lower() else e['shield'], 'equip': 'secondary', 'spec': e['shield']})
    for w in e.get('carried_weapons', []):
        n, q = item_name(w)
        step = {'item': ia.get(n.lower(), n), 'qty': q, 'spec': w}
        if n.lower() == main_name.lower():
            step['equip'] = 'two-handed' if two else 'primary'
            if two and n.lower() in VERSATILE:
                step['equip'] = 'versatile'
            main_name = '\0'
        out.append(step)
    return out
VERSATILE = {'longsword', 'quarterstaff', 'spear', 'battleaxe', 'warhammer', 'trident'}

out = {'schema': 'borealis-golden-mapping/1',
       'about': 'Per golden: levels[n] = the steps applied at character level n+1, post = spells, '
                'preparation and equipment after the last level. A step picks `pick` (a Borealis option '
                'name) in the first open select whose type or title matches `slot`; `id` is the element '
                'that name resolved to in the bundled SRD pack (GOLDEN_RESOLVE), checked on every run. '
                '`sources` / `resources` map an expected-sheet name to the name Borealis shows. '
                'Generated by tests/support/golden/gen_mapping.py.',
       'goldens': {}}
for f in sorted(glob.glob(G + '/specs/*.json')):
    spec = json.load(open(f))
    out['goldens'][spec['id']] = gen(spec)
json.dump(out, open(sys.argv[1], 'w'), indent=1)
