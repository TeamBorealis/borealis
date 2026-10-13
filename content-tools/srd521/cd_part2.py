from cd_part1 import *
# ---------------------------------------------------------------- Druid
d=defclass('Druid',hd=8,primary=['wisdom'],saves=['intelligence','wisdom'],
  skills=(2,['Animal Handling','Arcana','Insight','Medicine','Nature','Perception','Religion','Survival']),weapons=[SIMPLE],armor=[LIGHT,SHIELDS],
  tools=[grant('ID_PROFICIENCY_TOOL_PROFICIENCY_HERBALISM_KIT','Proficiency')],
  multiclass=dict(requirements='[wis:13]',prereq='Wisdom 13',grants=[grant(LIGHT,'Proficiency'),grant(SHIELDS,'Proficiency')]),
  spell=dict(ability='Wisdom',known=True,prepare=True,replace='long_rest',list='Druid',slots=SPELLSLOTS_FULL,focus='ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_DRUIDIC_FOCUS',focus_name='Druidic Focus'))
d['features']={
 'Druidic':[grant('ID_LANGUAGE_DRUIDIC','Language'),always('Speak with Animals',list='Druid')],
 'Primal Order':[select('Option','Primal Order',supports='Primal Order')],
 'Wild Shape':[res('wild shape','Wild Shape',tab('wild_shape'),['short_rest','long_rest'],regain={'short_rest':1,'long_rest':'all'}),
               custom('beast_forms',known=[{'level':2,'forms':4,'max_cr':'1/4','fly':False},{'level':4,'forms':6,'max_cr':'1/2','fly':False},{'level':8,'forms':8,'max_cr':'1','fly':True}],duration_hours='druid level / 2',temp_hp='druid level',change_on='long_rest')],
 'Wild Companion':[custom('spell_cast_alt_cost',spell=sp('Find Familiar'),cost=['spell slot','wild shape use'],no_material=True)],
 'Druid Subclass':[select('Archetype','Druid Circle',supports='Druid Subclass')],
 'Wild Resurgence':[],
 'Elemental Fury':[select('Option','Elemental Fury',supports='Elemental Fury')],
 'Improved Elemental Fury':[],
 'Beast Spells':[],
 'Archdruid':[],
}
d['subclass']=dict(feature_rules={
 'Circle of the Land Spells':[custom('select_param',name='Land Type',options=['Arid','Polar','Temperate','Tropical'],reselect_on='long_rest')]+
   [always(s,lv,list='Druid',requirements_param={'Land Type':land}) for land,tbl in [
     ('Arid',[(3,['Blur','Burning Hands','Fire Bolt']),(5,['Fireball']),(7,['Blight']),(9,['Wall of Stone'])]),
     ('Polar',[(3,['Fog Cloud','Hold Person','Ray of Frost']),(5,['Sleet Storm']),(7,['Ice Storm']),(9,['Cone of Cold'])]),
     ('Temperate',[(3,['Misty Step','Shocking Grasp','Sleep']),(5,['Lightning Bolt']),(7,['Freedom of Movement']),(9,['Tree Stride'])]),
     ('Tropical',[(3,['Acid Splash','Ray of Sickness','Web']),(5,['Stinking Cloud']),(7,['Polymorph']),(9,['Insect Plague'])])]
     for lv,ss in tbl for s in ss],
 "Land’s Aid":[res('lands aid','Land’s Aid',fx(1),['long_rest'])],
 'Natural Recovery':[always('Circle of the Land Spell',level=6,mode='free_cast_ref') if False else custom('free_cast_choice',level=6,from_feature='Circle of the Land Spells',spell_level='any',uses=1,recharge='long_rest'),custom('slot_recovery',level=6,max_combined_level='ceil(druid level / 2)',max_slot_level=5,uses=1,recharge='long_rest',on='short_rest')],
 'Nature’s Ward':[custom('condition_immunity',conditions=['poisoned']),custom('select_param',name='Nature’s Ward Resistance',options=['Cold','Fire','Lightning'] ,note='by land type')],
 'Nature’s Sanctuary':[res('natures sanctuary','Nature’s Sanctuary',frm('max(1, wisdom:modifier)'),['long_rest'])]})
d['subopts']=[('Primal Order',1,'Primal Order',['Magician','Warden'],
   {'Magician':[select('Spell','Magician Extra Cantrip',supports='$(spellcasting:list), 0',spellcasting='Druid'),custom('skill_bonus',skills=['Arcana','Nature'],value='wisdom:modifier',minimum=1)],
    'Warden':[grant(MARTIAL,'Proficiency'),grant(MED,'Proficiency')]}),
  ('Elemental Fury',7,'Elemental Fury',['Potent Spellcasting','Primal Strike'],
   {'Potent Spellcasting':[custom('cantrip_damage_bonus',spell_class='Druid',value='wisdom:modifier')],
    'Primal Strike':[custom('extra_damage',name='Primal Strike',dice='1d8',once_per='turn',damage_type='cold, fire, lightning, or thunder'),custom('extra_damage',level=15,name='Primal Strike (improved)',dice='2d8',replaces='Primal Strike')]})]

# ---------------------------------------------------------------- Fighter
f=defclass('Fighter',hd=10,primary=['strength','dexterity'],primary_mode='any',saves=['strength','constitution'],
  skills=(2,['Acrobatics','Animal Handling','Athletics','History','Insight','Intimidation','Persuasion','Perception','Survival']),weapons=[SIMPLE,MARTIAL],armor=[LIGHT,MED,HEAVY,SHIELDS],tools=[],
  multiclass=dict(requirements='([str:13]||[dex:13])',prereq='Strength 13 or Dexterity 13',grants=[grant(MARTIAL,'Proficiency'),grant(LIGHT,'Proficiency'),grant(MED,'Proficiency'),grant(SHIELDS,'Proficiency')]),
  spell=None)
f['features']={
 'Fighting Style':[custom('select_feat',name='Fighting Style (Fighter)',category='Fighting Style',replace_on='level_up')],
 'Second Wind':[res('second wind','Second Wind',tab('second_wind'),['short_rest','long_rest'],regain={'short_rest':1,'long_rest':'all'}),custom('healing',name='Second Wind',dice='1d10',plus='fighter level',action='bonus_action')],
 'Weapon Mastery':[custom('weapon_mastery',name='Weapon Mastery',count=tab('weapon_mastery'),filter={'weapon_kind':'any','categories':['Simple','Martial']},change_on='long_rest')],
 'Action Surge':[res('action surge','Action Surge',fx(1),['short_rest','long_rest']),res('action surge','Action Surge (second use)',fx(2),['short_rest','long_rest'],level=17,replaces='action surge')],
 'Tactical Mind':[],
 'Fighter Subclass':[select('Archetype','Martial Archetype',supports='Fighter Subclass')],
 'Extra Attack':[extra_attack(1)],
 'Tactical Shift':[],
 'Indomitable':[res('indomitable','Indomitable',fx(1),['long_rest']),res('indomitable','Indomitable (two uses)',fx(2),['long_rest'],level=13,replaces='indomitable'),res('indomitable','Indomitable (three uses)',fx(3),['long_rest'],level=17,replaces='indomitable')],
 'Tactical Master':[],
 'Two Extra Attacks':[extra_attack(2)],
 'Studied Attacks':[],
 'Three Extra Attacks':[extra_attack(3)],
}
f['subclass']=dict(feature_rules={
 'Improved Critical':[custom('critical_range',min=19,applies_to=['weapon attacks','unarmed strikes'])],
 'Remarkable Athlete':[adv('initiative'),adv('ability_check',skill='athletics')],
 'Additional Fighting Style':[custom('select_feat',name='Additional Fighting Style',category='Fighting Style')],
 'Heroic Warrior':[custom('heroic_inspiration',trigger='start_of_turn_in_combat')],
 'Superior Critical':[custom('critical_range',min=18,applies_to=['weapon attacks','unarmed strikes'])],
 'Survivor':[custom('death_save_rules',advantage=True,natural_20_threshold=18),custom('regeneration',condition='Bloodied and at least 1 HP',amount='5+constitution:modifier',timing='start_of_turn')]})
f['asi_levels']=[4,6,8,12,14,16]

# ---------------------------------------------------------------- Monk
m=defclass('Monk',hd=8,primary=['dexterity','wisdom'],primary_mode='all',saves=['strength','dexterity'],
  skills=(2,['Acrobatics','Athletics','History','Insight','Religion','Stealth']),
  weapons=[SIMPLE,custom('proficiency_filter',weapon_category='Martial',properties_any=['Light'],name='Martial weapons that have the Light property')],armor=[],
  tools=[select('Proficiency','Tool Proficiency (Monk)',supports='Artisan tools||Musical Instrument')],
  multiclass=dict(requirements='([dex:13]&&[wis:13])',prereq='Dexterity 13 and Wisdom 13',grants=[]),spell=None)
m['features']={
 'Martial Arts':[stat('martial arts:dice','6',bonus='martial arts'),stat('martial arts:dice','8',bonus='martial arts',level=5),stat('martial arts:dice','10',bonus='martial arts',level=11),stat('martial arts:dice','12',bonus='martial arts',level=17),
                 stat('martial arts:attack','strength:modifier',bonus='martial arts'),stat('martial arts:attack','dexterity:modifier',bonus='martial arts'),
                 stat('martial arts:damage','strength:modifier',bonus='martial arts'),stat('martial arts:damage','dexterity:modifier',bonus='martial arts'),
                 custom('martial_arts',die_table='martial_arts',bonus_unarmed_strike=True,dexterous_attacks=True,monk_weapons={'simple_melee':True,'martial_melee_with':['Light']},conditions={'no_armor':True,'no_shield':True})],
 'Unarmored Defense':ac_alt('unarmored defense monk','Unarmored Defense (Monk)',['dexterity','wisdom'],equipped='[armor:none],[shield:none]'),
 'Monk’s Focus':[res('focus points','Focus Points',tab('focus_points'),['short_rest','long_rest']),
                 stat('focus save dc','8'),stat('focus save dc','wisdom:modifier'),stat('focus save dc','proficiency'),
                 grant(idof('OPTION','Monk Focus','Flurry of Blows'),'Option'),grant(idof('OPTION','Monk Focus','Patient Defense'),'Option'),grant(idof('OPTION','Monk Focus','Step of the Wind'),'Option')],
 'Unarmored Movement':[stat('speed','10',bonus='unarmored movement',level=2,equipped='[armor:none],[shield:none]'),stat('speed','5',bonus='unarmored movement',level=6,equipped='[armor:none],[shield:none]'),
                       stat('speed','10',bonus='unarmored movement',level=10,equipped='[armor:none],[shield:none]') if False else stat('speed','5',bonus='unarmored movement',level=10,equipped='[armor:none],[shield:none]'),
                       stat('speed','5',bonus='unarmored movement',level=14,equipped='[armor:none],[shield:none]'),stat('speed','5',bonus='unarmored movement',level=18,equipped='[armor:none],[shield:none]')],
 'Uncanny Metabolism':[res('uncanny metabolism','Uncanny Metabolism',fx(1),['long_rest'])],
 'Deflect Attacks':[custom('damage_reduction',reaction=True,dice='1d10',plus=['dexterity:modifier','monk level'],damage_types=['bludgeoning','piercing','slashing']),
                    custom('damage_reduction_all_types',level=13)],
 'Monk Subclass':[select('Archetype','Monastic Tradition',supports='Monk Subclass')],
 'Slow Fall':[],
 'Extra Attack':[extra_attack(1)],
 'Stunning Strike':[],
 'Empowered Strikes':[],
 'Evasion':[custom('evasion')],
 'Acrobatic Movement':[],
 'Heightened Focus':[],
 'Self-Restoration':[],
 'Deflect Energy':[],
 'Disciplined Survivor':[grant('ID_PROFICIENCY_SAVINGTHROW_%s'%a.upper(),'Proficiency') for a in ['Strength','Dexterity','Constitution','Intelligence','Wisdom','Charisma']],
 'Perfect Focus':[],
 'Superior Defense':[],
 'Body and Mind':[stat('dexterity','4'),stat('wisdom','4'),stat('dexterity:max','5'),stat('wisdom:max','5')],
}
m['subclass']=dict(feature_rules={
 'Open Hand Technique':[],
 'Wholeness of Body':[res('wholeness of body','Wholeness of Body',frm('max(1, wisdom:modifier)'),['long_rest'])],
 'Fleet Step':[],'Quivering Palm':[]})
m['subopts']=[('Monk’s Focus',2,'Monk Focus',['Flurry of Blows','Patient Defense','Step of the Wind'],{})]
m['subopts_sub']=[('Open Hand Technique',3,'Open Hand Technique',['Addle','Push','Topple'],{})]

# ---------------------------------------------------------------- Paladin
p=defclass('Paladin',hd=10,primary=['strength','charisma'],primary_mode='all',saves=['wisdom','charisma'],
  skills=(2,['Athletics','Insight','Intimidation','Medicine','Persuasion','Religion']),weapons=[SIMPLE,MARTIAL],armor=[LIGHT,MED,HEAVY,SHIELDS],tools=[],
  multiclass=dict(requirements='([str:13]&&[cha:13])',prereq='Strength 13 and Charisma 13',grants=[grant(MARTIAL,'Proficiency'),grant(LIGHT,'Proficiency'),grant(MED,'Proficiency'),grant(SHIELDS,'Proficiency')]),
  spell=dict(ability='Charisma',known=True,prepare=True,replace='long_rest',list='Paladin',slots='ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF_UP',focus='ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_HOLY_SYMBOL',focus_name='Holy Symbol',cantrips=False))
p['features']={
 'Lay On Hands':[res('lay on hands','Lay On Hands',frm('5 * paladin level'),['long_rest'],pool=True,cure_poison_cost=5)],
 'Weapon Mastery':[custom('weapon_mastery',name='Weapon Mastery',count=fx(2),filter={'weapon_kind':'any','proficient':True},change_on='long_rest')],
 'Fighting Style':[custom('select_feat',name='Fighting Style (Paladin)',category='Fighting Style',alternatives=[idof('OPTION','Fighting Style','Blessed Warrior')],replace_on='level_up')],
 'Paladin’s Smite':[always('Divine Smite',list='Paladin',free_cast={'uses':1,'recharge':'long_rest'},also_with_slots=True)],
 'Channel Divinity':[res('channel divinity paladin','Channel Divinity',tab('channel_divinity'),['short_rest','long_rest'],regain={'short_rest':1,'long_rest':'all'}),grant(idof('OPTION','Paladin Channel Divinity','Divine Sense'),'Option'),custom('save_dc',id='channel divinity',formula='spell save DC (Paladin)')],
 'Paladin Subclass':[select('Archetype','Sacred Oath',supports='Paladin Subclass')],
 'Extra Attack':[extra_attack(1)],
 'Faithful Steed':[always('Find Steed',list='Paladin',free_cast={'uses':1,'recharge':'long_rest'},also_with_slots=True)],
 'Aura of Protection':[stat('saving throws:all','charisma:modifier',bonus='aura of protection',maximum=None) if False else custom('aura',id='aura of protection',radius_feet=10,bonus_saving_throws={'formula':'max(1, charisma:modifier)'},radius_by_level={'6':10,'18':30})],
 'Abjure Foes':[],
 'Aura of Courage':[custom('aura_immunity',aura='aura of protection',conditions=['frightened'])],
 'Radiant Strikes':[custom('extra_damage',name='Radiant Strikes',dice='1d8',trigger='melee weapon or unarmed strike hit',damage_type='radiant')],
 'Restoring Touch':[],
 'Aura Expansion':[],
}
p['subclass']=dict(feature_rules={
 'Oath of Devotion Spells':[always(s,lv,list='Paladin') for lv,ss in [(3,['Protection from Evil and Good','Shield of Faith']),(5,['Aid','Zone of Truth']),(9,['Beacon of Hope','Dispel Magic']),(13,['Freedom of Movement','Guardian of Faith']),(17,['Commune','Flame Strike'])] for s in ss],
 'Sacred Weapon':[custom('channel_divinity_option',name='Sacred Weapon',duration='10 minutes',attack_bonus='max(1, charisma:modifier)')],
 'Aura of Devotion':[custom('aura_immunity',aura='aura of protection',conditions=['charmed'])],
 'Smite of Protection':[],
 'Holy Nimbus':[res('holy nimbus','Holy Nimbus',fx(1),['long_rest'],alt_regain='expend a level 5 spell slot')]})
p['subopts']=[('Channel Divinity',3,'Paladin Channel Divinity',['Divine Sense'],{}),
              ('Fighting Style',2,'Fighting Style',['Blessed Warrior'],{'Blessed Warrior':[select('Spell','Blessed Warrior Cantrips',supports='Cleric, 0',number=2,spellcasting='Paladin',prepared=True,allow_replace=True)]})]
