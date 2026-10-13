import sys; sys.path.insert(0,'.')
from lib import *
import gen_equipment as GE
OUT.update({k:v for k,v in GE.OUT.items()})
ori=open('origins.txt').read()

# ---- name->element index of equipment for equipment packages
ITEMNAME={}
for p,els in GE.OUT.items():
    for e in els:
        ITEMNAME.setdefault(norm_name(e['name']),e['id'])
ITEM_ALIAS={'holysymbol':'holysymbol','parchment':'parchment','quiver':'quiver','arrows':'arrowsammunition','shortbow':'shortbow'}
def ref(name):
    n=name.strip()
    n=re.sub(r'\s*\(.*?\)\s*','',n) if not n.startswith('Gaming Set') else n
    k=norm_name(n)
    k={'travelersclothes':'clothestravelers','fineclothes':'clothesfine'}.get(k,k)
    cands=[k,k.rstrip('s'),k+'s']
    if k.endswith('ies'): cands.append(k[:-3]+'y')
    if k.endswith('es'): cands.append(k[:-2])
    for c in cands:
        if c in ITEMNAME: return ITEMNAME[c]
    for c in cands:  # ammunition names
        for key,val in ITEMNAME.items():
            if val.startswith('ID_2024_ITEM_AMMUNITION_') and (key.endswith(c) or c.endswith(key)): return val
    return None

# ---- Source element
emit('base/source.json', el('ID_2024_SOURCE_SRD521','SRD 5.2.1','Source','<p>System Reference Document 5.2.1, published by Wizards of the Coast LLC under CC-BY-4.0.</p>',
     setters=[setter('url','https://www.dndbeyond.com/srd'),setter('abbreviation','SRD'),setter('official','true'),setter('core','false'),setter('release','20250501')]))

# ---- languages (2024 tables)
STD=['Common','Common Sign Language','Draconic','Dwarvish','Elvish','Giant','Gnomish','Goblin','Halfling','Orc']
RARE=['Abyssal','Celestial','Deep Speech','Druidic','Infernal','Primordial','Sylvan','Thieves’ Cant','Undercommon']
LANG_ID={}
for n in STD+RARE:
    cid='ID_LANGUAGE_'+slug(n).replace('THIEVES_CANT','THIEVES_CANT')
    lid=idof('LANGUAGE',n); LANG_ID[n]=lid
    st=[setter('standard' if n in STD else 'exotic','true')]
    if n=='Primordial': st.append(setter('script','Dwarvish'))
    e=el(lid,n,'Language','<p>%s language (%s).%s</p>'%(n,'standard' if n in STD else 'rare','  Primordial includes the Aquan, Auran, Ignan, and Terran dialects.' if n=='Primordial' else ''),
         setters=st,supports=['Standard' if n in STD else 'Rare'],data={'rarity':'standard' if n in STD else 'rare'})
    core_id={'Thieves’ Cant':'ID_WOTC_PHB_LANGUAGE_THIEVES_CANT'}.get(n,cid)
    if core_id in IDS2014: e['maps_to']={'dnd2014':core_id,'kind':'equivalent'}
    emit('base/languages.json',e)

# ---- species text extraction
SPECIES=['Dragonborn','Dwarf','Elf','Gnome','Goliath','Halfling','Human','Orc','Tiefling']
def species_block(n):
    m=re.search(r'^%s\nCreature Type:'%n,ori,re.M)
    a=m.start()
    others=[re.search(r'^%s\nCreature Type:'%o,ori,re.M).start() for o in SPECIES if o!=n]
    b=min([x for x in others if x>a]+[ori.index('\n                    Feats\n') if '\n                    Feats\n' in ori else len(ori)])
    return ori[a:b]
def split_traits(block,names):
    lines=block.split('\n')
    # find trait starts
    starts=[]
    for i,l in enumerate(lines):
        for nm in names:
            if l.lstrip().startswith(nm+'. ') and (len(l)-len(l.lstrip()))>=1 or (l.startswith(nm+'. ') and i>3):
                if not any(s[1]==nm for s in starts): starts.append((i,nm))
    starts.sort()
    out={}
    for k,(i,nm) in enumerate(starts):
        j=starts[k+1][0] if k+1<len(starts) else len(lines)
        seg=lines[i:j]
        seg[0]=seg[0].lstrip()[len(nm)+2:]
        out[nm]=seg
    return out
def tbl(rows,header):
    h=''.join('<th>%s</th>'%esc(c) for c in header)
    b=''.join('<tr>'+''.join('<td>%s</td>'%esc(c) for c in r)+'</tr>' for r in rows)
    return '<table><tr>%s</tr>%s</table>'%(h,b)
DMG_RES=lambda t:'ID_INTERNAL_CONDITION_DAMAGE_RESISTANCE_'+t.upper()
def trait(sp,name,body_lines,rules=None,extra_html='',**data):
    tid=idof('TRAIT',sp,name)
    body=chunk_html2(body_lines) if body_lines else ''
    e=el(tid,name,'Racial Trait',body+extra_html,rules=rules or [],data=data or None,
         sheet={'display':False} if False else None)
    return e
def put(sp,e): emit('species/%s.json'%sp.lower(),e)

def make_species(sp,size,speed,rules,traits,extra_elems=(),size_choice=None,mapsto=None,height=None):
    block=species_block(sp)
    sid=idof('SPECIES',sp)
    trait_ids=[t['id'] for t in traits]
    srules=[grant('ID_SIZE_'+(size.upper() if not size_choice else 'MEDIUM'),'Size')] if not size_choice else [custom('select_size',name='Size',options=['Small','Medium'])]
    srules.append(stat('innate speed',speed,bonus='base'))
    srules+=rules
    srules+=[grant(t,'Racial Trait') for t in trait_ids]
    m=re.search(r'Size: (.*?)\nSpeed',block,re.S)
    sizetxt=join_lines(m.group(1).split('\n'))
    e=el(sid,sp,'Species',
         '<p>Creature type: Humanoid. Size: %s. Speed: %s feet.</p>'%(esc(sizetxt),speed),
         rules=srules,setters=[setter('short','Humanoid, %s, Speed %s ft.'%(sizetxt.split(' (')[0],speed))],
         data={'creature_type':'Humanoid','size':size_choice or [size],'speed':int(speed),'traits':trait_ids})
    if mapsto: e['maps_to']={'dnd2014':mapsto,'kind':'approximate'}
    emit('species/%s.json'%sp.lower(),e)
    for t in traits: emit('species/%s.json'%sp.lower(),t)
    for x in extra_elems: emit('species/%s.json'%sp.lower(),x)
    return e

def dv(r): return [grant('ID_VISION_DARKVISION','Vision'),stat('darkvision:range',r,bonus='base')]

# ================= Dragonborn
def sblock(sp,names): return split_traits(species_block(sp),names)
tb=sblock('Dragonborn',['Draconic Ancestry','Breath Weapon','Damage Resistance','Darkvision','Draconic Flight'])
DRAGONS=[('Black','Acid'),('Blue','Lightning'),('Brass','Fire'),('Bronze','Lightning'),('Copper','Acid'),('Gold','Fire'),('Green','Poison'),('Red','Fire'),('Silver','Cold'),('White','Cold')]
anc_opts=[]
for d,t in DRAGONS:
    oid=idof('OPTION','Draconic Ancestry',d)
    anc_opts.append(el(oid,d+' Dragon','Species Trait Option','<p>Draconic ancestry: %s dragon. Your Breath Weapon and Damage Resistance use %s damage.</p>'%(d,t),
        supports=['Draconic Ancestry'],rules=[grant(DMG_RES(t),'Condition'),stat('draconic ancestry damage type',t,inline=True)],data={'dragon':d,'damage_type':t.lower()}))
pb_res=custom('resource',id='breath weapon',name='Breath Weapon',max={'type':'stat','stat':'proficiency'},recharge=['long_rest'])
traits=[
 trait('Dragonborn','Draconic Ancestry',tb['Draconic Ancestry'],[select('Species Trait Option','Draconic Ancestry',supports='Draconic Ancestry')],
       extra_html=tbl([[d,t] for d,t in DRAGONS],['Dragon','Damage Type'])),
 trait('Dragonborn','Breath Weapon',tb['Breath Weapon'],[pb_res,custom('damage_scaling',name='Breath Weapon',dice='1d10',levels={'1':'1d10','5':'2d10','11':'3d10','17':'4d10'},scale='character_level',type_from='draconic ancestry damage type')],
       save='dexterity',dc='8+constitution+proficiency',area=['15-foot Cone','30-foot Line (5 feet wide)']),
 trait('Dragonborn','Damage Resistance',tb['Damage Resistance'],[]),
 trait('Dragonborn','Darkvision',tb['Darkvision'],dv(60)),
 trait('Dragonborn','Draconic Flight',tb['Draconic Flight'],[custom('resource',level=5,id='draconic flight',name='Draconic Flight',max={'type':'fixed','value':1},recharge=['long_rest']),
       custom('movement_mode',level=5,mode='fly',speed='speed',duration='10 minutes',action='bonus_action',requires_resource='draconic flight')])]
make_species('Dragonborn','Medium','30',[],traits,anc_opts,mapsto='ID_RACE_DRAGONBORN')

# ================= Dwarf
tb=sblock('Dwarf',['Darkvision','Dwarven Resilience','Dwarven Toughness','Stonecunning'])
traits=[
 trait('Dwarf','Darkvision',tb['Darkvision'],dv(120)),
 trait('Dwarf','Dwarven Resilience',tb['Dwarven Resilience'],[grant(DMG_RES('Poison'),'Condition'),custom('save_advantage',against_condition='poisoned',scope='avoid_or_end')]),
 trait('Dwarf','Dwarven Toughness',tb['Dwarven Toughness'],[stat('hp','1',level=l,alt='Dwarven Toughness') for l in range(1,21)]),
 trait('Dwarf','Stonecunning',tb['Stonecunning'],[custom('resource',id='stonecunning',name='Stonecunning',max={'type':'stat','stat':'proficiency'},recharge=['long_rest'])],sense='tremorsense 60 ft. for 10 minutes')]
make_species('Dwarf','Medium','30',[],traits,mapsto='ID_SRD_RACE_DWARF')

# ================= Elf
tb=sblock('Elf',['Darkvision','Elven Lineage','Fey Ancestry','Keen Senses','Trance'])
def lineage_opt(sp,group,name,l1_rules,l1_text,spells3,spells5,cantrips):
    oid=idof('OPTION',group,name)
    rules=list(l1_rules)
    for c in cantrips:
        rules.append(custom('grant_spell',spell=idof('SPELL',c),mode='known_cantrip',ability={'choice':['intelligence','wisdom','charisma'],'param':'Lineage Spellcasting Ability'}))
    rules.append(custom('grant_spell',level=3,spell=idof('SPELL',spells3),mode='always_prepared',free_cast={'uses':1,'recharge':'long_rest'},also_with_slots=True,ability={'param':'Lineage Spellcasting Ability'}))
    rules.append(custom('grant_spell',level=5,spell=idof('SPELL',spells5),mode='always_prepared',free_cast={'uses':1,'recharge':'long_rest'},also_with_slots=True,ability={'param':'Lineage Spellcasting Ability'}))
    return el(oid,name,'Species Trait Option','<p>%s Level 3 spell: %s. Level 5 spell: %s.</p>'%(esc(l1_text),spells3,spells5),supports=[group],rules=rules,
              data={'lineage':name,'level_1_text':l1_text,'level_3_spell':spells3,'level_5_spell':spells5,'level_1_cantrips':cantrips})
elf_opts=[
 lineage_opt('Elf','Elven Lineage','Drow',dv(120),'The range of your Darkvision increases to 120 feet. You also know the Dancing Lights cantrip.','Faerie Fire','Darkness',['Dancing Lights']),
 lineage_opt('Elf','Elven Lineage','High Elf',[custom('cantrip_swap',spell_list='Wizard',swap_on='long_rest',replaces='Prestidigitation')],'You know the Prestidigitation cantrip. Whenever you finish a Long Rest, you can replace that cantrip with a different cantrip from the Wizard spell list.','Detect Magic','Misty Step',['Prestidigitation']),
 lineage_opt('Elf','Elven Lineage','Wood Elf',[stat('innate speed','35',bonus='base')],'Your Speed increases to 35 feet. You also know the Druidcraft cantrip.','Longstrider','Pass without Trace',['Druidcraft'])]
ln_tbl=tbl([['Drow','The range of your Darkvision increases to 120 feet. You also know the Dancing Lights cantrip.','Faerie Fire','Darkness'],
            ['High Elf','You know the Prestidigitation cantrip. Whenever you finish a Long Rest, you can replace that cantrip with a different cantrip from the Wizard spell list.','Detect Magic','Misty Step'],
            ['Wood Elf','Your Speed increases to 35 feet. You also know the Druidcraft cantrip.','Longstrider','Pass without Trace']],['Lineage','Level 1','Level 3','Level 5'])
traits=[
 trait('Elf','Darkvision',tb['Darkvision'],dv(60)),
 trait('Elf','Elven Lineage',tb['Elven Lineage'],[select('Species Trait Option','Elven Lineage',supports='Elven Lineage'),custom('select_param',name='Lineage Spellcasting Ability',options=['Intelligence','Wisdom','Charisma'])],extra_html=ln_tbl),
 trait('Elf','Fey Ancestry',tb['Fey Ancestry'],[custom('save_advantage',against_condition='charmed',scope='avoid_or_end')]),
 trait('Elf','Keen Senses',tb['Keen Senses'],[select('Proficiency','Keen Senses',supports=skill_range(['Insight','Perception','Survival']))]),
 trait('Elf','Trance',tb['Trance'],[])]
make_species('Elf','Medium','30',[],traits,elf_opts,mapsto='ID_RACE_ELF')

# ================= Gnome
tb=sblock('Gnome',['Darkvision','Gnomish Cunning','Gnomish Lineage'])
gn_lines=tb['Gnomish Lineage']
fg=[l for l in gn_lines]
text_gl='\n'.join(gn_lines)
m_f=text_gl.index('Forest Gnome.'); m_r=text_gl.index('Rock Gnome.')
intro=text_gl[:m_f].split('\n'); forest=text_gl[m_f+len('Forest Gnome. '):m_r].split('\n'); rock=text_gl[m_r+len('Rock Gnome. '):].split('\n')
gn_opts=[
 el(idof('OPTION','Gnomish Lineage','Forest Gnome'),'Forest Gnome','Species Trait Option',chunk_html2(forest),supports=['Gnomish Lineage'],
    rules=[custom('grant_spell',spell=idof('SPELL','Minor Illusion'),mode='known_cantrip',ability={'param':'Lineage Spellcasting Ability'}),
           custom('grant_spell',spell=idof('SPELL','Speak with Animals'),mode='always_prepared',free_cast={'uses':'proficiency','recharge':'long_rest'},also_with_slots=True,ability={'param':'Lineage Spellcasting Ability'})]),
 el(idof('OPTION','Gnomish Lineage','Rock Gnome'),'Rock Gnome','Species Trait Option',chunk_html2(rock),supports=['Gnomish Lineage'],
    rules=[custom('grant_spell',spell=idof('SPELL','Mending'),mode='known_cantrip',ability={'param':'Lineage Spellcasting Ability'}),
           custom('grant_spell',spell=idof('SPELL','Prestidigitation'),mode='known_cantrip',ability={'param':'Lineage Spellcasting Ability'}),
           custom('resource',id='clockwork device',name='Clockwork Device',max={'type':'fixed','value':3},recharge=['none'],kind_note='devices in existence at a time')])]
traits=[
 trait('Gnome','Darkvision',tb['Darkvision'],dv(60)),
 trait('Gnome','Gnomish Cunning',tb['Gnomish Cunning'],[custom('save_advantage',against_ability=['intelligence','wisdom','charisma'])]),
 trait('Gnome','Gnomish Lineage',intro,[select('Species Trait Option','Gnomish Lineage',supports='Gnomish Lineage'),custom('select_param',name='Lineage Spellcasting Ability',options=['Intelligence','Wisdom','Charisma'])])]
make_species('Gnome','Small','30',[],traits,gn_opts,mapsto='ID_RACE_GNOME')

# ================= Goliath
tb=sblock('Goliath',['Giant Ancestry','Large Form','Powerful Build'])
ga='\n'.join(tb['Giant Ancestry'])
GIANTS=[("Cloud’s Jaunt (Cloud Giant)",'Cloud'),("Fire’s Burn (Fire Giant)",'Fire'),("Frost’s Chill (Frost Giant)",'Frost'),("Hill’s Tumble (Hill Giant)",'Hill'),("Stone’s Endurance (Stone Giant)",'Stone'),("Storm’s Thunder (Storm Giant)",'Storm')]
idx=[(ga.index(g+'. '),g) for g,_ in GIANTS]
intro_lines=ga[:idx[0][0]].split('\n')
go_opts=[]
for k,(pos,g) in enumerate(idx):
    end=idx[k+1][0] if k+1<len(idx) else len(ga)
    seg=ga[pos+len(g)+2:end].split('\n')
    short=g.split(' (')[0]
    go_opts.append(el(idof('OPTION','Giant Ancestry',short),g,'Species Trait Option',chunk_html2(seg),supports=['Giant Ancestry'],
        rules=[custom('resource',id='giant ancestry',name=short,max={'type':'stat','stat':'proficiency'},recharge=['long_rest'])]))
traits=[
 trait('Goliath','Giant Ancestry',intro_lines,[select('Species Trait Option','Giant Ancestry',supports='Giant Ancestry')]),
 trait('Goliath','Large Form',tb['Large Form'],[custom('resource',level=5,id='large form',name='Large Form',max={'type':'fixed','value':1},recharge=['long_rest'])]),
 trait('Goliath','Powerful Build',tb['Powerful Build'],[custom('save_advantage_check',context='end Grappled condition'),custom('carry_capacity_size_bonus',steps=1)])]
make_species('Goliath','Medium','35',[],traits,go_opts)

# ================= Halfling
tb=sblock('Halfling',['Brave','Halfling Nimbleness','Luck','Naturally Stealthy'])
traits=[trait('Halfling','Brave',tb['Brave'],[custom('save_advantage',against_condition='frightened',scope='avoid_or_end')]),
        trait('Halfling','Halfling Nimbleness',tb['Halfling Nimbleness'],[]),
        trait('Halfling','Luck',tb['Luck'],[]),
        trait('Halfling','Naturally Stealthy',tb['Naturally Stealthy'],[])]
make_species('Halfling','Small','30',[],traits,mapsto='ID_RACE_HALFLING')

# ================= Human
tb=sblock('Human',['Resourceful','Skillful','Versatile'])
# The source text runs on into the Tiefling's Fiendish Legacies table; keep only Human's own text.
_v='\n'.join(tb['Versatile']).split('Fiendish Legacies')[0].rstrip(); tb['Versatile']=_v.split('\n')
traits=[trait('Human','Resourceful',tb['Resourceful'],[custom('heroic_inspiration',trigger='long_rest')]),
        trait('Human','Skillful',tb['Skillful'],[select('Proficiency','Skillful',supports='Skill')]),
        trait('Human','Versatile',tb['Versatile'],[custom('select_feat',name='Versatile',category='Origin',default='ID_2024_FEAT_SKILLED')])]
make_species('Human','Medium','30',[],traits,size_choice=['Small','Medium'],mapsto='ID_RACE_HUMAN')

# ================= Orc
tb=sblock('Orc',['Adrenaline Rush','Darkvision','Relentless Endurance'])
traits=[trait('Orc','Adrenaline Rush',tb['Adrenaline Rush'],[custom('resource',id='adrenaline rush',name='Adrenaline Rush',max={'type':'stat','stat':'proficiency'},recharge=['short_rest','long_rest'])]),
        trait('Orc','Darkvision',tb['Darkvision'],dv(120)),
        trait('Orc','Relentless Endurance',tb['Relentless Endurance'],[custom('resource',id='relentless endurance',name='Relentless Endurance',max={'type':'fixed','value':1},recharge=['long_rest'])])]
make_species('Orc','Medium','30',[],traits,mapsto='ID_RACE_HALFORC')

# ================= Tiefling
tb=sblock('Tiefling',['Darkvision','Fiendish Legacy','Otherworldly Presence'])
LEG=[('Abyssal','Poison','Poison Spray','Ray of Sickness','Hold Person'),('Chthonic','Necrotic','Chill Touch','False Life','Ray of Enfeeblement'),('Infernal','Fire','Fire Bolt','Hellish Rebuke','Darkness')]
tf_opts=[]
for n,res,can,s3,s5 in LEG:
    tf_opts.append(el(idof('OPTION','Fiendish Legacy',n),n,'Species Trait Option',
        '<p>You have Resistance to %s damage. You also know the %s cantrip. Level 3 spell: %s. Level 5 spell: %s.</p>'%(res,can,s3,s5),supports=['Fiendish Legacy'],
        rules=[grant(DMG_RES(res),'Condition'),
               custom('grant_spell',spell=idof('SPELL',can),mode='known_cantrip',ability={'param':'Lineage Spellcasting Ability'}),
               custom('grant_spell',level=3,spell=idof('SPELL',s3),mode='always_prepared',free_cast={'uses':1,'recharge':'long_rest'},also_with_slots=True,ability={'param':'Lineage Spellcasting Ability'}),
               custom('grant_spell',level=5,spell=idof('SPELL',s5),mode='always_prepared',free_cast={'uses':1,'recharge':'long_rest'},also_with_slots=True,ability={'param':'Lineage Spellcasting Ability'})],
        data={'legacy':n,'resistance':res.lower(),'cantrip':can,'level_3_spell':s3,'level_5_spell':s5}))
fl_tbl=tbl([[n,'Resistance to %s damage; %s cantrip'%(r,c),s3,s5] for n,r,c,s3,s5 in LEG],['Legacy','Level 1','Level 3','Level 5'])
traits=[trait('Tiefling','Darkvision',tb['Darkvision'],dv(60)),
        trait('Tiefling','Fiendish Legacy',tb['Fiendish Legacy'],[select('Species Trait Option','Fiendish Legacy',supports='Fiendish Legacy'),custom('select_param',name='Lineage Spellcasting Ability',options=['Intelligence','Wisdom','Charisma'])],extra_html=fl_tbl),
        trait('Tiefling','Otherworldly Presence',tb['Otherworldly Presence'],[custom('grant_spell',spell=idof('SPELL','Thaumaturgy'),mode='known_cantrip',ability={'param':'Lineage Spellcasting Ability'})])]
make_species('Tiefling','Medium','30',[],traits,tf_opts,size_choice=['Small','Medium'],mapsto='ID_RACE_TIEFLING')
print('species done',{k:len(v) for k,v in OUT.items() if k.startswith('species')})

# ================= equipment package parsing
COIN_RX=re.compile(r'^(\d[\d,]*) (CP|SP|EP|GP|PP)$')
EQ_UNRESOLVED=[]
def parse_package(owner_id,owner_name,text,gaming=False):
    text=join_lines(text.split('\n')) if '\n' in text else text
    text=re.sub(r'^Choose A(?:, B,| or B)?(?: or C)?: ','',text)
    parts=re.split(r';\s*(?:or\s+)?(?=\([ABC]\))',text)
    opts=[]
    for part in parts:
        m=re.match(r'^\(([ABC])\)\s*(.*)$',part.strip().rstrip('.'))
        if not m: continue
        label,body=m.groups()
        body=re.sub(r'^or\s+','',body)
        items=[];coins={};labels=[]
        for it in re.split(r',\s*(?:and\s+)?|\s+and\s+(?=\d)',body):
            it=it.strip()
            if not it: continue
            if it.startswith('and '): it=it[4:]
            cm=COIN_RX.match(it)
            if cm: coins[cm.group(2).lower()]=int(cm.group(1).replace(',','')); continue
            mq=re.match(r'^(\d+)\s+(.*)$',it)
            qty=int(mq.group(1)) if mq else 1
            nm=mq.group(2) if mq else it
            note=re.search(r'\((.*?)\)',nm)
            r=ref(nm)
            ent={'qty':qty}
            if nm.startswith('Gaming Set'): ent['same_as']='background tool proficiency'; r=ITEMNAME.get('gamingset')
            if r: ent['id']=r
            else: ent['name']=nm; EQ_UNRESOLVED.append((owner_name,nm))
            if note and r: ent['note']=note.group(1)
            if qty==1: ent.pop('qty')
            items.append(ent)
        opts.append({'id':label,'items':items,'coins':coins})
    pid='ID_2024_EQUIPMENT_PACKAGE_'+owner_id.replace('ID_2024_','')
    return pid,opts

# ================= backgrounds
bg_text=ori[ori.index('Background Descriptions\n'):ori.index('Character Species\n')]
BG=['Acolyte','Criminal','Sage','Soldier']
def bg_block(n):
    a=re.search(r'^%s\nAbility Scores:'%n,bg_text,re.M).start()
    nxt=[re.search(r'^%s\nAbility Scores:'%o,bg_text,re.M).start() for o in BG if o!=n]
    b=min([x for x in nxt if x>a]+[len(bg_text)])
    return bg_text[a:b]
FEAT_FOR={'Acolyte':('ID_2024_FEAT_MAGIC_INITIATE','Cleric'),'Criminal':('ID_2024_FEAT_ALERT',None),'Sage':('ID_2024_FEAT_MAGIC_INITIATE','Wizard'),'Soldier':('ID_2024_FEAT_SAVAGE_ATTACKER',None)}
TOOLREF={'Calligrapher’s Supplies':'ID_PROFICIENCY_TOOL_PROFICIENCY_CALLIGRAPHERS_SUPPLIES','Thieves’ Tools':'ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS'}
BGMAP={'Acolyte':'ID_BACKGROUND_ACOLYTE'}
for n in BG:
    b=bg_block(n)
    abil=re.search(r'Ability Scores: (.*)',b).group(1).replace(' and ',', ').split(', ')
    abil=[a.strip() for a in abil if a.strip()]
    feat=re.search(r'Feat: (.*?) \(see',b).group(1)
    skills=re.search(r'Skill Proficiencies: (.*?)\n',b).group(1).split(' and ')
    tool=re.sub(r'\s+',' ',re.search(r'Tool Proficiency: (.*?)\nEquipment:',b,re.S).group(1)).strip()
    equip=re.search(r'Equipment: (.*)',b,re.S).group(1)
    equip=join_lines(equip.split('\n'))
    bid=idof('BACKGROUND',n)
    pid,opts=parse_package(bid,n,equip)
    featid,param=FEAT_FOR[n]
    rules=[custom('ability_distribution',name='Ability Scores (%s)'%n,pool=[CODE_ABIL[a] for a in abil],options=[[2,1],[1,1,1]],cap=20),
           custom('grant_feat',feat=featid,params=({'spell_list':param} if param else {})),
           ]+[grant(SKILL_ID(s),'Proficiency') for s in skills]
    if tool.startswith('Choose one kind of Gaming Set'): rules.append(select('Proficiency','Tool Proficiency ('+n+')',supports='Gaming Set'))
    else: rules.append(grant(TOOLREF[tool],'Proficiency'))
    rules.append(custom('equipment_package',ref=pid,name='Starting Equipment ('+n+')'))
    bgl=[l for l in b.split('\n')]
    desc='<p><b>Ability Scores:</b> %s</p><p><b>Feat:</b> %s</p><p><b>Skill Proficiencies:</b> %s</p><p><b>Tool Proficiency:</b> %s</p><p><b>Equipment:</b> %s</p>'%(
        esc(', '.join(abil)),esc(feat),esc(' and '.join(skills)),esc(tool),esc(equip))
    e=el(bid,n,'Background',desc,rules=rules,setters=[setter('short','%s, %s; %s'%(' and '.join(skills),tool,feat))],
         data={'ability_scores':[CODE_ABIL[a] for a in abil],'origin_feat':featid,'origin_feat_params':({'spell_list':param} if param else {}),
               'skills':skills,'tool':tool,'equipment_package':pid})
    if n in BGMAP: e['maps_to']={'dnd2014':BGMAP[n],'kind':'approximate'}
    emit('backgrounds/backgrounds.json',e)
    emit('backgrounds/equipment-packages.json',el(pid,'Starting Equipment ('+n+')','Equipment Package','',data={'owner':bid,'options':opts}))
print('unresolved equipment',EQ_UNRESOLVED)

# ================= feats
ft=ori[ori.index('Origin Feats\nAlert'):]
FEATS=[('Alert','Origin',None),('Magic Initiate','Origin',None),('Savage Attacker','Origin',None),('Skilled','Origin',None),
 ('Ability Score Improvement','General','Level 4+'),('Grappler','General','Level 4+, Strength or Dexterity 13+'),
 ('Archery','Fighting Style','Fighting Style Feature'),('Defense','Fighting Style','Fighting Style Feature'),('Great Weapon Fighting','Fighting Style','Fighting Style Feature'),('Two-Weapon Fighting','Fighting Style','Fighting Style Feature'),
 ('Boon of Combat Prowess','Epic Boon','Level 19+'),('Boon of Dimensional Travel','Epic Boon','Level 19+'),('Boon of Fate','Epic Boon','Level 19+'),('Boon of Irresistible Offense','Epic Boon','Level 19+'),
 ('Boon of Spell Recall','Epic Boon','Level 19+, Spellcasting Feature'),('Boon of the Night Spirit','Epic Boon','Level 19+'),('Boon of Truesight','Epic Boon','Level 19+')]
def feat_block(n):
    pos=[]
    for fn,_,_ in FEATS:
        m=re.search(r'^%s\n(Origin|General|Fighting Style|Epic Boon) Feat'%re.escape(fn),ft,re.M); pos.append((m.start(),fn,m.end()))
    pos.sort()
    for i,(a,fn,e) in enumerate(pos):
        if fn==n:
            b=pos[i+1][0] if i+1<len(pos) else len(ft)
            seg=ft[e:b]
            # remove category section headers that precede next feat
            seg=re.sub(r'\n(General|Fighting Style|Epic Boon) Feats\n','\n',seg)
            seg=re.sub(r'^\s*\(Prerequisite:.*?\)\n','',seg,count=1,flags=re.S)
            return seg
FEAT_RULES={
 'Alert':[stat('initiative','proficiency',alt='Alert (Initiative Proficiency)'),custom('initiative_swap',with_ally=True)],
 'Magic Initiate':[custom('select_param',name='Spell List',options=['Cleric','Druid','Wizard'],distinct_per_instance=True),
                   custom('select_param',name='Spellcasting Ability',options=['Intelligence','Wisdom','Charisma']),
                   select('Spell','Magic Initiate Cantrips',supports='$(param:Spell List), 0',number=2,spellcasting='Magic Initiate'),
                   select('Spell','Magic Initiate Level 1 Spell',supports='$(param:Spell List), 1',spellcasting='Magic Initiate',prepared=True),
                   custom('spell_free_cast',select='Magic Initiate Level 1 Spell',uses=1,recharge='long_rest',also_with_slots=True,ability={'param':'Spellcasting Ability'}),
                   custom('spell_replace_on_level',scope='feat spells',same_level=True)],
 'Savage Attacker':[],
 'Skilled':[select('Proficiency','Skilled',supports='Skill||Tool||Gaming Set',number=3)],
 'Ability Score Improvement':[custom('ability_increase',options=[{'abilities':1,'amount':2},{'abilities':2,'amount':1}],cap=20)],
 'Grappler':[custom('ability_increase',options=[{'abilities':['strength','dexterity'],'pick':1,'amount':1}],cap=20)],
 'Archery':[custom('attack_bonus',weapon='ranged',value=2)],
 'Defense':[stat('ac:misc','1',bonus='fighting style',equipped='[armor:any]',alt='Defense (Fighting Style)')],
 'Great Weapon Fighting':[custom('damage_die_floor',floor=3,weapon='melee two-handed or versatile held in two hands')],
 'Two-Weapon Fighting':[custom('extra_attack_damage_modifier',property='Light')],
 'Boon of Combat Prowess':[custom('ability_increase',options=[{'abilities':1,'amount':1}],cap=30),custom('resource',id='peerless aim',name='Peerless Aim',max={'type':'fixed','value':1},recharge=['turn_start'])],
 'Boon of Dimensional Travel':[custom('ability_increase',options=[{'abilities':1,'amount':1}],cap=30)],
 'Boon of Fate':[custom('ability_increase',options=[{'abilities':1,'amount':1}],cap=30),custom('resource',id='improve fate',name='Improve Fate',max={'type':'fixed','value':1},recharge=['initiative','short_rest','long_rest'])],
 'Boon of Irresistible Offense':[custom('ability_increase',options=[{'abilities':['strength','dexterity'],'pick':1,'amount':1}],cap=30)],
 'Boon of Spell Recall':[custom('ability_increase',options=[{'abilities':['intelligence','wisdom','charisma'],'pick':1,'amount':1}],cap=30)],
 'Boon of the Night Spirit':[custom('ability_increase',options=[{'abilities':1,'amount':1}],cap=30)],
 'Boon of Truesight':[custom('ability_increase',options=[{'abilities':1,'amount':1}],cap=30),grant('ID_VISION_TRUESIGHT','Vision'),stat('truesight:range','60',bonus='base')],
}
FEAT_REQ={'Level 4+':'[character:4]','Level 19+':'[character:19]'}
for n,cat,pre in FEATS:
    seg=feat_block(n)
    lines=seg.split('\n')
    # drop the "(Prerequisite..." continuation of the category line
    while lines and (not lines[0].strip() or lines[0].lstrip().startswith(('feat','Feat','Dexterity','Feature)','Strength','Level 4','Spellcasting'))): lines=lines[1:]
    # category line may wrap: ' (Prerequisite: ...' handled by stripping lines up to ')' 
    body=chunk_html2(lines)
    fid=idof('FEAT',n)
    req=None
    if pre:
        parts=[p.strip() for p in pre.split(',')]
        rq=[]
        for p in parts:
            if p in FEAT_REQ: rq.append(FEAT_REQ[p])
            elif p=='Strength or Dexterity 13+': rq.append('([str:13]||[dex:13])')
        req='&&'.join(rq) if rq else None
    rep=None
    rep={'allowed':n in('Magic Initiate','Skilled','Ability Score Improvement'),'distinct_param':'spell_list' if n=='Magic Initiate' else None}
    pdata={'level_min':4 if 'Level 4' in (pre or '') else (19 if 'Level 19' in (pre or '') else None),
           'abilities_any':[{'strength':13},{'dexterity':13}] if n=='Grappler' else None,
           'requires_feature':'Fighting Style' if cat=='Fighting Style' else ('Spellcasting' if n=='Boon of Spell Recall' else None)}
    e=el(fid,n,'Feat',body,rules=FEAT_RULES[n],setters=[setter('category',cat)],
         data={'category':cat,'prerequisite':{k:v for k,v in pdata.items() if v is not None},'repeatable':{k:v for k,v in rep.items() if v is not None}},
         **({'prerequisite':pre} if pre else {}),**({'requirements':req} if req else {}))
    if n=='Grappler': e['maps_to']={'dnd2014':'ID_PHB_FEAT_GRAPPLER','kind':'approximate'}
    e['supports']=[cat+' Feat','Feat']
    emit('feats/feats.json',e)
print('feats',len(OUT['feats/feats.json']))
