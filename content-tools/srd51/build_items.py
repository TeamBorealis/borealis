"""Build SRD 5.1 equipment + magic items (clean-room: text from the SRD PDF events, numbers from its tables).

Run:  python3 -I build_items.py
Writes app/content/srd-5.1/items/*.json
"""
import re, sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

EV = events()
USED_IDS = {}

def uid(i, name):
    if i in USED_IDS and USED_IDS[i] != name:
        raise SystemExit(f'duplicate id {i}: {USED_IDS[i]!r} vs {name!r}')
    USED_IDS[i] = name
    return i

def up(s):
    s = s.upper().replace('’', '').replace("'", '')
    return re.sub(r'[^A-Z0-9]+', '_', s).strip('_')

# ------------------------------------------------------------------ descriptions by run-in label (equipment pages)
def label_descriptions(pmin, pmax):
    out = {}
    for e in EV:
        if e['k'] == 'P' and pmin <= e['page'] <= pmax:
            m = re.match(r'^\s*<i><b>\s*(.*?)\s*</b></i>(.*)$', e['html'])
            if m:
                label = clean(re.sub(r'<[^>]+>', '', m.group(1))).rstrip('.')
                rest = clean_inline(m.group(2))
                out.setdefault(norm_name(label), []).append(rest)
    return out

LABELS = label_descriptions(60, 68)

def desc_for(*names):
    for n in names:
        k = norm_name(n)
        if k in LABELS:
            return ''.join(f'<p>{p}</p>' for p in LABELS[k])
    return ''

def desc_lookup(name):
    cands = [name]
    base = re.sub(r'\s*\(.*?\)', '', name).strip()
    cands.append(base)
    if ',' in base:
        cands.append(base.split(',')[0])
        cands.append(base.split(',')[1].strip() + ' ' + base.split(',')[0])
    return desc_for(*cands)

# ------------------------------------------------------------------ cost / weight
CUR = {'cp': 'cp', 'sp': 'sp', 'ep': 'ep', 'gp': 'gp', 'pp': 'pp'}
FR = {'½': 0.5, '¼': 0.25}

def parse_weight(s):
    s = s.strip()
    if s in ('—', '-', '', '*'):
        return None, None
    m = re.match(r'^(\d+)?\s*([½¼]|\d+/\d+)?\s*lb', s)
    if not m:
        return None, None
    v = 0.0
    if m.group(1): v += int(m.group(1))
    f = m.group(2)
    if f:
        v += FR[f] if f in FR else int(f.split('/')[0]) / int(f.split('/')[1])
    shown = re.sub(r'\s*\(full\)', '', s)
    return v, s

def lbs(v):
    return str(int(v)) if v == int(v) else repr(round(v, 4))

def cost_setter(num, cur):
    return setter('cost', num.replace(',', ''), currency=cur)

def weight_setter(w):
    v, shown = parse_weight(w)
    if v is None:
        return None
    return setter('weight', shown, lb=lbs(v))

# ------------------------------------------------------------------ T-event rows
def trows(page_min, col_filter=None, start_text=None, end_text=None):
    rows = []
    on = start_text is None
    for e in EV:
        if e['k'] != 'T' or not (page_min[0] <= e['page'] <= page_min[1]):
            continue
        t = clean(e['plain'])
        if not on:
            if t.startswith(start_text) and (col_filter is None or e['col'] == col_filter):
                on = True
            continue
        if end_text and t.startswith(end_text):
            break
        rows.append(t)
    return rows

PRICE = re.compile(r'^(?P<name>.+?) (?P<cost>\d[\d,]*) (?P<cur>[cseg]p|pp)(?: (?P<rest>.*))?$')

# ================================================================== WEAPONS
GROUP = {'Club': 'CLUBS', 'Greatclub': 'CLUBS', 'Handaxe': 'AXES', 'Battleaxe': 'AXES', 'Greataxe': 'AXES',
         'Light hammer': 'HAMMERS', 'Warhammer': 'HAMMERS', 'Mace': 'MACES', 'Spear': 'SPEARS',
         'Crossbow, light': 'CROSSBOWS', 'Crossbow, hand': 'CROSSBOWS', 'Crossbow, heavy': 'CROSSBOWS',
         'Shortbow': 'BOWS', 'Longbow': 'BOWS', 'Greatsword': 'SWORDS', 'Longsword': 'SWORDS',
         'Shortsword': 'SWORDS', 'Maul': 'MAULS'}
PROP_ID = {'ammunition': 'AMMUNITION', 'finesse': 'FINESSE', 'heavy': 'HEAVY', 'light': 'LIGHT',
           'loading': 'LOADING', 'reach': 'REACH', 'thrown': 'THROWN', 'two-handed': 'TWOHANDED',
           'versatile': 'VERSATILE'}

def build_weapons():
    rows = trows((65, 66), start_text='Name Cost Damage Weight Properties')
    cat = None
    out = []
    for t in rows:
        m = re.match(r'^(Simple|Martial) (Melee|Ranged) Weapons$', t)
        if m:
            cat = f'{m.group(1).upper()}_{m.group(2).upper()}'
            continue
        m = re.match(r'^(?P<name>.+?) (?P<cost>\d+) (?P<cur>[cseg]p) (?P<dmg>—|\d+(?:d\d+)?(?: [a-z]+)?) (?P<wt>—|[\d½¼/ ]+ ?lb\.) (?P<props>.*)$', t)
        if not m or cat is None:
            continue
        name = m.group('name')
        disp = ', '.join(w.capitalize() if i == 0 else w for i, w in enumerate(name.split(', '))) if ',' in name else name
        disp = name[0].upper() + name[1:]
        if ', ' in disp:
            a, b = disp.split(', ')
            disp = f'{a}, {b.capitalize()}'
        disp = ' '.join(w if w.islower() and False else w for w in disp.split(' '))
        # title-case words like "Light hammer" -> "Light Hammer", "War pick" -> "War Pick"
        disp = re.sub(r'\b([a-z])', lambda mm: mm.group(1).upper(), disp) if name not in ('Crossbow, light',) else disp
        disp = disp.replace('Crossbow, Light', 'Crossbow, Light')
        wid = fixture_id('Weapon', disp) or f'ID_WOTC_PHB_WEAPON_{up(disp)}'
        props_text = m.group('props').replace('--', '-')
        props = [p.strip() for p in re.split(r',\s*(?![^()]*\))', props_text) if p.strip() and p.strip() != '—']
        supports = [f'ID_INTERNAL_WEAPON_CATEGORY_{cat}']
        dmg = m.group('dmg')
        dtype = None; dice = None
        if dmg != '—':
            parts = dmg.split(' ')
            dice = parts[0]; dtype = parts[1] if len(parts) > 1 else None
            if dtype:
                supports.append(f'ID_INTERNAL_DAMAGE_TYPE_{dtype.upper()}')
        rng = None; versatile = None; twohand = False
        for p in props:
            key = re.match(r'^[A-Za-z-]+', p).group(0).lower()
            if key in PROP_ID:
                supports.append(f'ID_INTERNAL_WEAPON_PROPERTY_{PROP_ID[key]}')
            if key == 'two-handed': twohand = True
            if key == 'special':
                supports += ['ID_INTERNAL_WEAPON_PROPERTY_SPECIAL']
                if disp == 'Lance': supports.append('ID_INTERNAL_WEAPON_PROPERTY_SPECIAL_LANCE')
                if disp == 'Net': supports.append('ID_INTERNAL_WEAPON_PROPERTY_SPECIAL_NET')
            r = re.search(r'range (\d+/\d+)', p)
            if r: rng = r.group(1)
            v = re.search(r'Versatile \((\S+)\)', p)
            if v: versatile = v.group(1)
        if name in GROUP:
            supports.append(f'ID_INTERNAL_WEAPON_GROUP_{GROUP[name]}')
        sets = [setter('category', 'Weapons'), cost_setter(m.group('cost'), m.group('cur'))]
        w = weight_setter(m.group('wt'))
        sets.append(w if w else setter('weight', '—', lb='0'))
        sets.append(setter('slot', 'twohand' if twohand else 'onehand'))
        if rng: sets.append(setter('range', rng))
        if dice:
            sets.append(setter('damage', dice, **({'type': dtype} if dtype else {})))
        if versatile: sets.append(setter('versatile', versatile))
        sets.append(setter('proficiency', 'ID_PROFICIENCY_WEAPON_PROFICIENCY_' + up(disp)))
        out.append(element(uid(wid, disp), disp, 'Weapon', desc_lookup(disp), supports=supports, setters=sets))
    return out

# ================================================================== ARMOR
ARMOR = [  # name, cost gp, base AC, group, str, stealth, lb, acText
    ('Padded', 5, 11, 'Light', None, True, 8, '11 + Dex modifier'),
    ('Leather', 10, 11, 'Light', None, False, 10, '11 + Dex modifier'),
    ('Studded Leather', 45, 12, 'Light', None, False, 13, '12 + Dex modifier'),
    ('Hide', 10, 12, 'Medium', None, False, 12, '12 + Dex modifier (max 2)'),
    ('Chain Shirt', 50, 13, 'Medium', None, False, 20, '13 + Dex modifier (max 2)'),
    ('Scale Mail', 50, 14, 'Medium', None, True, 45, '14 + Dex modifier (max 2)'),
    ('Breastplate', 400, 14, 'Medium', None, False, 20, '14 + Dex modifier (max 2)'),
    ('Half Plate', 750, 15, 'Medium', None, True, 40, '15 + Dex modifier (max 2)'),
    ('Ring Mail', 30, 14, 'Heavy', None, True, 40, '14'),
    ('Chain Mail', 75, 16, 'Heavy', 13, True, 55, '16'),
    ('Splint', 200, 17, 'Heavy', 15, True, 60, '17'),
    ('Plate', 1500, 18, 'Heavy', 15, True, 65, '18'),
]

def build_armor():
    out = []
    for name, cost, ac, grp, strn, stealth, lb, actext in ARMOR:
        aid = fixture_id('Armor', name) or f'ID_WOTC_ARMOR_{grp.upper()}_{up(name)}'
        sets = [setter('category', 'Armor'), cost_setter(str(cost), 'gp'), setter('weight', f'{lb} lb.', lb=lb),
                setter('slot', 'body'), setter('armor', grp), setter('armorClass', actext)]
        if strn: sets.append(setter('strength', strn))
        if stealth: sets.append(setter('stealth', 'Disadvantage'))
        sets.append(setter('proficiency', 'ID_PROFICIENCY_ARMOR_PROFICIENCY_' + up(name)))
        rules = []
        if stealth: rules.append(grant('ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE', 'Grants'))
        rules.append(stat('ac:armored:armor', ac))
        if strn: rules.append(stat('speed', -10, bonus='armor', requirements=f'![str:{strn}]'))
        out.append(element(uid(aid, name), name, 'Armor', desc_lookup(name),
                           supports=[f'ID_INTERNAL_ARMOR_GROUP_{grp.upper()}'], setters=sets, rules=rules))
    sets = [setter('category', 'Armor'), cost_setter('10', 'gp'), setter('weight', '6 lb.', lb=6),
            setter('slot', 'onehand,secondary'), setter('type', 'Shield'), setter('armor', 'Shield'),
            setter('armorClass', '+2'), setter('proficiency', 'ID_PROFICIENCY_ARMOR_PROFICIENCY_SHIELD')]
    out.append(element(uid(fixture_id('Armor', 'Shield') or 'ID_WOTC_GEAR_SHIELD', 'Shield'), 'Shield', 'Armor',
                       desc_for('Shields') or '<p>A shield is made from wood or metal and is carried in one hand. Wielding a shield increases your Armor Class by 2. You can benefit from only one shield at a time.</p>',
                       supports=['ID_INTERNAL_ARMOR_GROUP_SHIELD'], setters=sets,
                       rules=[stat('ac:shield', 2, bonus='shield')]))
    return out

# ================================================================== GEAR / TOOLS / MOUNTS
ALIAS_ID = {
    'Arrows (20)': 'ID_WOTC_PHB_ITEM_ARROW', 'Blowgun needles (50)': 'ID_WOTC_PHB_ITEM_BLOWGUNNEEDLE',
    'Crossbow bolts (20)': 'ID_WOTC_PHB_ITEM_CROSSBOWBOLT', 'Sling bullets (20)': 'ID_WOTC_PHB_ITEM_SLINGBULLET',
    'Chalk (1 piece)': 'ID_WOTC_PHB_ITEM_CHALK_1PIECE', 'Rod': 'ID_WOTC_PHB_ITEM_ROD',
    'Ladder (10-foot)': 'ID_WOTC_PHB_ITEM_LADDER_10_FOOT', 'Pole (10-foot)': 'ID_WOTC_PHB_ITEM_POLE_10_FOOT',
    'Tent, two-person': 'ID_WOTC_PHB_ITEM_TENT_TWOPERSON', 'Manacles': 'ID_WOTC_PHB_ITEM_MANACLES',
}

def item_name(raw):
    # capitalise each word of the SRD table row (except small words and anything inside parentheses)
    small = {'of', 'or', 'and', 'the', 'a'}
    out = []
    depth = 0
    for i, w in enumerate(raw.split(' ')):
        inside = depth > 0 or w.startswith('(')
        depth += w.count('(') - w.count(')')
        if inside or (i > 0 and w in small):
            out.append(w)
        else:
            out.append(w[0].upper() + w[1:])
    return ' '.join(out).replace('Two-person', 'Two-Person')

GEAR_GROUP = {'Arcane focus': ('Spellcasting Focus', 'Arcane Focus'), 'Druidic focus': ('Spellcasting Focus', 'Druidic Focus'),
              'Holy symbol': ('Spellcasting Focus', 'Holy Symbol')}

def build_gear():
    rows = trows((69, 69), start_text='Item Cost Weight', end_text='Container Capacity')
    out = []
    group = None
    ammo = False
    for t in rows:
        if t in ('Item Cost Weight',): continue
        m = PRICE.match(t)
        if not m:
            group = t
            ammo = (t == 'Ammunition')
            continue
        raw = m.group('name')
        wt = (m.group('rest') or '').strip()
        # row like "Antitoxin (vial) 50 gp —" handled by rest
        disp = item_name(raw)
        # sub-items of groups (indented in the PDF but flat here)
        in_group = group in GEAR_GROUP and raw in {'Crystal', 'Orb', 'Rod', 'Staff', 'Wand', 'Sprig of mistletoe', 'Totem', 'Wooden staff', 'Yew wand', 'Amulet', 'Emblem', 'Reliquary'}
        if not in_group and group in GEAR_GROUP:
            group = None
        iid = ALIAS_ID.get(raw) or fixture_id('Item', disp) or f'ID_WOTC_PHB_ITEM_{up(disp)}'
        sets = []
        if in_group:
            cat, cont = GEAR_GROUP[group]
            sets += [setter('category', cat)]
        elif ammo:
            sets += [setter('category', 'Ammunition')]
        else:
            sets += [setter('category', 'Adventuring Gear')]
        sets.append(cost_setter(m.group('cost'), m.group('cur')))
        w = weight_setter(wt)
        sets.append(w if w else setter('weight', '—', lb='0'))
        if ammo: sets.append(setter('type', 'Ammunition'))
        if in_group:
            sets.append(setter('container', cont)); sets.append(setter('slot', 'onehand'))
        desc = desc_lookup(disp) if not in_group else desc_for(GEAR_GROUP[group][1])
        out.append(element(uid(iid, disp), disp, 'Item', desc, setters=sets))
    return out

TOOL_GROUPS = {'Artisan’s tools': 'Artisan’s Tools', 'Gaming set': 'Gaming Sets', 'Musical instrument': 'Musical Instruments'}

def build_tools():
    rows = trows((70, 70), start_text='Item Cost Weight', end_text='Vehicles (land or water)')
    out = []
    group = None
    for t in rows:
        if t == 'Item Cost Weight': continue
        m = PRICE.match(t)
        if not m:
            group = t.replace("'", '’'); continue
        raw = m.group('name').replace("'", '’')
        disp = item_name(raw)
        disp = disp.replace('’S', '’s')
        is_inst = group == 'Musical instrument'
        is_game = group == 'Gaming set' and disp.lower().endswith(' set')
        cat = 'Musical Instruments' if is_inst else ('Gaming Sets' if is_game else 'Tools')
        iid = fixture_id('Item', disp)
        if not iid:
            if is_inst: iid = f'ID_WOTC_SRD_INSTRUMENT_{up(disp).replace("_", "")}'
            else: iid = f'ID_WOTC_PHB_ITEM_TOOL_{up(disp)}'
        sets = [setter('category', cat), cost_setter(m.group('cost'), m.group('cur'))]
        w = weight_setter((m.group('rest') or '').strip())
        sets.append(w if w else setter('weight', '—', lb='0'))
        if is_inst: sets.append(setter('slot', 'onehand'))
        sets.append(setter('type', 'Tool'))
        pid = None
        if is_inst:
            pid = 'ID_PROFICIENCY_TOOL_PROFICIENCY_' + up(disp)
        elif is_game:
            pid = 'ID_PROFICIENCY_GAMING_SET_PROFICIENCY_' + up(disp)
        else:
            pid = 'ID_PROFICIENCY_TOOL_PROFICIENCY_' + up(disp)
        sets.append(setter('proficiency', pid))
        out.append(element(uid(iid, disp), disp, 'Item', '', setters=sets,
                           supports=['ID_INTERNAL_TOOL_MUSICAL_INSTRUMENT'] if is_inst else None))
    return out

def build_mounts():
    out = []
    # mounts and other animals
    rows = trows((71, 72), start_text='Item Cost Speed Capacity', end_text='Tack, Harness')
    for t in rows:
        m = re.match(r'^(?P<name>.+?) (?P<cost>\d+) gp (?P<speed>\d+) ft\. (?P<cap>[\d,]+) lb\.$', t)
        if not m: continue
        disp = item_name(m.group('name'))
        sets = [setter('category', 'Mounts and Other Animals'), cost_setter(m.group('cost'), 'gp'),
                setter('speed', m.group('speed') + ' ft.'), setter('capacity', m.group('cap') + ' lb.')]
        out.append(element(uid(f'ID_WOTC_PHB_ITEM_MOUNT_{up(disp)}', disp), disp, 'Item', '', setters=sets))
    # tack, harness, drawn vehicles
    rows = trows((72, 72), start_text='Item Cost Weight', end_text='Waterborne Vehicles')
    saddle = False
    for t in rows:
        if t == 'Item Cost Weight': continue
        if t == 'Saddle': saddle = True; continue
        if t.startswith('Barding'):
            out.append(element(uid('ID_WOTC_PHB_ITEM_TACK_BARDING', 'Barding'), 'Barding', 'Item',
                               '<p>Barding is armor designed to protect an animal’s head, neck, chest, and body. Any type of armor shown in the Armor table can be purchased as barding. The cost is four times the equivalent armor made for humanoids, and it weighs twice as much.</p>',
                               setters=[setter('category', 'Tack and Harness')]))
            continue
        m = PRICE.match(t)
        if not m: continue
        raw = m.group('name')
        if saddle and raw in ('Exotic', 'Military', 'Pack', 'Riding'):
            disp = f'Saddle, {raw}'
        else:
            if raw == 'Saddlebags': saddle = False
            disp = item_name(raw)
        cat = 'Drawn Vehicles' if disp in ('Carriage', 'Cart', 'Chariot', 'Sled', 'Wagon') else 'Tack and Harness'
        sets = [setter('category', cat), cost_setter(m.group('cost'), m.group('cur'))]
        w = weight_setter((m.group('rest') or '').strip())
        if w: sets.append(w)
        out.append(element(uid(f'ID_WOTC_PHB_ITEM_{"VEHICLE" if cat == "Drawn Vehicles" else "TACK"}_{up(disp)}', disp), disp, 'Item', '', setters=sets))
    # waterborne
    rows = trows((72, 72), start_text='Item Cost Speed', end_text='Trade Goods')
    for t in rows:
        m = re.match(r'^(?P<name>[A-Za-z ]+?) (?P<cost>[\d,]+) gp (?P<speed>[\d½]+ mph)$', t)
        if not m: continue
        disp = item_name(m.group('name'))
        sets = [setter('category', 'Waterborne Vehicles'), cost_setter(m.group('cost'), 'gp'), setter('speed', m.group('speed'))]
        out.append(element(uid(f'ID_WOTC_PHB_ITEM_VEHICLE_{up(disp)}', disp), disp, 'Item', '', setters=sets))
    return out

TRADE = [('1', 'cp', ['Wheat (1 lb.)']), ('2', 'cp', ['Flour (1 lb.)', 'Chicken']), ('5', 'cp', ['Salt (1 lb.)']),
         ('1', 'sp', ['Iron (1 lb.)', 'Canvas (1 sq. yd.)']), ('5', 'sp', ['Copper (1 lb.)', 'Cotton Cloth (1 sq. yd.)']),
         ('1', 'gp', ['Ginger (1 lb.)', 'Goat']), ('2', 'gp', ['Cinnamon (1 lb.)', 'Pepper (1 lb.)', 'Sheep']),
         ('3', 'gp', ['Cloves (1 lb.)', 'Pig']), ('5', 'gp', ['Silver (1 lb.)', 'Linen (1 sq. yd.)']),
         ('10', 'gp', ['Silk (1 sq. yd.)', 'Cow']), ('15', 'gp', ['Saffron (1 lb.)', 'Ox']),
         ('50', 'gp', ['Gold (1 lb.)']), ('500', 'gp', ['Platinum (1 lb.)'])]

def build_trade():
    # cross-check cost list against the SRD table text
    rows = trows((72, 72), start_text='Cost Goods', end_text='Expenses')
    found = [re.match(r'^(\d+) ([cseg]p)', r).groups() for r in rows if re.match(r'^\d+ [cseg]p', r)]
    assert found == [(c, u) for c, u, _ in TRADE], found
    out = []
    for c, u, names in TRADE:
        for n in names:
            base = re.sub(r'\s*\(.*\)', '', n)
            out.append(element(uid(f'ID_WOTC_PHB_ITEM_TRADE_GOOD_{up(n)}', n), n, 'Item', '',
                               setters=[setter('category', 'Trade Goods'), cost_setter(c, u), setter('stackable', 'true')]))
    return out

PACKS = {
    "Burglar’s Pack": (16, [('BACKPACK', 1), ('BALLBEARINGS', 1), ('BELL', 1), ('CANDLE', 5), ('CROWBAR', 1), ('HAMMER', 1), ('PITON', 10), ('LANTERN_HOODED', 1), ('OIL_FLASK', 2), ('RATIONS_1DAY', 5), ('TINDERBOX', 1), ('WATERSKIN', 1), ('ROPE_HEMPEN_50FEET', 1)], None),
    "Diplomat’s Pack": (39, [('CHEST', 1), ('CASE_MAPORSCROLL', 2), ('CLOTHES_FINE', 1), ('INK_1OUNCEBOTTLE', 1), ('INKPEN', 1), ('LAMP', 1), ('OIL_FLASK', 2), ('PAPER_ONESHEET', 5), ('PERFUME_VIAL', 1), ('SEALINGWAX', 1), ('SOAP', 1)], None),
    "Dungeoneer’s Pack": (12, [('BACKPACK', 1), ('CROWBAR', 1), ('HAMMER', 1), ('PITON', 10), ('TORCH', 10), ('TINDERBOX', 1), ('RATIONS_1DAY', 10), ('WATERSKIN', 1), ('ROPE_HEMPEN_50FEET', 1)], None),
    "Entertainer’s Pack": (40, [('BACKPACK', 1), ('BEDROLL', 1), ('CLOTHES_COSTUME', 2), ('CANDLE', 5), ('RATIONS_1DAY', 5), ('WATERSKIN', 1), ('TOOL_DISGUISE_KIT', 1)], None),
    "Explorer’s Pack": (10, [('BACKPACK', 1), ('BEDROLL', 1), ('MESSKIT', 1), ('TINDERBOX', 1), ('TORCH', 10), ('RATIONS_1DAY', 10), ('WATERSKIN', 1), ('ROPE_HEMPEN_50FEET', 1)], None),
    "Priest’s Pack": (19, [('BACKPACK', 1), ('BLANKET', 1), ('CANDLE', 10), ('TINDERBOX', 1), ('RATIONS_1DAY', 2), ('WATERSKIN', 1)], 'Also includes an alms box, 2 blocks of incense, a censer, and vestments.'),
    "Scholar’s Pack": (40, [('BACKPACK', 1)], 'Also includes a book of lore, a bottle of ink, an ink pen, 10 sheets of parchment, a little bag of sand, and a small knife.'),
}
PACK_TEXT = {}

def build_packs():
    # the pack descriptions come from the 'Equipment Packs' paragraphs (T events, pages 70)
    rows = trows((70, 70), start_text='individually.', end_text='Tools')
    txt = ' '.join(rows)
    out = []
    for name, (cost, items, extra) in PACKS.items():
        m = re.search(re.escape(name) + r' \(\d+ gp\)\. (Includes .*?\.)(?= [A-Z][a-z’]+(?:’s)? Pack \(|$)', txt)
        d = m.group(1) if m else ''
        # the final "The pack also has ..." sentence stays inside d when present
        pid = fixture_id('Item', name) or f'ID_WOTC_ITEM_{up(name)}'
        ex = {'items': [({'id': 'ID_WOTC_PHB_ITEM_' + i} | ({'amount': n} if n != 1 else {})) for i, n in items]}
        if extra or d:
            ex['description'] = f'<p>{d}</p>'
        sets = [setter('category', 'Equipment Packs'), cost_setter(str(cost), 'gp')]
        out.append(element(uid(pid, name), name, 'Item', f'<p>{d}</p>' if d else '', setters=sets, extract=ex))
    return out

# ================================================================== MAGIC ITEMS
TYPE_RX = re.compile(r'^\s*(Armor|Weapon|Wondrous item|Potion|Ring|Rod|Staff|Wand|Scroll)\b', re.I)
RARITIES = [('very rare', 'Very Rare'), ('uncommon', 'Uncommon'), ('common', 'Common'), ('rare', 'Rare'),
            ('legendary', 'Legendary'), ('artifact', 'Artifact'), ('varies', 'Varies')]
CATEGORY = {'Armor': 'Magic Armor', 'Weapon': 'Magic Weapons', 'Wondrous item': 'Wondrous Items', 'Potion': 'Potions',
            'Ring': 'Rings', 'Rod': 'Rods', 'Staff': 'Staffs', 'Wand': 'Wands', 'Scroll': 'Spell Scrolls'}
FILE_OF = {'Armor': 'armor', 'Weapon': 'weapons', 'Wondrous item': 'wondrous', 'Potion': 'potions',
           'Ring': 'rings', 'Rod': 'rods-staffs-wands', 'Staff': 'rods-staffs-wands', 'Wand': 'rods-staffs-wands'}

ID_ALIAS = {'Stone of Good Luck (Luckstone)': 'ID_WOTC_DMG_MAGIC_ITEM_STONE_OF_GOOD_LUCK',
            'Feather Token': 'ID_WOTC_DMG_MAGIC_ITEM_QUAALS_FEATHER_TOKEN'}
TYPES = ['acid', 'cold', 'fire', 'force', 'lightning', 'necrotic', 'poison', 'psychic', 'radiant', 'thunder']

def magic_events():
    a = [i for i, e in enumerate(EV) if e['k'] == 'H' and clean(e['text']) == 'Magic Items A-Z'][0]
    b = [i for i, e in enumerate(EV) if e['k'] == 'H' and 'Sentient' in e['text']][0]
    heads = []
    for i in range(a + 1, b):
        e = EV[i]
        if e['k'] == 'H' and EV[i + 1]['k'] == 'P' and TYPE_RX.match(EV[i + 1]['plain']):
            heads.append(i)
    items = []
    for n, i in enumerate(heads):
        end = heads[n + 1] if n + 1 < len(heads) else b
        # body ends at the next heading of any kind
        j = i + 2
        evs = []
        while j < end and EV[j]['k'] != 'H':
            evs.append(EV[j]); j += 1
        items.append((clean(EV[i]['text']), clean(EV[i + 1]['plain']), evs))
    return items

def body_html(evs):
    out = []
    tbl = []
    def flush():
        nonlocal tbl
        if tbl:
            out.append('<p class="table-text">' + '<br />'.join(esc(r) for r in tbl) + '</p>')
            tbl = []
    bullets = []
    def flushb():
        nonlocal bullets
        if bullets:
            out.append('<ul>' + ''.join(f'<li>{b}</li>' for b in bullets) + '</ul>')
            bullets = []
    for e in evs:
        if e['k'] == 'T':
            flushb()
            t = clean(e['plain'])
            if t: tbl.append(t)
            continue
        flush()
        h = clean_inline(e['html'])
        if not h: continue
        if e['k'] == 'B':
            bullets.append(re.sub(r'^•\s*', '', h))
        else:
            flushb(); out.append(f'<p>{h}</p>')
    flush(); flushb()
    return ''.join(out)

def esc(s):
    return s.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;')

def parse_typeline(line):
    m = TYPE_RX.match(line)
    typ = m.group(1)
    typ = 'Wondrous item' if typ.lower().startswith('wondrous') else typ.capitalize()
    low = line.lower()
    param = re.match(r'^[A-Za-z ]+?\s*\(([^)]*)\)', line)
    param = param.group(1) if param and not param.group(1).lower().startswith('requires') and not param.group(1).startswith('+') else None
    rarity = None
    tail = re.sub(r'\(requires attunement[^)]*\)', '', low)
    tail = tail.split(',', 1)[1] if ',' in tail else tail
    if 'varies' in tail and 'rarity' in low: rarity = 'Varies'
    else:
        for key, val in RARITIES:
            if re.search(r'\b' + key + r'\b', tail):
                rarity = val; break
    att = re.search(r'\(requires attunement(?: ([^)]*))?\)', line)
    return typ, param, rarity, bool(att), (att.group(1) if att and att.group(1) else None)

SLOT_RULES = [(r'^Ring of', 'ring'), (r'^(Amulet|Periapt|Necklace|Medallion|Brooch|Scarab|Talisman)', 'neck'),
              (r'^(Boots|Slippers)', 'feet'), (r'^(Gloves|Gauntlets)', 'hands'), (r'^Bracers', 'arms'),
              (r'^(Cloak|Cape|Mantle|Wings)', 'shoulders'), (r'^Belt', 'waist'),
              (r'^(Helm|Hat|Circlet|Goggles|Eyes of|Headband)', 'head'), (r'^Robe', 'misc')]

def slot_of(name, typ):
    if typ == 'Ring': return 'ring'
    if typ in ('Rod', 'Staff', 'Wand'): return 'onehand'
    if typ != 'Wondrous item': return None
    for rx, s in SLOT_RULES:
        if re.search(rx, name): return s
    return None

def weapon_expr(param):
    p = (param or '').lower()
    if not p or p == 'any': return 'ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_MELEE||ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_RANGED||ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE||ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_RANGED'
    if p.startswith('any ammunition') or p == 'arrow': return None
    if p == 'any sword': return 'ID_INTERNAL_WEAPON_GROUP_SWORDS'
    if p == 'any axe': return 'ID_INTERNAL_WEAPON_GROUP_AXES'
    if p == 'any axe or sword': return 'ID_INTERNAL_WEAPON_GROUP_SWORDS||ID_INTERNAL_WEAPON_GROUP_AXES'
    if p.startswith('any sword that deals slashing'): return 'ID_INTERNAL_WEAPON_GROUP_SWORDS,ID_INTERNAL_DAMAGE_TYPE_SLASHING'
    return item_name(p)

def armor_expr(param):
    p = (param or '').lower()
    if p == 'light, medium, or heavy': return 'ID_INTERNAL_ARMOR_GROUP_LIGHT|ID_INTERNAL_ARMOR_GROUP_MEDIUM|ID_INTERNAL_ARMOR_GROUP_HEAVY'
    if p.startswith('medium or heavy'): return '(ID_INTERNAL_ARMOR_GROUP_MEDIUM||ID_INTERNAL_ARMOR_GROUP_HEAVY),!Hide'
    if p == 'shield': return 'ID_INTERNAL_ARMOR_GROUP_SHIELD'
    return item_name(p)

def resist_rules(t):
    return [grant(f'ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_{t.upper()}', 'Condition')]

SAVES = ['strength', 'dexterity', 'constitution', 'intelligence', 'wisdom', 'charisma']

def protection_rules(n):
    return [stat('ac:misc', n)] + [stat(f'{a}:save:misc', n) for a in SAVES]

# curated mechanics (only where the SRD text states a plain numeric/granted effect)
def curated_rules(name):
    sets = {
        'Amulet of Health': [stat('constitution:score:set', 19, bonus='base')],
        'Headband of Intellect': [stat('intelligence:score:set', 19, bonus='base')],
        'Gauntlets of Ogre Power': [stat('strength:score:set', 19, bonus='base')],
        'Cloak of Protection': protection_rules(1),
        'Ring of Protection': protection_rules(1),
        'Bracers of Defense': [stat('ac:misc', 2, equipped='[armor:none],[shield:none]')],
        'Goggles of Night': [grant('ID_VISION_DARKVISION', 'Vision')],
        'Ring of Warmth': resist_rules('cold'),
        'Brooch of Shielding': resist_rules('force'),
        'Periapt of Proof against Poison': [grant('ID_INTERNAL_CONDITION_DAMAGE_IMMUNITY_POISON', 'Condition')],
        'Staff of Power': [stat('ac:misc', 2)] + [stat(f'{a}:save:misc', 2) for a in SAVES],
        'Dwarven Plate': [stat('ac:armored:enhancement', 2, bonus='enhancement')],
    }
    return sets.get(name, [])

BELTS = [('Hill', 21, 'Rare'), ('Frost', 23, 'Very Rare'), ('Stone', 23, 'Very Rare'), ('Fire', 25, 'Very Rare'),
         ('Cloud', 27, 'Legendary'), ('Storm', 29, 'Legendary')]
DSM = [('Black', 'acid'), ('Blue', 'lightning'), ('Brass', 'fire'), ('Bronze', 'lightning'), ('Copper', 'acid'),
       ('Gold', 'fire'), ('Green', 'poison'), ('Red', 'fire'), ('Silver', 'cold'), ('White', 'cold')]
BONUS_RARITY = {1: 'Uncommon', 2: 'Rare', 3: 'Very Rare'}
ARMOR_RARITY = {1: 'Rare', 2: 'Very Rare', 3: 'Legendary'}

def mk(name, id_, typ, desc, rarity, att, att_add, param=None, extra_sets=None, rules=None, supports=None,
       category=None, slot='auto', enh=None):
    sets = []
    if category is None: category = CATEGORY[typ]
    sets.append(setter('category', category))
    sl = slot_of(name, typ) if slot == 'auto' else slot
    if typ == 'Armor' and slot == 'auto': sl = 'body' if (param or '').lower() != 'shield' else None
    if sl: sets.append(setter('slot', sl))
    tattrs = {'addition': param} if param and typ in ('Armor', 'Weapon') else {}
    sets.append(setter('type', typ, **tattrs))
    if att:
        sets.append(setter('attunement', 'true', **({'addition': att_add} if att_add else {})))
    if rarity: sets.append(setter('rarity', rarity))
    if enh is not None: sets.append(setter('enhancement', enh))
    if typ == 'Armor' and param is not None and category == 'Magic Armor':
        pass
    sets += (extra_sets or [])
    return element(uid(id_, name), name, 'Magic Item', desc, setters=sets, rules=rules, supports=supports)

def build_magic():
    out = {}
    def put(typ, el):
        out.setdefault(FILE_OF[typ], []).append(el)
    skipped = []
    for name, tline, evs in magic_events():
        typ, param, rarity, att, att_add = parse_typeline(tline)
        desc = body_html(evs)
        fid = ID_ALIAS.get(name) or fixture_id('Magic Item', name)
        base_id = fid or f'ID_WOTC_DMG_MAGIC_ITEM_{up(name)}'
        enh = None
        text = text_of(evs)
        m = re.search(r'\+(\d) bonus to (?:AC|attack and damage)', text)
        if name == 'Spell Scroll':
            skipped.append(name); continue
        # ---- families with per-bonus elements
        if name.startswith(('Armor, +1', 'Shield, +1', 'Weapon, +1', 'Ammunition, +1', 'Wand of the War Mage')):
            base = name.split(',')[0] if ',' in name else 'Wand of the War Mage'
            for n in (1, 2, 3):
                if base == 'Wand of the War Mage':
                    nm = f'Wand of the War Mage, +{n}'; i = f'ID_WOTC_DMG_MAGIC_ITEM_WAND_OF_THE_WAR_MAGE_{n}'
                    el = mk(nm, i, 'Wand', desc, BONUS_RARITY[n], True, 'by a spellcaster')
                    put('Wand', el); continue
                nm = f'{base}, +{n}'
                i = f'ID_WOTC_DMG_MAGIC_ITEM_{base.upper()}_{n}'
                if base == 'Armor':
                    el = mk(nm, i, 'Armor', desc, ARMOR_RARITY[n], False, None, param=param,
                            extra_sets=[setter('enhancement', n), setter('armor', armor_expr(param)), setter('name-format', '{{parent}} +{{enhancement}}')],
                            rules=[stat('ac:armored:enhancement', n, bonus='enhancement')])
                    put('Armor', el)
                elif base == 'Shield':
                    el = mk(nm, i, 'Armor', desc, BONUS_RARITY[n], False, None, param='shield',
                            extra_sets=[setter('enhancement', n), setter('armor', 'ID_INTERNAL_ARMOR_GROUP_SHIELD'), setter('name-format', '{{parent}} +{{enhancement}}')],
                            # ac:shield (the Aurora DMG shape): the bonus applies without body armor
                            rules=[stat('ac:shield', n, bonus='enhancement')], slot=None)
                    put('Armor', el)
                elif base == 'Weapon':
                    el = mk(nm, i, 'Weapon', desc, BONUS_RARITY[n], False, None, param='any',
                            extra_sets=[setter('weapon', weapon_expr('any')), setter('name-format', '{{parent}} +{{enhancement}}')], enh=n)
                    put('Weapon', el)
                else:  # Ammunition
                    el = mk(nm, i, 'Weapon', desc, BONUS_RARITY[n], False, None, param='any ammunition', category='Ammunition', enh=n,
                            extra_sets=[setter('keywords', 'ammunition')])
                    put('Weapon', el)
            continue
        if name == 'Armor of Resistance' or name == 'Ring of Resistance' or name == 'Potion of Resistance':
            for t in TYPES:
                T = t.capitalize()
                nm = f'{name}, {T}'
                i = f'ID_WOTC_DMG_MAGIC_ITEM_{up(name)}_{t.upper()}'
                d = f'<p><em>Resistance type: {T}.</em></p>' + desc
                if name == 'Armor of Resistance':
                    el = mk(nm, i, 'Armor', d, rarity, att, att_add, param=param,
                            extra_sets=[setter('armor', armor_expr(param)), setter('name-format', '{{parent}} of ' + T + ' Resistance'), setter('keywords', t)],
                            rules=resist_rules(t))
                    put('Armor', el)
                elif name == 'Ring of Resistance':
                    put('Ring', mk(nm, i, 'Ring', d, rarity, att, att_add, extra_sets=[setter('keywords', t)], rules=resist_rules(t)))
                else:
                    put('Potion', mk(nm, i, 'Potion', d, rarity, att, att_add, extra_sets=[setter('keywords', t)]))
            continue
        if name == 'Belt of Giant Strength':
            for gname, score, rar in BELTS:
                nm = f'Belt of {gname} Giant Strength'
                d = f'<p><em>{gname} giant version: Strength score {score}; rarity {rar.lower()}.</em></p>' + desc
                put('Wondrous item', mk(nm, f'ID_WOTC_DMG_MAGIC_ITEM_BELT_OF_{gname.upper()}_GIANT_STRENGTH', 'Wondrous item', d, rar, True, None,
                                        rules=[stat('strength:score:set', score, bonus='base')]))
            continue
        if name == 'Dragon Scale Mail':
            for color, res in DSM:
                nm = f'Dragon Scale Mail, {color}'
                d = f'<p><em>{color} dragon scale mail: resistance to {res} damage.</em></p>' + desc
                put('Armor', mk(nm, f'ID_WOTC_DMG_MAGIC_ITEM_DRAGON_SCALE_MAIL_{color.upper()}', 'Armor', d, rarity, att, att_add, param=param,
                                extra_sets=[setter('armor', 'Scale Mail'), setter('enhancement', 1), setter('name-format', f'Dragon Scale Mail ({color})'), setter('keywords', res)],
                                rules=[stat('ac:armored:enhancement', 1, bonus='enhancement')] + resist_rules(res)))
            continue
        # ---- single elements
        extra, rules, enhv = [], list(curated_rules(name)), None
        if typ == 'Armor':
            if param: extra.append(setter('armor', armor_expr(param)))
            if m: rules.append(stat('ac:armored:enhancement', int(m.group(1)), bonus='enhancement:shield' if (param or '') == 'shield' else 'enhancement'))
            if name in ('Dwarven Plate',): rules = [r for r in rules if r['name'] != 'ac:armored:enhancement'] + [stat('ac:armored:enhancement', 2, bonus='enhancement')]
            # prefer the single enhancement rule only
            seen = set(); rr = []
            for r in rules:
                k = (r['name'], r.get('bonus'))
                if k in seen: continue
                seen.add(k); rr.append(r)
            rules = rr
        if typ == 'Weapon':
            w = weapon_expr(param)
            if w: extra.append(setter('weapon', w))
            m2 = re.search(r'\+(\d) bonus to attack and damage rolls', text)
            if m2: enhv = int(m2.group(1))
            if name.endswith(('Axe',)) and w: extra.append(setter('name-format', name.replace('Axe', '{{parent}}')))
        el = mk(name, base_id, typ, desc, rarity, att, att_add, param=param, extra_sets=extra, rules=rules or None,
                enh=enhv, slot='auto')
        put(typ, el)
    return out, skipped

# ================================================================== write
INFO = 'SRD 5.1 equipment and magic items. Structured into JSON from the System Reference Document 5.1 (CC-BY-4.0); text condensed/reformatted.'

def main():
    counts = {}
    def w(rel, els, name):
        write_content(rel, els, name=name, description=INFO)
        counts[rel] = len(els)
    w('items/weapons.json', build_weapons(), 'SRD 5.1 Weapons')
    w('items/armor.json', build_armor(), 'SRD 5.1 Armor and Shields')
    w('items/gear.json', build_gear(), 'SRD 5.1 Adventuring Gear')
    w('items/tools.json', build_tools(), 'SRD 5.1 Tools, Gaming Sets and Musical Instruments')
    w('items/mounts-vehicles.json', build_mounts(), 'SRD 5.1 Mounts, Tack and Vehicles')
    w('items/packs.json', build_packs(), 'SRD 5.1 Equipment Packs')
    w('items/trade-goods.json', build_trade(), 'SRD 5.1 Trade Goods')
    magic, skipped = build_magic()
    names = {'armor': 'Magic Armor', 'weapons': 'Magic Weapons', 'wondrous': 'Wondrous Items', 'potions': 'Potions and Oils',
             'rings': 'Rings', 'rods-staffs-wands': 'Rods, Staffs and Wands'}
    for k, els in magic.items():
        w(f'items/magic/{k}.json', els, 'SRD 5.1 Magic Items: ' + names[k])
    for k, v in counts.items():
        print(f'{k}: {v}')
    print('total', sum(counts.values()), 'skipped (synthesized by loader):', skipped)

if __name__ == '__main__':
    main()
