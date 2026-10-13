import sys; sys.path.insert(0,'.')
from lib import *
import subprocess
PDF='../../../input/srd/SRD_CC_v5.2.1.pdf'
def page_layout(p): return subprocess.run(['pdftotext','-layout','-f',str(p),'-l',str(p),PDF,'-'],capture_output=True,text=True).stdout

# ---------- mastery / property descriptions from text
eq=open('equip.txt').read()
def section(text,start,end):
    a=text.index(start); b=text.index(end,a+len(start)) if end else len(text)
    return text[a+len(start):b]
MAST=['Cleave','Graze','Nick','Push','Sap','Slow','Topple','Vex']
mast_text=section(eq,'Improvised Weapons\n','\nWeapons\nName')
# the Improvised callout interrupts: strip it
mast_text=re.sub(r'^\s*If you use an object.*?Ranged weapon to make a melee attack\.\n','',mast_text,flags=re.S|re.M)
def split_named(text,names):
    out={};idx=[]
    for n in names:
        m=re.search(r'^%s$'%re.escape(n),text,re.M)
        idx.append((m.start(),n,m.end()))
    idx.sort()
    for i,(s,n,e) in enumerate(idx):
        end=idx[i+1][0] if i+1<len(idx) else len(text)
        body=[l for l in text[e:end].split('\n')]
        out[n]=chunk_html2(body)
    return out
MASTERY_DESC=split_named(mast_text,MAST)
PROPS=['Ammunition','Finesse','Heavy','Light','Loading','Range','Reach','Thrown','Two-Handed','Versatile']
_a=eq.index('column of the Weapons table.\n')+len('column of the Weapons table.\n'); _b=eq.index('\nMastery Properties\nEach weapon')
prop_text=eq[_a:_b]
PROP_DESC=split_named(prop_text,PROPS)
MAST_ID={m:'ID_2024_MASTERY_'+m.upper() for m in MAST}
for m in MAST:
    emit('equipment/mastery-properties.json', el(MAST_ID[m], m, 'Mastery Property', MASTERY_DESC[m],
        supports=['Mastery Property'], data={'kind':'weapon_mastery','mastery':m.lower()}))

# ---------- weapon property rules text (2024 wording)
for _p,_d in PROP_DESC.items():
    emit('equipment/weapon-properties.json', el('ID_2024_WEAPON_PROPERTY_'+slug(_p),_p,'Weapon Property',_d,supports=['Weapon Property'],data={'property':_p.lower()},
        **({'maps_to':{'dnd2014':'ID_INTERNAL_WEAPON_PROPERTY_'+{'Two-Handed':'TWOHANDED'}.get(_p,slug(_p)),'kind':'equivalent'}})))
# ---------- weapons
PROP_ID={'Ammunition':'ID_INTERNAL_WEAPON_PROPERTY_AMMUNITION','Finesse':'ID_INTERNAL_WEAPON_PROPERTY_FINESSE','Heavy':'ID_INTERNAL_WEAPON_PROPERTY_HEAVY',
 'Light':'ID_INTERNAL_WEAPON_PROPERTY_LIGHT','Loading':'ID_INTERNAL_WEAPON_PROPERTY_LOADING','Range':'ID_INTERNAL_WEAPON_PROPERTY_RANGE','Reach':'ID_INTERNAL_WEAPON_PROPERTY_REACH',
 'Thrown':'ID_INTERNAL_WEAPON_PROPERTY_THROWN','Two-Handed':'ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED','Versatile':'ID_INTERNAL_WEAPON_PROPERTY_VERSATILE'}
DMG_ID={'Bludgeoning':'ID_INTERNAL_DAMAGE_TYPE_BLUDGEONING','Piercing':'ID_INTERNAL_DAMAGE_TYPE_PIERCING','Slashing':'ID_INTERNAL_DAMAGE_TYPE_SLASHING'}
CAT_ID={'Simple Melee':'ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_MELEE','Simple Ranged':'ID_INTERNAL_WEAPON_CATEGORY_SIMPLE_RANGED','Martial Melee':'ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_MELEE','Martial Ranged':'ID_INTERNAL_WEAPON_CATEGORY_MARTIAL_RANGED'}
CAT_PROF={'Simple':'ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS','Martial':'ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_WEAPONS'}
GROUP={'Longsword':'SWORDS','Greatsword':'SWORDS','Shortsword':'SWORDS','Scimitar':'SWORDS','Rapier':'SWORDS','Battleaxe':'AXES','Greataxe':'AXES','Handaxe':'AXES',
 'Mace':'MACES','Morningstar':'MACES','Flail':'MACES','Warhammer':'HAMMERS','Light Hammer':'HAMMERS','Maul':'MAULS','Shortbow':'BOWS','Longbow':'BOWS',
 'Light Crossbow':'CROSSBOWS','Hand Crossbow':'CROSSBOWS','Heavy Crossbow':'CROSSBOWS','Spear':'SPEARS','Javelin':'SPEARS','Club':'CLUBS','Greatclub':'CLUBS'}
def cost_setter(txt):
    m=re.match(r'^([\d,]+)\s*(CP|SP|EP|GP|PP)$',txt.strip())
    if not m: return None
    return setter('cost',m.group(1).replace(',',''),currency=m.group(2).lower())
def weight_setter(txt):
    t=txt.strip()
    if t in('—','','Varies'): return None
    m=re.match(r'^(\d+)?\s*(½|1/2|1/4)?\s*lb\.?(?: \(full\))?$',t)
    lb={'½':0.5,'1/2':0.5,'1/4':0.25}
    if not m: return None
    v=(int(m.group(1)) if m.group(1) else 0)+(lb[m.group(2)] if m.group(2) else 0)
    return setter('weight','%s lb.'%t.replace(' lb.','').replace(' lb',''),lb=('%g'%v))
pt=page_layout(91)
rows=[];cat=None;prev=None
for ln in pt.split('\n'):
    s=ln.strip()
    m=re.match(r'^(Simple|Martial) (Melee|Ranged) Weapons$',s)
    if m: cat=m.group(1)+' '+m.group(2); continue
    m=re.match(r'^(\S.*?)\s{2,}(1d\d+|1d12|2d6|1) (Bludgeoning|Piercing|Slashing)\s{2,}(.*?)\s{2,}(Cleave|Graze|Nick|Push|Sap|Slow|Topple|Vex)\s+(—|[\d/]+ lb\.)\s+([\d,]+ [CSGEP]P)$',s)
    if not m:
        m2=re.match(r'^(Heavy Crossbow) (1d10) (Piercing)\s{2,}(.*?)\s{2,}(Push)\s+(18 lb\.)\s+(50 GP)$',s)
        m=m2
    if m and cat:
        rows.append(dict(name=m.group(1).strip(),dice=m.group(2),dtype=m.group(3),props=m.group(4),mastery=m.group(5),weight=m.group(6),cost=m.group(7),cat=cat)); prev=rows[-1]; continue
    if prev and cat and s and not s.startswith(('Name','Weapons','91 ')) and re.match(r'^\S',ln.lstrip()) and ln.startswith(' '*30):
        prev['props']+=' '+s
WEAPONS=rows
assert len(rows)==38,len(rows)
RANGE_RX=re.compile(r'(Ammunition|Thrown) \(Range (\d+)/(\d+)(?:; (\w+))?\)')
def weapon_el(r):
    name=r['name']; props=r['props'].replace('—','').strip()
    plist=[]
    ver=None; rng=None; ammo=None
    for part in re.split(r',\s*(?![^()]*\))',props):
        part=part.strip()
        if not part: continue
        m=re.match(r'^(Ammunition|Thrown) \(Range (\d+)/(\d+)(?:; (\w+))?\)$',part)
        if m:
            plist.append(m.group(1)); rng='%s/%s'%(m.group(2),m.group(3)); ammo=m.group(4); continue
        m=re.match(r'^Versatile \((.*?)\)$',part)
        if m: plist.append('Versatile'); ver=m.group(1); continue
        m=re.match(r'^Two-Handed( \(unless mounted\))?$',part)
        if m: plist.append('Two-Handed'); continue
        assert part in PROP_ID,(name,part)
        plist.append(part)
    cat=r['cat']; melee=cat.endswith('Melee')
    sup=[CAT_ID[cat],DMG_ID[r['dtype']]]+[PROP_ID[p] for p in plist]
    if rng: sup.append('ID_INTERNAL_WEAPON_PROPERTY_RANGE')
    if name in GROUP: sup.append('ID_INTERNAL_WEAPON_GROUP_'+GROUP[name])
    sup.append(MAST_ID[r['mastery']])
    seen=[];[seen.append(x) for x in sup if x not in seen]
    prof_key='ID_PROFICIENCY_WEAPON_PROFICIENCY_'+{'Light Crossbow':'CROSSBOW_LIGHT','Hand Crossbow':'CROSSBOW_HAND','Heavy Crossbow':'CROSSBOW_HEAVY'}.get(name,slug(name))
    prof=prof_key if prof_key in IDS2014 else CAT_PROF[cat.split()[0]]
    slot='twohand' if 'Two-Handed' in plist and 'Versatile' not in plist else 'onehand'
    st=[setter('category','Weapons'),cost_setter(r['cost']),weight_setter(r['weight']) or setter('weight','—'),setter('slot',slot),
        setter('damage',r['dice'],type=r['dtype'].lower()),setter('proficiency',prof),setter('mastery',r['mastery'].lower())]
    if ver: st.insert(5,setter('versatile',ver))
    if rng: st.insert(5,setter('range',rng))
    st=[s for s in st if s]
    wid=idof('WEAPON',name)
    e=el(wid,name,'Weapon','',supports=seen,setters=st,
         data={'weapon_category':cat,'melee':melee,'damage':{'dice':r['dice'],'type':r['dtype'].lower()},'properties':plist,'versatile_damage':ver,
               'range':rng,'ammunition_type':ammo,'mastery':r['mastery'].lower(),'mastery_id':MAST_ID[r['mastery']],'two_handed_unless_mounted':'unless mounted' in r['props']})
    old='ID_WOTC_PHB_WEAPON_'+{'Light Hammer':'LIGHTHAMMER','Light Crossbow':'CROSSBOW_LIGHT','Hand Crossbow':'CROSSBOW_HAND','Heavy Crossbow':'CROSSBOW_HEAVY'}.get(name,slug(name))
    if old in IDS2014: e['maps_to']={'dnd2014':old,'kind':'equivalent'}
    return e
for r in WEAPONS: emit('equipment/weapons.json',weapon_el(r))

# ---------- armor
ap=page_layout(92)
ARMOR=[];grp=None
for ln in ap.split('\n'):
    s=ln.strip()
    m=re.match(r'^(Light|Medium|Heavy) Armor \((.*)\)$',s)
    if m: grp=m.group(1); donning=m.group(2); continue
    m=re.match(r'^Shield \((.*)\)$',s)
    if m: grp='Shield'; donning=m.group(1); continue
    m=re.match(r'^(\S.*?)\s{2,}(\d+(?: \+ Dex modifier(?: \(max 2\))?)?|\+2)\s{2,}(—|Str \d+)\s{2,}(—|Disadvantage)\s+(\d+ lb\.)\s+([\d,]+ GP)$',s)
    if m and grp:
        ARMOR.append(dict(name=m.group(1),ac=m.group(2),str=m.group(3),stealth=m.group(4),weight=m.group(5),cost=m.group(6),grp=grp,don=donning))
assert len(ARMOR)==13,len(ARMOR)
ARMOR_PROF={'Padded Armor':'PADDED','Leather Armor':'LEATHER','Studded Leather Armor':'STUDDED_LEATHER','Hide Armor':'HIDE','Chain Shirt':'CHAIN_SHIRT','Scale Mail':'SCALE_MAIL','Breastplate':'BREASTPLATE','Half Plate Armor':'HALF_PLATE','Ring Mail':'RING_MAIL','Chain Mail':'CHAIN_MAIL','Splint Armor':'SPLINT','Plate Armor':'PLATE','Shield':'SHIELD'}
def armor_el(a):
    name=a['name']; g=a['grp']
    base=int(re.match(r'\d+',a['ac']).group(0)) if g!='Shield' else 2
    dex='Dex' in a['ac']; cap=2 if 'max 2' in a['ac'] else None
    strv=int(a['str'].split()[1]) if a['str']!='—' else None
    sup=['ID_INTERNAL_ARMOR_GROUP_'+g.upper()]
    st=[setter('category','Armor'),cost_setter(a['cost']),weight_setter(a['weight']),setter('slot','body' if g!='Shield' else 'secondary'),
        setter('armor',g),setter('armorClass',a['ac'].replace('+ Dex modifier','+ Dex')),
        setter('proficiency','ID_PROFICIENCY_ARMOR_PROFICIENCY_'+ARMOR_PROF[name])]
    if strv: st.append(setter('strength',strv))
    st.append(setter('stealth','Disadvantage' if a['stealth']!='—' else '—'))
    rules=[]
    if g=='Shield':
        rules.append(stat('ac:shield','2',bonus='shield'))
    else:
        if a['stealth']!='—': rules.append(grant('ID_INTERNAL_GRANTS_STEALTH_DISADVANTAGE','Grants'))
        if dex:
            rules.append(stat('ac:armored:armor',str(base)))
            rules.append(stat('ac:armored:dexterity','dexterity:modifier'))
            if cap: rules.append(stat('ac:armored:dexterity:cap','2'))
        else:
            rules.append(stat('ac:armored:armor',str(base)))
            rules.append(stat('ac:armored:dexterity','0'))
        if strv: rules.append(stat('speed','-10',bonus='armor',requirements='![str:%d]'%strv))
    old={'Padded Armor':'ID_WOTC_ARMOR_LIGHT_PADDED','Chain Mail':'ID_WOTC_ARMOR_HEAVY_CHAIN_MAIL'}.get(name)
    # find 2014 armor id by name
    cand=[k for k,(n,t) in IDS2014.items() if t=='Armor' and norm_name(n).replace('armor','')==norm_name(name).replace('armor','')]
    e=el(idof('ARMOR',name),name,'Armor','',supports=sup,setters=[s for s in st if s],rules=rules,
         data={'armor_category':g,'base_ac':base,'dex_modifier':dex,'dex_cap':cap,'strength_requirement':strv,'stealth_disadvantage':a['stealth']!='—','don_doff':a['don']})
    if cand: e['maps_to']={'dnd2014':cand[0],'kind':'equivalent'}
    return e
for a in ARMOR: emit('equipment/armor.json',armor_el(a))
print(len(WEAPONS),len(ARMOR))

# ---------- tools
tt=open('tools.txt').read()
TOOLS=[]
tl=tt.split('\n')
i=0
tool_hdr=re.compile(r'^([A-Z][A-Za-z’\' ]+?) \((\d+ [CSGEP]P|Varies)\)$')
cur=None
sect=None
for ln in tl:
    if ln.strip() in('Artisan’s Tools','Other Tools'): sect=ln.strip(); 
    m=tool_hdr.match(ln.strip())
    if m and m.group(1) not in('Adventuring Gear',):
        if cur: TOOLS.append(cur)
        cur={'name':m.group(1),'cost':m.group(2),'lines':[],'sect':sect}; continue
    if ln.strip()=='Adventuring Gear': break
    if cur is not None: cur['lines'].append(ln)
if cur: TOOLS.append(cur)
TOOLS=[t for t in TOOLS if any(l.startswith('Ability:') for l in t['lines'])]
assert len(TOOLS)==25,[t['name'] for t in TOOLS]
def tool_fields(t):
    body='\n'.join(t['lines'])
    # remove stray blank/page garbage
    ls=[l for l in t['lines']]
    fields={};cur=None
    for l in ls:
        m=re.match(r'^(Ability|Utilize|Craft|Variants): ?(.*)$',l.strip())
        if m and not l.startswith(' '):
            cur=m.group(1);fields[cur]=m.group(2);continue
        if cur and l.strip() and (l.startswith(' ') or True):
            fields[cur]=join_lines([fields[cur],l])
    return fields
TOOL_PROF={'Alchemist’s Supplies':'ALCHEMISTS_SUPPLIES','Brewer’s Supplies':'BREWERS_SUPPLIES','Calligrapher’s Supplies':'CALLIGRAPHERS_SUPPLIES','Carpenter’s Tools':'CARPENTERS_TOOLS',
 'Cartographer’s Tools':'CARTOGRAPHERS_TOOLS','Cobbler’s Tools':'COBBLERS_TOOLS','Cook’s Utensils':'COOKS_UTENSILS','Glassblower’s Tools':'GLASSBLOWERS_TOOLS','Jeweler’s Tools':'JEWELERS_TOOLS',
 'Leatherworker’s Tools':'LEATHERWORKERS_TOOLS','Mason’s Tools':'MASONS_TOOLS','Painter’s Supplies':'PAINTERS_SUPPLIES','Potter’s Tools':'POTTERS_TOOLS','Smith’s Tools':'SMITHS_TOOLS',
 'Tinker’s Tools':'TINKERS_TOOLS','Weaver’s Tools':'WEAVERS_TOOLS','Woodcarver’s Tools':'WOODCARVERS_TOOLS','Disguise Kit':'DISGUISE_KIT','Forgery Kit':'FORGERY_KIT','Gaming Set':'GAMING_SET',
 'Herbalism Kit':'HERBALISM_KIT','Musical Instrument':'MUSICAL_INSTRUMENT','Navigator’s Tools':'NAVIGATORS_TOOLS','Poisoner’s Kit':'POISONERS_KIT','Thieves’ Tools':'THIEVES_TOOLS'}
def prof_id_for(n):
    for pre in ('ID_PROFICIENCY_TOOL_PROFICIENCY_','ID_PROFICIENCY_GAMING_SET_PROFICIENCY_'):
        k=pre+TOOL_PROF[n]
        if k in IDS2014: return k
    return 'ID_PROFICIENCY_TOOL_PROFICIENCY_'+TOOL_PROF[n]
TOOL_ID={}
for t in TOOLS:
    f=tool_fields(t); n=t['name']
    util=[]
    if 'Utilize' in f:
        for part in re.split(r',\s*or\s+|,\s*(?=or )|\s+or\s+(?=[A-Z])',f['Utilize']):
            part=part.strip().rstrip('.')
            m=re.match(r'^(.*?)\s*\(DC (\d+)\)$',part)
            if m: util.append({'action':(lambda a:a[:1].upper()+a[1:])(m.group(1).strip()),'dc':int(m.group(2))})
            elif part: util.append({'action':part})
    crafts=[c.strip() for c in re.split(r',\s*(?![^()]*\))',f['Craft'])] if 'Craft' in f else []
    crafts=[c for c in crafts if c]
    tid=idof('TOOL',n); TOOL_ID[n]=tid
    abil=f['Ability'].split('Weight')[0].strip()
    wt=f['Ability'].split('Weight:')[1].strip() if 'Weight:' in f['Ability'] else '—'
    desc='<p><b>Ability:</b> %s</p>'%esc(abil)
    if 'Utilize' in f: desc+='\n<p><b>Utilize:</b> %s</p>'%esc(f['Utilize'])
    if 'Craft' in f: desc+='\n<p><b>Craft:</b> %s</p>'%esc(f['Craft'])
    if 'Variants' in f: desc+='\n<p><b>Variants:</b> %s</p>'%esc(f['Variants'])
    st=[setter('category','Tools'),setter('type','Tool')]
    c=cost_setter(t['cost']) if t['cost']!='Varies' else None
    if c: st.append(c)
    w=weight_setter(wt)
    if w: st.append(w)
    else: st.append(setter('weight',wt))
    st.append(setter('proficiency',prof_id_for(n)))
    sup=['ID_INTERNAL_TOOL_PROPERTY_'+('ARTISAN' if t['sect'].startswith('Artisan') else {'Gaming Set':'GAMING_SET','Musical Instrument':'MUSICAL_INSTRUMENT','Thieves’ Tools':'THIEVES_TOOLS'}.get(n,'KIT'))]
    e=el(tid,n,'Item',desc,supports=sup,setters=st,
         data={'tool':True,'ability':abil.lower(),'utilize':util,'craft':crafts,'variants':f.get('Variants'),'tool_group':'artisan' if t['sect'].startswith('Artisan') else 'other'})
    old=[k for k,(nn,tt2) in IDS2014.items() if k.startswith('ID_WOTC_PHB_ITEM_TOOL_') and norm_name(nn)==norm_name(n)]
    if old: e['maps_to']={'dnd2014':old[0],'kind':'equivalent'}
    emit('equipment/tools.json',e)
print('tools',len(TOOLS))

# ---------- adventuring gear
gp=page_layout(95)
GEAR=[]
W=r'(—|Varies|[\d½]*\s?[\d/]*\s?lb\.(?: \(full\))?|[\d½/]+ lb\.)'
rowrx=re.compile(r'^\s*(\S.*?)\s{2,}(—|Varies|(?:\d+½?|½|1/2|1/4)(?: lb\.)(?: \(full\))?)\s{2,}(Varies|[\d,]+ [CSGEP]P)\s*$')
for ln in gp.split('\n'):
    if len(ln)<20: continue
    for part in (ln[:56],ln[56:]):
        m=rowrx.match(part)
        if m and m.group(1) not in('Item',):
            GEAR.append((m.group(1).strip(),m.group(2),m.group(3)))
print('gear rows',len(GEAR))
gt=open('gear.txt').read()
gt_start=gt.index('Acid (25 GP)')
gt_end=gt.index('Mounts and Vehicles\nA mount can help')
gear_text=gt[gt_start:gt_end].split('\n')
hdrs=[]
for i,l in enumerate(gear_text):
    m=re.match(r'^([A-Z][A-Za-z’\', ]+?) \((Varies|\d[\d,]* [CSGEP]P)\)$',l)
    if m: hdrs.append((i,m.group(1),m.group(2)))
    elif l.startswith('Spell Scroll (Cantrip, 30 GP;'): hdrs.append((i,'Spell Scroll','Varies'))
GEAR_DESC={}
for k,(i,n,c) in enumerate(hdrs):
    j=hdrs[k+1][0] if k+1<len(hdrs) else len(gear_text)
    body=gear_text[i+1:j]
    if n=='Spell Scroll': body=body[1:]
    GEAR_DESC[n]=body
def gear_name_key(n): return norm_name(n)
GEAR_ID={}
PACKS={}
for (n,w,c) in GEAR:
    base_name=n
    gid=idof('ITEM',n); GEAR_ID[gear_name_key(n)]=gid
for (n,w,c) in GEAR:
    name=n
    dkey=n
    if n.startswith('Spell Scroll'): dkey='Spell Scroll'
    body=GEAR_DESC.get(dkey)
    if body is None and n=='Ammunition': body=GEAR_DESC.get('Ammunition')
    desc=chunk_html2(body) if body else ''
    cat='Equipment Packs' if n.endswith('Pack') else 'Adventuring Gear'
    if 'Focus' in n or n=='Holy Symbol': cat='Spellcasting Focus'
    st=[setter('category',cat)]
    cs=cost_setter(c)
    if cs: st.append(cs)
    ws=weight_setter(w)
    if ws: st.append(ws)
    elif w!='—': st.append(setter('weight',w))
    e=el(GEAR_ID[gear_name_key(n)],n,'Item',desc,setters=st)
    if n=='Potion of Healing': e['data']={'healing':'2d4+2','crafting':True}
    old=[k for k,(nn,tt2) in IDS2014.items() if tt2=='Item' and norm_name(nn)==norm_name(n)]
    if old: e['maps_to']={'dnd2014':old[0],'kind':'equivalent'}
    if cat=='Equipment Packs':
        txt=' '.join(l.strip() for l in (body or []))
        txt=txt.replace('- ','')
        PACKS[n]=(e,txt)
    emit('equipment/gear.json' if cat!='Equipment Packs' else 'equipment/packs.json',e)
# resolve pack contents
ALIAS={'fineclothes':'clothesfine','travelersclothes':'clothestravelers','hoodedlantern':'lanternhooded','bullseyelantern':'lanternbullseye','mapscrollcase':'casemaporscroll','mapsorscrollcases':'casemaporscroll','mapscase':'casemaporscroll',
 'crossbowboltcase':'casecrossbowbolt','crossbowboltcases':'casecrossbowbolt','glassbottle':'bottleglass','ironspikes':'spikesiron','ironpot':'potiron','basicpoison':'poisonbasic','portableram':'ramportable'}
UNMATCHED=[]
def match_item(nm):
    k=norm_name(nm)
    for cand in (k,k.rstrip('s'),ALIAS.get(k),ALIAS.get(k.rstrip('s'))):
        if cand and cand in GEAR_ID: return GEAR_ID[cand]
    if k.endswith('es') and k[:-2] in GEAR_ID: return GEAR_ID[k[:-2]]
    if k.rstrip('s') in ('maporscrollcase',): return GEAR_ID['casemaporscroll']
    k2=norm_name(re.sub(r'^(flasks?|days?|sheets?|pints?) of ','',nm,flags=re.I))
    for cand in (k2,k2.rstrip('s'),ALIAS.get(k2)):
        if cand and cand in GEAR_ID: return GEAR_ID[cand]
    # tools
    for tn,tid in TOOL_ID.items():
        if norm_name(tn)==k: return tid
    return None
for pn,(e,txt) in PACKS.items():
    m=re.search(r'following items:\s*(.*?)(?:\.\s|\.$)',txt)
    contents=m.group(1) if m else ''
    items=[]
    for part in re.split(r',\s*(?:and\s+)?|\s+and\s+(?=\d|[A-Z])',contents):
        part=part.strip().rstrip('.')
        if not part: continue
        mm=re.match(r'^(\d+)\s+(.*)$',part)
        amt=int(mm.group(1)) if mm else 1
        nm=mm.group(2) if mm else part
        nm=nm.replace('(','').replace(')','')
        iid=match_item(nm)
        if iid is None: UNMATCHED.append((pn,nm)); continue
        items.append({'id':iid,'amount':amt} if amt>1 else {'id':iid})
    e['extract']={'items':items}
    if contents: e['data']={'contents_text':contents}
print('unmatched pack items',UNMATCHED)

# ---------- ammunition (table on p96)
for ln in page_layout(96).split('\n'):
    m=re.match(r'^\s*(Arrows|Bolts|Bullets, Firearm|Bullets, Sling|Needles)\s+(\d+)\s+(Quiver|Case|Pouch)\s+(\d+½? lb\.|1½ lb\.|1 lb\.|2 lb\.)\s+(\d+ [CSGEP]P)',ln)
    if m:
        n,amt,stor,wt,cost=m.groups()
        st=[setter('category','Ammunition'),setter('type','Ammunition'),cost_setter(cost),weight_setter(wt)]
        e=el(idof('ITEM','Ammunition',n),n,'Item','<p>Comes in a bundle of %s; stored in a %s.</p>'%(amt,stor),setters=[s for s in st if s],
             data={'ammunition':True,'amount':int(amt),'storage':stor,'bundle_weight':wt,'bundle_cost':cost})
        emit('equipment/gear.json',e)

# ---------- mounts, tack and vehicles (pages 100-101)
def add_rows(path,lines,rx,builder):
    for ln in lines:
        m=re.match(rx,ln)
        if m: emit(path,builder(*[g.strip() if g else g for g in m.groups()]))
p100=page_layout(100).split('\n')
right100=[l[61:] if len(l)>61 else '' for l in p100]
def mount_el(n,cap,cost):
    return el(idof('MOUNT',n),n,'Item','',setters=[setter('category','Mounts and Other Animals'),cost_setter(cost)],data={'carrying_capacity':cap})
add_rows('equipment/mounts-vehicles.json',right100,r'^\s*(Camel|Elephant|Horse, Draft|Horse, Riding|Mastiff|Mule|Pony|Warhorse)\s+([\d,]+ lb\.)\s+([\d,]+ [CSGEP]P)\s*$',mount_el)
def tack_el(n,wt,cost):
    return el(idof('ITEM',n),n,'Item','',setters=[x for x in (setter('category','Tack, Harness, and Drawn Vehicles'),cost_setter(cost),weight_setter(wt)) if x])
add_rows('equipment/mounts-vehicles.json',right100,r'^\s*(Carriage|Cart|Chariot|Feed per day|Sled|Wagon)\s+([\d,]+ lb\.)\s+([\d,]+ [CSGEP]P)\s*$',tack_el)
for sad,wt,cost in re.findall(r'^\s*(Exotic|Military|Riding)\s+(\d+ lb\.)\s+(\d+ [CSGEP]P)\s*$','\n'.join(right100),re.M):
    emit('equipment/mounts-vehicles.json',tack_el(sad+' Saddle',wt,cost))
for ln in right100:
    m=re.match(r'^\s*Stabling per day\s+—\s+(\d+ [CSGEP]P)\s*$',ln)
    if m: emit('equipment/mounts-vehicles.json',el(idof('ITEM','Stabling per day'),'Stabling per day','Item','',setters=[setter('category','Services'),cost_setter(m.group(1))]))
p101=page_layout(101).split('\n')
def ship_el(n,speed,crew,pas,cargo,ac,hp,dt,cost):
    return el(idof('VEHICLE',n),n,'Item','',setters=[x for x in (setter('category','Airborne and Waterborne Vehicles'),cost_setter(cost)) if x],
              data={'speed':speed,'crew':crew,'passengers':pas,'cargo_tons':cargo,'ac':int(ac),'hp':int(hp),'damage_threshold':dt})
add_rows('equipment/mounts-vehicles.json',p101,r'^\s*(Airship|Galley|Keelboat|Longship|Rowboat|Sailing Ship|Warship)\s+(\S+ mph)\s+(\d+)\s+(\d+|—)\s+(\S+)\s+(\d+)\s+(\d+)\s+(\d+|—)\s+([\d,]+ [CSGEP]P)\s*$',ship_el)
# ---------- lifestyle expenses, services (text)
t100=text(100,102)
for m in re.finditer(r'^(Wretched|Squalid|Poor|Modest|Comfortable|Wealthy|Aristocratic) \((Free|[\d]+ [CSGEP]P per Day)\)\n(.*?)(?=^(?:Wretched|Squalid|Poor|Modest|Comfortable|Wealthy|Aristocratic) \(|^Food, Drink|\Z)',t100,re.M|re.S):
    n,c,body=m.groups()
    cs=cost_setter(c.replace(' per Day','')) if c!='Free' else None
    emit('equipment/services.json',el(idof('LIFESTYLE',n),n+' Lifestyle','Item',chunk_html2(body.split('\n')[:12]) if False else '<p>%s</p>'%esc(join_lines([l for l in body.split('\n')][:8]).split('Food, Drink')[0]),
        setters=[x for x in (setter('category','Lifestyle Expenses'),cs) if x],data={'cost_text':c}))
HIRE=[('Skilled hireling','2 GP per day'),('Untrained hireling','2 SP per day'),('Messenger','2 CP per mile')]
for n,c in HIRE:
    m=re.match(r'(\d+) ([CSGEP]P) per (\w+)',c)
    emit('equipment/services.json',el(idof('HIRELING',n),n,'Item','',setters=[setter('category','Hirelings'),setter('cost',m.group(1),currency=m.group(2).lower(),per=m.group(3))],data={'cost_text':c}))
SERVICES=[('Cantrip','Village, town, or city','30 GP'),('1','Village, town, or city','50 GP'),('2','Village, town, or city','200 GP'),('3','Town or city only','300 GP'),('4–5','Town or city only','2,000 GP'),('6–8','City only','20,000 GP'),('9','City only','100,000 GP')]
for lv,avail,c in SERVICES:
    n='Spellcasting Service ('+('Cantrip' if lv=='Cantrip' else 'Level '+lv)+')'
    emit('equipment/services.json',el(idof('SERVICE','Spellcasting',lv.replace('–','_')),n,'Item','<p>Availability: %s.</p>'%avail,setters=[setter('category','Spellcasting Services'),cost_setter(c)],data={'availability':avail,'spell_level':lv}))
# food, drink and lodging
FOOD=[('Ale (mug)','4 CP'),('Bread (loaf)','2 CP'),('Cheese (wedge)','1 SP'),('Wine, Common (bottle)','2 SP'),('Wine, Fine (bottle)','10 GP')]
INN=[('Squalid','7 CP','1 CP'),('Poor','1 SP','2 CP'),('Modest','5 SP','1 SP'),('Comfortable','8 SP','2 SP'),('Wealthy','2 GP','3 SP'),('Aristocratic','4 GP','6 SP')]
for n,c in FOOD:
    emit('equipment/services.json',el(idof('FOOD',n),n,'Item','',setters=[setter('category','Food, Drink, and Lodging'),cost_setter(c)]))
for q,inn,meal in INN:
    emit('equipment/services.json',el(idof('LODGING','Inn Stay',q),'Inn Stay per Day ('+q+')','Item','',setters=[setter('category','Food, Drink, and Lodging'),cost_setter(inn)]) if False else el(idof('LODGING','Inn Stay',q),'Inn Stay per Day ('+q+')','Item','',setters=[setter('category','Food, Drink, and Lodging'),cost_setter(inn)]))
    emit('equipment/services.json',el(idof('MEAL',q),'Meal ('+q+')','Item','',setters=[setter('category','Food, Drink, and Lodging'),cost_setter(meal)]))

for p,l in OUT.items(): print(p,len(l))
