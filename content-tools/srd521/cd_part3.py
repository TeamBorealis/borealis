from cd_part2 import *
# ---------------------------------------------------------------- Ranger
r=defclass('Ranger',hd=10,primary=['dexterity','wisdom'],primary_mode='all',saves=['strength','dexterity'],
  skills=(3,['Animal Handling','Athletics','Insight','Investigation','Nature','Perception','Stealth','Survival']),weapons=[SIMPLE,MARTIAL],armor=[LIGHT,MED,SHIELDS],tools=[],
  multiclass=dict(requirements='([dex:13]&&[wis:13])',prereq='Dexterity 13 and Wisdom 13',grants=[select('Proficiency','Skill Proficiency (Ranger Multiclass)',supports=skill_range(['Animal Handling','Athletics','Insight','Investigation','Nature','Perception','Stealth','Survival'])),grant(MARTIAL,'Proficiency'),grant(LIGHT,'Proficiency'),grant(MED,'Proficiency'),grant(SHIELDS,'Proficiency')]),
  spell=dict(ability='Wisdom',known=True,prepare=True,replace='long_rest',list='Ranger',slots='ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_HALF_UP',focus='ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_DRUIDIC_FOCUS',focus_name='Druidic Focus',cantrips=False))
r['features']={
 'Favored Enemy':[always("Hunter’s Mark" if False else 'Hunters Mark',list='Ranger',free_cast={'uses':tab('favored_enemy'),'recharge':'long_rest'},also_with_slots=True)],
 'Weapon Mastery':[custom('weapon_mastery',name='Weapon Mastery',count=fx(2),filter={'weapon_kind':'any','proficient':True},change_on='long_rest')],
 'Deft Explorer':[custom('expertise',count=1,from_='proficient skills without Expertise'),select('Language','Deft Explorer Languages',supports='Standard||Rare',number=2)],
 'Fighting Style':[custom('select_feat',name='Fighting Style (Ranger)',category='Fighting Style',alternatives=[idof('OPTION','Fighting Style','Druidic Warrior')],replace_on='level_up')],
 'Ranger Subclass':[select('Archetype','Ranger Conclave',supports='Ranger Subclass')],
 'Extra Attack':[extra_attack(1)],
 'Roving':[stat('speed','10',equipped='![armor:heavy]',alt='Roving'),stat('speed:climb','speed',requirements=None),stat('speed:swim','speed')],
 'Expertise':[expertise(2)],
 'Tireless':[res('tireless','Tireless',frm('max(1, wisdom:modifier)'),['long_rest'])],
 'Relentless Hunter':[],
 'Nature’s Veil':[res('natures veil','Nature’s Veil',frm('max(1, wisdom:modifier)'),['long_rest'])],
 'Precise Hunter':[],
 'Feral Senses':[grant('ID_VISION_BLINDSIGHT','Vision'),stat('blindsight:range','30',bonus='base')],
 'Foe Slayer':[],
}
r['subclass']=dict(feature_rules={
 'Hunter’s Lore':[],'Hunter’s Prey':[select('Option','Hunter’s Prey',supports='Hunters Prey')],
 'Defensive Tactics':[select('Option','Defensive Tactics',supports='Defensive Tactics')],
 'Superior Hunter’s Prey':[],'Superior Hunter’s Defense':[]})
r['subopts']=[('Fighting Style',2,'Fighting Style',['Druidic Warrior'],{'Druidic Warrior':[select('Spell','Druidic Warrior Cantrips',supports='Druid, 0',number=2,spellcasting='Ranger',prepared=True,allow_replace=True)]})]
r['subopts_sub']=[('Hunter’s Prey',3,'Hunters Prey',['Colossus Slayer','Horde Breaker'],{}),('Defensive Tactics',7,'Defensive Tactics',['Escape the Horde','Multiattack Defense'],{})]

# ---------------------------------------------------------------- Rogue
g=defclass('Rogue',hd=8,primary=['dexterity'],saves=['dexterity','intelligence'],
  skills=(4,['Acrobatics','Athletics','Deception','Insight','Intimidation','Investigation','Perception','Persuasion','Sleight of Hand','Stealth']),
  weapons=[SIMPLE,custom('proficiency_filter',weapon_category='Martial',properties_any=['Finesse','Light'],name='Martial weapons that have the Finesse or Light property')],armor=[LIGHT],
  tools=[grant('ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS','Proficiency')],
  multiclass=dict(requirements='[dex:13]',prereq='Dexterity 13',grants=[select('Proficiency','Skill Proficiency (Rogue Multiclass)',supports=skill_range(['Acrobatics','Athletics','Deception','Insight','Intimidation','Investigation','Perception','Persuasion','Sleight of Hand','Stealth'])),grant('ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS','Proficiency'),grant(LIGHT,'Proficiency')]),spell=None)
g['features']={
 'Expertise':[expertise(2),expertise(2,level=6)],
 'Sneak Attack':[custom('extra_damage',name='Sneak Attack',dice_table='sneak_attack',once_per='turn',requires='advantage, or ally within 5 feet of target and no disadvantage',weapon='Finesse or Ranged',damage_type='weapon')],
 'Thieves’ Cant':[grant('ID_WOTC_PHB_LANGUAGE_THIEVES_CANT','Language'),select('Language','Thieves’ Cant Extra Language',supports='Standard||Rare')],
 'Weapon Mastery':[custom('weapon_mastery',name='Weapon Mastery',count=fx(2),filter={'weapon_kind':'any','proficient':True},change_on='long_rest')],
 'Cunning Action':[],
 'Rogue Subclass':[select('Archetype','Roguish Archetype',supports='Rogue Subclass')],
 'Steady Aim':[],
 'Cunning Strike':[custom('cunning_strike',save_dc='8+dexterity+proficiency',max_effects=1),grant(idof('OPTION','Cunning Strike','Poison'),'Option'),grant(idof('OPTION','Cunning Strike','Trip'),'Option'),grant(idof('OPTION','Cunning Strike','Withdraw'),'Option')],
 'Uncanny Dodge':[],
 'Evasion':[custom('evasion')],
 'Reliable Talent':[custom('check_floor',applies_to=['ability checks with skill or tool proficiency'],d20_floor=10)],
 'Improved Cunning Strike':[custom('cunning_strike',max_effects=2)],
 'Devious Strikes':[grant(idof('OPTION','Cunning Strike','Daze'),'Option'),grant(idof('OPTION','Cunning Strike','Knock Out'),'Option'),grant(idof('OPTION','Cunning Strike','Obscure'),'Option')],
 'Slippery Mind':[grant('ID_PROFICIENCY_SAVINGTHROW_WISDOM','Proficiency'),grant('ID_PROFICIENCY_SAVINGTHROW_CHARISMA','Proficiency')],
 'Elusive':[],
 'Stroke of Luck':[res('stroke of luck','Stroke of Luck',fx(1),['short_rest','long_rest'])],
}
g['asi_levels']=[4,8,10,12,16]
g['subclass']=dict(feature_rules={
 'Fast Hands':[],'Second-Story Work':[stat('speed:climb','speed')],
 'Supreme Sneak':[grant(idof('OPTION','Cunning Strike','Stealth Attack'),'Option')],
 'Use Magic Device':[stat('attunement:max','1')],
 'Thief’s Reflexes':[]})
g['subopts']=[('Cunning Strike',5,'Cunning Strike',['Poison','Trip','Withdraw'],{'Poison':[custom('cunning_effect',cost_dice=1,save='constitution')],'Trip':[custom('cunning_effect',cost_dice=1,save='dexterity')],'Withdraw':[custom('cunning_effect',cost_dice=1)]}),
              ('Devious Strikes',14,'Cunning Strike',['Daze','Knock Out','Obscure'],{'Daze':[custom('cunning_effect',cost_dice=2,save='constitution')],'Knock Out':[custom('cunning_effect',cost_dice=6,save='constitution')],'Obscure':[custom('cunning_effect',cost_dice=3,save='dexterity')]})]
g['subopts_sub']=[('Supreme Sneak',9,'Cunning Strike',['Stealth Attack'],{'Stealth Attack':[custom('cunning_effect',cost_dice=1)]})]

# ---------------------------------------------------------------- Sorcerer
s=defclass('Sorcerer',hd=6,primary=['charisma'],saves=['constitution','charisma'],
  skills=(2,['Arcana','Deception','Insight','Intimidation','Persuasion','Religion']),weapons=[SIMPLE],armor=[],tools=[],
  multiclass=dict(requirements='[cha:13]',prereq='Charisma 13',grants=[]),
  spell=dict(ability='Charisma',known=True,prepare=True,replace='level',list='Sorcerer',slots=SPELLSLOTS_FULL,focus='ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_ARCANE_FOCUS',focus_name='Arcane Focus'))
s['features']={
 'Innate Sorcery':[res('innate sorcery','Innate Sorcery',fx(2),['long_rest']),toggle('innate sorcery','Innate Sorcery','bonus_action','1 minute',[stat('spell save dc:sorcerer','1',bonus='innate sorcery'),adv('spell_attack',spell_class='Sorcerer')])],
 'Font of Magic':[res('sorcery points','Sorcery Points',tab('sorcery_points'),['long_rest']),custom('convert',from_='spell slot',to='sorcery points',amount='slot level',action='none'),
                  custom('convert',from_='sorcery points',to='spell slot',table=[{'slot':1,'cost':2,'min_level':2},{'slot':2,'cost':3,'min_level':3},{'slot':3,'cost':5,'min_level':5},{'slot':4,'cost':6,'min_level':7},{'slot':5,'cost':7,'min_level':9}],action='bonus_action',expires='long_rest')],
 'Metamagic':[select('Option','Metamagic (Sorcerer 2)',supports='Metamagic',number=2,level=2),select('Option','Metamagic (Sorcerer 10)',supports='Metamagic',number=2,level=10),select('Option','Metamagic (Sorcerer 17)',supports='Metamagic',number=2,level=17),custom('replace_on_level',scope='metamagic',count=1)],
 'Sorcerer Subclass':[select('Archetype','Sorcerous Origin',supports='Sorcerer Subclass')],
 'Sorcerous Restoration':[res('sorcerous restoration','Sorcerous Restoration',fx(1),['long_rest']),custom('resource_regain',resource='sorcery points',amount='floor(sorcerer level / 2)',on=['short_rest'])],
 'Sorcery Incarnate':[],
 'Arcane Apotheosis':[],
}
s['subclass']=dict(feature_rules={
 'Draconic Resilience':[hp_stat(3,alt='Draconic Resilience')]+[hp_stat(1,level=l,alt='Draconic Resilience') for l in range(4,21)]+ac_alt('draconic resilience','Draconic Resilience',['dexterity','charisma']),
 'Draconic Spells':[always(sp_,lv,list='Sorcerer') for lv,ss in [(3,['Alter Self','Chromatic Orb','Command','Dragon’s Breath']),(5,['Fear','Fly']),(7,['Arcane Eye','Charm Monster']),(9,['Legend Lore','Summon Dragon'])] for sp_ in ss],
 'Elemental Affinity':[select('Option','Elemental Affinity Damage Type',supports='Dragon Damage Type'),custom('damage_bonus_spell',value='charisma:modifier',once_per='spell',damage_type_param='Elemental Affinity Damage Type')],
 'Dragon Wings':[res('dragon wings','Dragon Wings',fx(1),['long_rest'],alt_regain='spend 3 Sorcery Points'),custom('movement_mode',mode='fly',speed=60,duration='1 hour',action='bonus_action')],
 'Dragon Companion':[always('Summon Dragon',list='Sorcerer',free_cast={'uses':1,'recharge':'long_rest'},also_with_slots=True,no_material=True)]})

# ---------------------------------------------------------------- Warlock
w=defclass('Warlock',hd=8,primary=['charisma'],saves=['wisdom','charisma'],
  skills=(2,['Arcana','Deception','History','Intimidation','Investigation','Nature','Religion']),weapons=[SIMPLE],armor=[LIGHT],tools=[],
  multiclass=dict(requirements='[cha:13]',prereq='Charisma 13',grants=[grant(LIGHT,'Proficiency')]),
  spell=dict(ability='Charisma',known=True,prepare=True,replace='level',list='Warlock',slots='ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_SOLO',pact=True,focus='ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_ARCANE_FOCUS',focus_name='Arcane Focus'))
w['features']={
 'Eldritch Invocations':[select('Option','Eldritch Invocation (Warlock 1)',supports='Eldritch Invocation',number=1)]+
    [select('Option','Eldritch Invocation (Warlock %d)'%l,supports='Eldritch Invocation',number=n,level=l) for l,n in [(2,2),(5,2),(6,1),(7,1),(9,1),(12,1),(15,1),(18,1)]]+
    [custom('replace_on_level',scope='eldritch invocations',count=1)],
 'Pact Magic':[],
 'Magical Cunning':[res('magical cunning','Magical Cunning',fx(1),['long_rest']),custom('slot_recovery',pact_slots=True,amount='half of maximum, rounded up',cast_time='1 minute',improved_at={'level':20,'amount':'all'})],
 'Warlock Subclass':[select('Archetype','Otherworldly Patron',supports='Warlock Subclass')],
 'Contact Patron':[always('Contact Other Plane',list='Warlock',free_cast={'uses':1,'recharge':'long_rest'},also_with_slots=True,auto_succeed_save=True)],
 'Mystic Arcanum':[select('Spell','Mystic Arcanum (level 6)',supports='Warlock, 6',level=11,spellcasting='Warlock',prepared=True),select('Spell','Mystic Arcanum (level 7)',supports='Warlock, 7',level=13,spellcasting='Warlock',prepared=True),
                   select('Spell','Mystic Arcanum (level 8)',supports='Warlock, 8',level=15,spellcasting='Warlock',prepared=True),select('Spell','Mystic Arcanum (level 9)',supports='Warlock, 9',level=17,spellcasting='Warlock',prepared=True),
                   custom('mystic_arcanum',slots=[{'spell_level':6,'level':11},{'spell_level':7,'level':13},{'spell_level':8,'level':15},{'spell_level':9,'level':17}],free_cast={'uses':1,'recharge':'long_rest'})],
 'Eldritch Master':[],
}
w['subclass']=dict(feature_rules={
 'Dark One’s Blessing':[],
 'Fiend Spells':[always(sp_,lv,list='Warlock') for lv,ss in [(3,['Burning Hands','Command','Scorching Ray','Suggestion']),(5,['Fireball','Stinking Cloud']),(7,['Fire Shield','Wall of Fire']),(9,['Geas','Insect Plague'])] for sp_ in ss],
 'Dark One’s Own Luck':[res('dark ones own luck','Dark One’s Own Luck',frm('max(1, charisma:modifier)'),['long_rest'])],
 'Fiendish Resilience':[custom('select_param',name='Fiendish Resilience Damage Type',options=['Acid','Bludgeoning','Cold','Fire','Lightning','Necrotic','Piercing','Poison','Psychic','Radiant','Slashing','Thunder'],reselect_on=['short_rest','long_rest'],grants_resistance=True)],
 'Hurl Through Hell':[res('hurl through hell','Hurl Through Hell',fx(1),['long_rest'],alt_regain='expend a Pact Magic spell slot')]})

# ---------------------------------------------------------------- Wizard
z=defclass('Wizard',hd=6,primary=['intelligence'],saves=['intelligence','wisdom'],
  skills=(2,['Arcana','History','Insight','Investigation','Medicine','Nature','Religion']),weapons=[SIMPLE],armor=[],tools=[],
  multiclass=dict(requirements='[int:13]',prereq='Intelligence 13',grants=[]),
  spell=dict(ability='Intelligence',known=False,prepare=True,replace='long_rest',list='Wizard',slots=SPELLSLOTS_FULL,spellbook=True,focus='ID_INTERNAL_PROFICIENCY_SPELLFOCUS_GROUP_ARCANE_FOCUS',focus_name='Arcane Focus'))
z['features']={
 'Ritual Adept':[grant('ID_INTERNAL_RITUAL_CASTING','Grants'),custom('ritual_casting',source='spellbook',requires_prepared=False)],
 'Arcane Recovery':[res('arcane recovery','Arcane Recovery',fx(1),['long_rest']),custom('slot_recovery',on='short_rest',max_combined_level='ceil(wizard level / 2)',max_slot_level=5)],
 'Scholar':[skill_sel('Scholar',['Arcana','History','Investigation','Medicine','Nature','Religion']),custom('expertise',count=1,from_='Scholar skill choice')],
 'Wizard Subclass':[select('Archetype','Arcane Tradition',supports='Wizard Subclass')],
 'Memorize Spell':[],
 'Spell Mastery':[select('Spell','Spell Mastery (level 1)',supports='Wizard, 1',level=18,spellcasting='Wizard',prepared=True),select('Spell','Spell Mastery (level 2)',supports='Wizard, 2',level=18,spellcasting='Wizard',prepared=True),custom('at_will_spells',levels=[1,2],casting_time='action',swap_on='long_rest')],
 'Signature Spells':[select('Spell','Signature Spell 1',supports='Wizard, 3',level=20,spellcasting='Wizard',prepared=True),select('Spell','Signature Spell 2',supports='Wizard, 3',level=20,spellcasting='Wizard',prepared=True),custom('free_cast_choice',spell_level=3,count=2,uses_each=1,recharge=['short_rest','long_rest'])],
}
z['subclass']=dict(feature_rules={
 'Evocation Savant':[select('Spell','Evocation Savant',supports='Wizard, Evocation, (0||1||2)',number=2,spellcasting='Wizard')],
 'Potent Cantrip':[],'Sculpt Spells':[],'Empowered Evocation':[],'Overchannel':[]})
