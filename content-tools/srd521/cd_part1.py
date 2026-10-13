from cd_base import *
LIGHT='ID_PROFICIENCY_ARMOR_PROFICIENCY_LIGHT_ARMOR'; MED='ID_PROFICIENCY_ARMOR_PROFICIENCY_MEDIUM_ARMOR'; HEAVY='ID_PROFICIENCY_ARMOR_PROFICIENCY_HEAVY_ARMOR'; SHIELDS='ID_PROFICIENCY_ARMOR_PROFICIENCY_SHIELDS'
SIMPLE='ID_PROFICIENCY_WEAPON_PROFICIENCY_SIMPLE_WEAPONS'; MARTIAL='ID_PROFICIENCY_WEAPON_PROFICIENCY_MARTIAL_WEAPONS'

# ---------------------------------------------------------------- Barbarian
b=defclass('Barbarian',hd=12,primary=['strength'],saves=['strength','constitution'],
  skills=(2,['Animal Handling','Athletics','Intimidation','Nature','Perception','Survival']),
  weapons=[SIMPLE,MARTIAL],armor=[LIGHT,MED,SHIELDS],tools=[],
  multiclass=dict(requirements='[str:13]',prereq='Strength 13',grants=[grant(MARTIAL,'Proficiency'),grant(SHIELDS,'Proficiency')]),
  spell=None)
b['features']={
 'Rage':[res('rage','Rage',tab('rages'),['short_rest','long_rest'],regain={'short_rest':1,'long_rest':'all'}),
         stat('rage damage','2'),stat('rage damage','1',level=9),stat('rage damage','1',level=16),
         toggle('rage','Rage','bonus_action','1 round (extendable up to 10 minutes)',
                [resist('Bludgeoning'),resist('Piercing'),resist('Slashing'),adv('ability_check',ability='strength'),adv('saving_throw',ability='strength'),
                 stat('damage:strength melee','rage damage',bonus='rage')],
                requires_not_wearing='heavy armor',blocks=['concentration','spellcasting'])],
 'Unarmored Defense':ac_alt('unarmored defense barbarian','Unarmored Defense (Barbarian)',['dexterity','constitution']),
 'Weapon Mastery':[custom('weapon_mastery',name='Weapon Mastery',count=tab('weapon_mastery'),filter={'weapon_kind':'melee','categories':['Simple','Martial']},change_on='long_rest')],
 'Danger Sense':[sadv(ability='dexterity',unless_condition='incapacitated')],
 'Barbarian Subclass':[select('Archetype','Primal Path',supports='Barbarian Subclass')],
 'Primal Knowledge':[skill_sel('Primal Knowledge (Skill)',['Animal Handling','Athletics','Intimidation','Nature','Perception','Survival'])],
 'Extra Attack':[extra_attack(1)],
 'Fast Movement':[stat('speed','10',equipped='![armor:heavy]',alt='Fast Movement')],
 'Feral Instinct':[adv('initiative')],
 'Brutal Strike':[custom('extra_damage',name='Brutal Strike',dice='1d10',trigger='forgo Reckless Attack advantage on one Strength attack',effects_choose=1),
                  grant(idof('OPTION','Brutal Strike','Forceful Blow'),'Option'),grant(idof('OPTION','Brutal Strike','Hamstring Blow'),'Option')],
 'Relentless Rage':[res('relentless rage','Relentless Rage DC',fx(1),['short_rest','long_rest'],counter='dc',start=10,step=5)],
 'Improved Brutal Strike@13':[grant(idof('OPTION','Brutal Strike','Staggering Blow'),'Option'),grant(idof('OPTION','Brutal Strike','Sundering Blow'),'Option')],
 'Improved Brutal Strike@17':[custom('extra_damage',name='Brutal Strike (improved)',dice='2d10',effects_choose=2)],
 'Persistent Rage':[res('persistent rage','Persistent Rage',fx(1),['long_rest'])],
 'Indomitable Might':[custom('check_floor',applies_to=['strength check','strength saving throw'],floor='strength')],
 'Primal Champion':[stat('strength','4'),stat('constitution','4'),stat('strength:max','5'),stat('constitution:max','5')],
}
b['subclass']=dict(feature_rules={
 'Frenzy':[custom('extra_damage',name='Frenzy',trigger='Reckless Attack while Rage is active; first target hit with a Strength-based attack each turn',dice_count={'type':'stat','stat':'rage damage'},die='d6',once_per='turn',first_target_only=True,damage_type='same as the attack')],
 'Mindless Rage':[custom('condition_immunity_while',toggle='rage',conditions=['charmed','frightened'])],
 'Retaliation':[],
 'Intimidating Presence':[res('intimidating presence','Intimidating Presence',fx(1),['long_rest'],alt_regain='expend a Rage use'),
                          custom('save_dc',id='intimidating presence',formula='8+strength+proficiency')]})
b['subopts']=[('Brutal Strike',9,'Brutal Strike',['Forceful Blow','Hamstring Blow'],{}),('Improved Brutal Strike@13',13,'Brutal Strike',['Staggering Blow','Sundering Blow'],{})]
b['asi_levels']=[4,8,12,16]

# ---------------------------------------------------------------- Bard
d=defclass('Bard',hd=8,primary=['charisma'],saves=['dexterity','charisma'],
  skills=(3,ALL_SKILLS),weapons=[SIMPLE],armor=[LIGHT],
  tools=[select('Proficiency','Musical Instrument Proficiency (Bard)',supports='Musical Instrument',number=3)],
  multiclass=dict(requirements='[cha:13]',prereq='Charisma 13',grants=[select('Proficiency','Skill Proficiency (Bard Multiclass)',supports='Skill'),select('Proficiency','Musical Instrument Proficiency (Bard Multiclass)',supports='Musical Instrument'),grant(LIGHT,'Proficiency')]),
  spell=dict(ability='Charisma',known=True,prepare=True,replace='level',list='Bard',slots=SPELLSLOTS_FULL,focus='ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_MUSICAL_INSTRUMENT',focus_name='Musical Instrument'))
d['features']={
 'Bardic Inspiration':[res('bardic inspiration','Bardic Inspiration',frm('max(1, charisma:modifier)'),['long_rest']),
                       stat('bardic inspiration die','6'),stat('bardic inspiration die','2',level=5),stat('bardic inspiration die','2',level=10),stat('bardic inspiration die','2',level=15)],
 'Expertise':[expertise(2),expertise(2,level=9)],
 'Jack of All Trades':[stat('jack of all trades','proficiency:half',inline=False) if False else custom('skill_bonus_unproficient',value='proficiency:half',rounding='down',applies_to='ability checks using a skill without proficiency')],
 'Bard Subclass':[select('Archetype','Bard College',supports='Bard Subclass')],
 'Font of Inspiration':[custom('resource_regain',resource='bardic inspiration',on=['short_rest','long_rest']),custom('resource_regain',resource='bardic inspiration',by='expend a spell slot',amount=1)],
 'Countercharm':[],
 'Magical Secrets':[custom('spell_list_extend',spell_class='Bard',lists=['Cleric','Druid','Wizard'],for_prepared=True)],
 'Superior Inspiration':[custom('resource_regain',resource='bardic inspiration',on=['initiative'],up_to=2)],
 'Words of Creation':[always('Power Word Heal',list='Bard'),always('Power Word Kill',list='Bard')],
}
d['subclass']=dict(feature_rules={
 'Bonus Proficiencies':[skill_sel('Bonus Proficiencies (College of Lore)',ALL_SKILLS,n=3)],
 'Cutting Words':[],
 'Magical Discoveries':[select('Spell','Magical Discoveries',supports='(Cleric||Druid||Wizard), $(spellcasting:slots)||0',number=2,spellcasting='Bard',prepared=True,allow_replace=True)],
 'Peerless Skill':[]})

# ---------------------------------------------------------------- Cleric
c=defclass('Cleric',hd=8,primary=['wisdom'],saves=['wisdom','charisma'],
  skills=(2,['History','Insight','Medicine','Persuasion','Religion']),weapons=[SIMPLE],armor=[LIGHT,MED,SHIELDS],tools=[],
  multiclass=dict(requirements='[wis:13]',prereq='Wisdom 13',grants=[grant(LIGHT,'Proficiency'),grant(MED,'Proficiency'),grant(SHIELDS,'Proficiency')]),
  spell=dict(ability='Wisdom',known=True,prepare=True,replace='long_rest',list='Cleric',slots=SPELLSLOTS_FULL,focus='ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_HOLY_SYMBOL',focus_name='Holy Symbol'))
c['features']={
 'Divine Order':[select('Option','Divine Order',supports='Divine Order')],
 'Channel Divinity':[res('channel divinity cleric','Channel Divinity',tab('channel_divinity'),['short_rest','long_rest'],regain={'short_rest':1,'long_rest':'all'}),
                     grant(idof('OPTION','Cleric Channel Divinity','Divine Spark'),'Option'),grant(idof('OPTION','Cleric Channel Divinity','Turn Undead'),'Option'),
                     custom('save_dc',id='channel divinity',formula='spell save DC (Cleric)')],
 'Cleric Subclass':[select('Archetype','Divine Domain',supports='Cleric Subclass')],
 'Sear Undead':[],
 'Blessed Strikes':[select('Option','Blessed Strikes',supports='Blessed Strikes')],
 'Divine Intervention':[res('divine intervention','Divine Intervention',fx(1),['long_rest'])],
 'Improved Blessed Strikes':[],
 'Greater Divine Intervention':[],
}
c['subclass']=dict(feature_rules={
 'Disciple of Life':[],
 'Life Domain Spells':[always(s,lv,list='Cleric') for lv,ss in [(3,['Aid','Bless','Cure Wounds','Lesser Restoration']),(5,['Mass Healing Word','Revivify']),(7,['Aura of Life','Death Ward']),(9,['Greater Restoration','Mass Cure Wounds'])] for s in ss],
 'Preserve Life':[custom('channel_divinity_option',pool_hit_points='5*cleric level',max_fraction='half hit point maximum')],
 'Blessed Healer':[],
 'Supreme Healing':[]})
c['subopts']=[('Divine Order',1,'Divine Order',['Protector','Thaumaturge'],
   {'Protector':[grant(MARTIAL,'Proficiency'),grant(HEAVY,'Proficiency')],
    'Thaumaturge':[select('Spell','Thaumaturge Extra Cantrip',supports='$(spellcasting:list), 0',spellcasting='Cleric'),custom('skill_bonus',skills=['Arcana','Religion'],value='wisdom:modifier',minimum=1)]}),
  ('Channel Divinity',2,'Cleric Channel Divinity',['Divine Spark','Turn Undead'],{}),
  ('Blessed Strikes',7,'Blessed Strikes',['Divine Strike','Potent Spellcasting'],
   {'Divine Strike':[custom('extra_damage',name='Divine Strike',dice='1d8',trigger='hit with a weapon attack',once_per='turn',damage_type='necrotic or radiant'),custom('extra_damage',level=14,name='Divine Strike (improved)',dice='2d8',replaces='Divine Strike')],
    'Potent Spellcasting':[custom('cantrip_damage_bonus',spell_class='Cleric',value='wisdom:modifier')]})]
