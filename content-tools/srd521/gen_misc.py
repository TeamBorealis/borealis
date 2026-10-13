import sys; sys.path.insert(0,'.')
from gen_catalogs import *
from gloss import glossary
import pickle
ct=open('creation.txt').read()
# ---- profile tables
xp=[];pb=[]
for m in re.finditer(r'^\s+(\d{1,2})\s+([\d,]+)\s+\+(\d)\s*$',ct,re.M):
    xp.append(int(m.group(2).replace(',','')));pb.append(int(m.group(3)))
assert len(xp)==20,len(xp)
sab={}
for m in re.finditer(r'^(Barbarian|Bard|Cleric|Druid|Fighter|Monk|Paladin|Ranger|Rogue|Sorcerer|Warlock|Wizard)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s*$',ct,re.M):
    sab[m.group(1)]=dict(zip(['strength','dexterity','constitution','intelligence','wisdom','charisma'],map(int,m.groups()[1:])))
assert len(sab)==12,len(sab)
mc=[]
blk=ct[ct.index('Multiclass Spellcaster:'):]
for m in re.finditer(r'^\s+(\d{1,2})\s+((?:[\d—]\s+){8}[\d—])\s*$',blk,re.M):
    row=[0 if x=='—' else int(x) for x in m.group(2).split()]
    mc.append(row)
assert len(mc)==20,len(mc)
POINTS={'8':0,'9':1,'10':2,'11':3,'12':4,'13':5,'14':7,'15':9}
tables={
 'experience_points':xp,'proficiency_bonus':pb,'point_buy':{'budget':27,'min_score':8,'max_score':15,'costs':POINTS},
 'standard_array':[15,14,13,12,10,8],'standard_array_by_class':sab,
 'fixed_hit_points_per_level':{'Barbarian':7,'Fighter':6,'Paladin':6,'Ranger':6,'Bard':5,'Cleric':5,'Druid':5,'Monk':5,'Rogue':5,'Warlock':5,'Sorcerer':4,'Wizard':4},
 'multiclass_spell_slots':mc,'multiclass_caster_levels':{'full':['Bard','Cleric','Druid','Sorcerer','Wizard'],'half_round_up':['Paladin','Ranger'],'pact':['Warlock']},
 'ability_modifier':'floor((score - 10) / 2)','ability_score_cap':20,'epic_boon_ability_cap':30,
 'languages':{'standard':[x for x in GO.STD],'rare':[x for x in GO.RARE]},
 'coin_values_gp':{'cp':'1/100','sp':'1/10','ep':'1/2','gp':1,'pp':10},
 'character_creation_steps':['class','origin','ability_scores','alignment','details'],
 'exhaustion':{'levels':6,'d20_penalty_per_level':2,'speed_penalty_ft_per_level':5},
}
emit('base/profile-tables.json',el('ID_2024_PROFILE_TABLES','SRD 5.2.1 Rules Tables','Core','<p>Experience, proficiency bonus, point buy, standard arrays, fixed hit points and multiclass spell slot tables from SRD 5.2.1.</p>',
     data=tables,supports=['Profile Tables']))
# ---- conditions + glossary
GL=glossary()
seen=set();CONDS=[];RULES=[]
for name,body in GL:
    m=re.match(r'^(.*?)(?: \[(.*)\])?$',name)
    base,tag=m.group(1),m.group(2)
    if base in seen and tag!='Action': 
        # merge duplicates: append
        continue
    seen.add(base)
    lines=body
    html=chunk_html2(lines)
    if tag=='Condition':
        CONDS.append((base,html))
    else:
        RULES.append((base,tag,html))
assert len(CONDS)==15,[c[0] for c in CONDS]
for base,html in CONDS:
    rules=[]
    if base=='Exhaustion': rules=[custom('exhaustion',max_levels=6,d20_penalty_per_level=2,speed_penalty_ft_per_level=5,dies_at_level=6,reduce_on='long_rest')]
    emit('rules/conditions.json',el(idof('CONDITION',base),base,'Condition',html,supports=['Condition'],rules=rules,data={'tag':'Condition'}))
for base,tag,html in RULES:
    emit('rules/glossary.json',el(idof('RULE',base+(('_'+tag) if tag else '')),base,'Rules Glossary Entry',html,supports=['Rules Glossary']+([tag] if tag else []),data={'tag':tag}))
# ---- spells
sp_els=pickle.load(open('spells_out.pkl','rb'))
for e in sp_els:
    e=dict(e)
    emit('spells/level-%d.json'%e['data']['level'],e)
# ---- magic items
raw=pickle.load(open('magic_raw.pkl','rb'))
n14={}
for k,(n,t) in IDS2014.items():
    if t=='Magic Item': n14.setdefault(norm_name(n),k)
CATMAP={'Armor':'armor','Potion':'potions','Ring':'rings','Rod':'rods','Scroll':'scrolls','Staff':'staffs','Wand':'wands','Weapon':'weapons','Wondrous Item':'wondrous-items'}
RARES=['Very Rare','Uncommon','Common','Rare','Legendary','Artifact']
for o in raw:
    name=o['name']
    if name.startswith('Potions of'): name='Potion of Healing'
    body=chunk_html2(o['body'])
    rar=o['rarity']
    rarities=[r for r in ['Common','Uncommon','Rare','Very Rare','Legendary','Artifact'] if re.search(r'\b'+r+r'\b',rar.replace('Very Rare','VR').replace('Uncommon','UC')) or (r=='Very Rare' and 'Very Rare' in rar) or (r=='Uncommon' and 'Uncommon' in rar)]
    # simple, robust: scan words
    rarities=[]
    for r in ['Common','Uncommon','Rare','Very Rare','Legendary','Artifact']:
        pat=r'(?<![A-Za-z])'+r+r'(?![A-Za-z])'
        if r=='Rare':
            if re.search(r'(?<!Very )(?<![A-Za-z])Rare(?![A-Za-z])',rar): rarities.append(r)
        elif re.search(pat,rar): rarities.append(r)
    st=[setter('category',CATMAP[o['cat']].replace('-',' ').title() if False else {'Armor':'Armor','Potion':'Potions','Ring':'Rings','Rod':'Rods','Scroll':'Spell Scrolls','Staff':'Staffs','Wand':'Wands','Weapon':'Weapons','Wondrous Item':'Wondrous Items'}[o['cat']]),
        setter('type','Wondrous item' if o['cat']=='Wondrous Item' else o['cat'],**({'addition':o['sub']} if o['sub'] and o['cat']!='Wondrous Item' else {})),
        setter('rarity',rar)]
    if o['attune'] is not None:
        st.append(setter('attunement','true',**({'addition':o['attune']} if o['attune'] else {})))
    ch=re.search(r'has (\d+) charges',' '.join(o['body']))
    e=el(idof('MAGIC_ITEM',name),name,'Magic Item',body,setters=st,
         data={'item_type':o['cat'],'item_subtype':o['sub'],'rarity_text':rar,'rarities':rarities,'requires_attunement':o['attune'] is not None,
               'attunement_by':o['attune'] or None,'charges':int(ch.group(1)) if ch else None,'raw_type_line':o['raw']})
    k=norm_name(name)
    if k in n14: e['maps_to']={'dnd2014':n14[k],'kind':'equivalent'}
    elif k.endswith('s') and k[:-1] in n14: e['maps_to']={'dnd2014':n14[k[:-1]],'kind':'approximate'}
    emit('magic-items/%s.json'%CATMAP[o['cat']],e)
print('misc ok',len(CONDS),len(RULES))
