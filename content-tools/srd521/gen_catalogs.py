import sys; sys.path.insert(0,'.')
from gen_classes import *
import pickle
# ---------------- Metamagic
sec=[s for s in PARSED['Sorcerer'][1] if s['kind']=='meta' and s['name']=='Metamagic Options'][0]
MM=['Careful Spell','Distant Spell','Empowered Spell','Extended Spell','Heightened Spell','Quickened Spell','Seeking Spell','Subtle Spell','Transmuted Spell','Twinned Spell']
lines=sec['lines']
idx=[]
for n in MM:
    i=next(k for k,l in enumerate(lines) if l.strip()==n)
    idx.append((i,n))
idx.sort()
for k,(i,n) in enumerate(idx):
    j=idx[k+1][0] if k+1<len(idx) else len(lines)
    seg=lines[i+1:j]
    cost=int(re.match(r'Cost: (\d+) Sorcery Point',seg[0].strip()).group(1))
    body=chunk_html2(seg[1:])
    emit('classes/sorcerer.json',opt(idof('OPTION','Metamagic',n),n,'<p><b>Cost:</b> %d Sorcery Point%s</p>\n%s'%(cost,'s' if cost>1 else '',body),['Metamagic'],
         rules=[custom('resource_cost',resource='sorcery points',cost=cost)],cost=cost,class_name='Sorcerer'))
# dragon damage types for Elemental Affinity
for t in ['Acid','Cold','Fire','Lightning','Poison']:
    emit('classes/sorcerer-draconic-sorcery.json',opt(idof('OPTION','Dragon Damage Type',t),t,'<p>Elemental Affinity damage type: %s. You have Resistance to %s damage.</p>'%(t,t),['Dragon Damage Type'],
         rules=[resist(t)],damage_type=t.lower()))
# ---------------- Eldritch Invocations
sec=[s for s in PARSED['Warlock'][1] if s['kind']=='meta' and s['name']=='Eldritch Invocation Options'][0]
lines=sec['lines']
# name -> (prereq text, min level, prereq option, repeatable, prereq_lines, rules)
W=lambda *a,**k: custom(*a,**k)
def castself(spell,**k): return custom('grant_spell',spell=sp(spell),mode='at_will_no_slot',target='self',ability='charisma',**k)
INV=[
 ('Agonizing Blast','Level 2+ Warlock, a Warlock Cantrip That Deals Damage',2,None,True,2,[custom('cantrip_damage_bonus',spell_class='Warlock',value='charisma:modifier',choose_cantrip=True,cantrip_filter='deals damage')]),
 ('Armor of Shadows',None,0,None,False,0,[castself('Mage Armor')]),
 ('Ascendant Step','Level 5+ Warlock',5,None,False,1,[castself('Levitate')]),
 ('Devil’s Sight','Level 2+ Warlock',2,None,False,1,[custom('sense',sense='devils sight',range=120,note='see normally in Dim Light and Darkness, magical or nonmagical')]),
 ('Devouring Blade','Level 12+ Warlock, Thirsting Blade Invocation',12,'Thirsting Blade',False,2,[extra_attack(2)]),
 ('Eldritch Mind',None,0,None,False,0,[sadv(ability='constitution',context='maintain concentration')]),
 ('Eldritch Smite','Level 5+ Warlock, Pact of the Blade Invocation',5,'Pact of the Blade',False,2,[custom('extra_damage',name='Eldritch Smite',dice='1d8',per_slot_level='1d8',damage_type='force',once_per='turn',trigger='pact weapon hit',cost='Pact Magic spell slot')]),
 ('Eldritch Spear','Level 2+ Warlock, a Warlock Cantrip That Deals Damage',2,None,True,2,[custom('cantrip_range_bonus',spell_class='Warlock',feet_per_warlock_level=30,choose_cantrip=True,cantrip_filter='deals damage, range 10+ feet')]),
 ('Fiendish Vigor','Level 2+ Warlock',2,None,False,1,[castself('False Life',max_roll=True)]),
 ('Gaze of Two Minds','Level 5+ Warlock',5,None,False,1,[]),
 ('Gift of the Depths','Level 5+ Warlock',5,None,False,1,[stat('speed:swim','speed'),always('Water Breathing',list='Warlock',free_cast={'uses':1,'recharge':'long_rest'},also_with_slots=False)]),
 ('Gift of the Protectors','Level 9+ Warlock, Pact of the Tome Invocation',9,'Pact of the Tome',False,2,[res('gift of the protectors','Gift of the Protectors',fx(1),['long_rest'])]),
 ('Investment of the Chain Master','Level 5+ Warlock, Pact of the Chain Invocation',5,'Pact of the Chain',False,2,[]),
 ('Lessons of the First Ones','Level 2+ Warlock',2,None,True,1,[custom('select_feat',name='Lessons of the First Ones',category='Origin',distinct=True)]),
 ('Lifedrinker','Level 9+ Warlock, Pact of the Blade Invocation',9,'Pact of the Blade',False,2,[custom('extra_damage',name='Lifedrinker',dice='1d6',damage_type='necrotic, psychic, or radiant',once_per='turn',trigger='pact weapon hit')]),
 ('Mask of Many Faces','Level 2+ Warlock',2,None,False,1,[castself('Disguise Self')]),
 ('Master of Myriad Forms','Level 5+ Warlock',5,None,False,1,[castself('Alter Self')]),
 ('Misty Visions','Level 2+ Warlock',2,None,False,1,[castself('Silent Image')]),
 ('One with Shadows','Level 5+ Warlock',5,None,False,1,[castself('Invisibility',condition='in Dim Light or Darkness')]),
 ('Otherworldly Leap','Level 2+ Warlock',2,None,False,1,[castself('Jump')]),
 ('Pact of the Blade',None,0,None,False,0,[custom('pact_weapon',action='bonus_action',weapon_kinds=['Simple Melee','Martial Melee'],ability='charisma',damage_types=['necrotic','psychic','radiant'],is_spellcasting_focus=True)]),
 ('Pact of the Chain',None,0,None,False,0,[always('Find Familiar',list='Warlock',mode='at_will_no_slot',special_forms=['Imp','Pseudodragon','Quasit','Skeleton','Sphinx of Wonder','Sprite','Venomous Snake'])]),
 ('Pact of the Tome',None,0,None,False,0,[select('Spell','Book of Shadows Cantrips',supports='0',number=3,spellcasting='Warlock',prepared=True),select('Spell','Book of Shadows Rituals',supports='1, Ritual',number=2,spellcasting='Warlock',prepared=True),grant('ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_ARCANE_FOCUS','Proficiency')]),
 ('Repelling Blast','Level 2+ Warlock, a Warlock Cantrip That Deals Damage via an Attack Roll',2,None,True,2,[custom('cantrip_rider',spell_class='Warlock',effect='push 10 feet (Large or smaller)',choose_cantrip=True,cantrip_filter='requires an attack roll')]),
 ('Thirsting Blade','Level 5+ Warlock, Pact of the Blade Invocation',5,'Pact of the Blade',False,2,[extra_attack(1,)]),
 ('Visions of Distant Realms','Level 9+ Warlock',9,None,False,1,[castself('Arcane Eye')]),
 ('Whispers of the Grave','Level 7+ Warlock',7,None,False,1,[castself('Speak with Dead')]),
 ('Witch Sight','Level 15+ Warlock',15,None,False,1,[grant('ID_VISION_TRUESIGHT','Vision'),stat('truesight:range','30',bonus='base')]),
]
names=[x[0] for x in INV]
pos=[]
for n in names:
    i=next(k for k,l in enumerate(lines) if l.strip()==n)
    pos.append((i,n))
pos.sort()
assert len(INV)==28
for k,(i,n) in enumerate(pos):
    j=pos[k+1][0] if k+1<len(pos) else len(lines)
    spec=next(x for x in INV if x[0]==n)
    _,pre,minlv,reqopt,rep,plines,rules=spec
    seg=lines[i+1+plines:j]
    body=chunk_html2(seg)
    req=[]
    if minlv: req.append('[warlock:%d]'%minlv)
    if reqopt: req.append(idof('OPTION','Eldritch Invocation',reqopt))
    e=opt(idof('OPTION','Eldritch Invocation',n),n,body,['Eldritch Invocation'],rules=rules,class_name='Warlock',min_warlock_level=minlv or None,requires_invocation=reqopt,repeatable=rep)
    if pre: e['prerequisite']=pre
    if req: e['requirements']='&&'.join(req)
    emit('classes/warlock.json',e)
print('catalogs ok')
