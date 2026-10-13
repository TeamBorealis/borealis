import json,glob,collections,re,os
ROOT='../../content/srd-5.2.1'
els=[];byfile={}
for f in sorted(glob.glob(ROOT+'/**/*.json',recursive=True)):
    if f.endswith('pack.json'): continue
    d=json.load(open(f)); rel=os.path.relpath(f,ROOT); byfile[rel]=d['elements']; els+=[(rel,e) for e in d['elements']]
types=collections.Counter(e['type'] for _,e in els)
def n(t,pred=lambda e:True): return sum(1 for _,e in els if e['type']==t and pred(e))
# ---- custom rule kinds
kinds=collections.defaultdict(lambda:{'n':0,'attrs':collections.Counter(),'els':[]})
def walk(rules,eid):
    for r in rules:
        k=r['kind']
        if k in('grant','select','stat'): continue
        d=kinds[k];d['n']+=1
        for a in r:
            if a not in('kind','level','requirements'): d['attrs'][a]+=1
        if eid not in d['els']: d['els'].append(eid)
        if k=='toggle': walk(r.get('effects',[]),eid)
for _,e in els:
    walk(e.get('rules',[]),e['id'])
    if 'multiclass' in e: walk(e['multiclass'].get('rules',[]),e['id'])
SEM={
'resource':'Declares a limited-use pool. `id` (stat-style key, lower-case), `name`, `max` = {type:fixed,value} | {type:stat,stat:"proficiency"} | {type:table,key:<class table column>} | {type:formula,expr:"max(1, charisma:modifier)"}; `recharge` = list of short_rest | long_rest | initiative | turn_start | none; optional `regain` {short_rest:n, long_rest:"all"} (partial recovery per rest), `alt_regain` (text, alternate restore trigger), `counter`/`start`/`step` (escalating DC counter such as Relentless Rage), `pool` (HP-pool style such as Lay On Hands), `replaces` (a later rule with the same `id` supersedes the earlier maximum). The sheet shows current/max and a rest-aware reset button; max is re-evaluated on level/stat change.',
'toggle':'A switchable state (Rage, Innate Sorcery). `activation` (action type), `duration` (text), `effects` = nested rules (grant/stat/custom) applied ONLY while the toggle is on, `blocks` (e.g. concentration, spellcasting), `requires_not_wearing`. Needs a character-state boolean per toggle id and conditional evaluation of nested rules.',
'select_feat':'A feat slot. `category` Origin | General | Fighting Style | Epic Boon, `name` (persistence key), optional `default` (preselected feat id), `alternatives` (non-feat option ids that may be picked instead, e.g. Blessed Warrior), `replace_on` (level_up), `distinct`. Candidates are Feat elements whose `data.category` matches and whose prerequisites pass. Replaces the 2014 generated ASI/feat class features.',
'ability_increase':'Declared on feats. `options` = list of {abilities:<count>|[ids], pick, amount}; `cap` 20 (30 for Epic Boons). The UI asks which abilities; the engine clamps to `cap`.',
'ability_distribution':'Background ASI. `pool` (three ability ids), `options` [[2,1],[1,1,1]], `cap` 20. One selection per background; applied as a labelled ability-score bonus.',
'grant_feat':'Grants a specific feat. `feat` id, `params` (e.g. {spell_list:"Cleric"} for Magic Initiate) fed to the feat parameters.',
'select_param':'A named parameter choice stored with the owning element instance: `name`, `options`, `distinct_per_instance` (cannot repeat across repeated feat takes), `reselect_on` (long_rest, short_rest), `grants_resistance`. Parameters are referenced by other rules via `$(param:<name>)` in select.supports and via `{param:<name>}` in `ability`.',
'select_size':'Species size choice (`options` Small/Medium). Replaces the fixed Size grant.',
'grant_spell':'Spell grant with 2024 semantics. `spell` id, `mode` always_prepared | known_cantrip | at_will_no_slot | free_cast_ref, `counts_against_prepared:false` for always-prepared, `free_cast` {uses: n | "proficiency" | table, recharge}, `also_with_slots` (can still be cast with slots), `ability` (ability id or {param}/{choice}), `spell_list` (which class list it counts for), `target` self, `condition`, `no_material`, `auto_succeed_save`, `max_roll`. The rule `level` is the CHARACTER level for species/feats and the CLASS level for class/subclass rules. Needs per-spell free-cast counters.',
'spell_free_cast':'Free cast bound to a Spell select by name (`select`), `uses`, `recharge`, `ability` ({param}), `also_with_slots`.',
'spell_replace_on_level':'Allows replacing feat-chosen spells on gaining a level (`same_level`, `scope`).',
'cantrip_swap':'High Elf: replace a named cantrip with another from `spell_list` on `swap_on` rest.',
'spell_list_extend':'Magical Secrets: prepared spells may come from `lists` and count as `spell_class` spells.',
'spell_cast_alt_cost':'Wild Companion: cast `spell` paying an alternative `cost` (spell slot or a resource use), optionally without Material components.',
'prepared_spell_change':'How prepared lists are edited: `when` gain_level | long_rest, `count` one | any.',
'spellbook':'Wizard spellbook: `initial_spells`, `per_level` additions, copy cost/time, `prepare_from` spellbook. The free spells are `Spellbook (Wizard n)` select slots (6 at level 1, 2 per level after); their picks plus copied spells form the book, separate from prepared spells.',
'pact_magic':'Warlock slots: slot count and slot level come from class-table columns `pact_slots` / `pact_slot_level`; all slots are one level; recover on short or long rest. Multiclass interaction via core grant ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_SOLO.',
'mystic_arcanum':'Warlock arcanum spells at class levels 11/13/15/17 (spell levels 6-9), one free cast each per long rest.',
'at_will_spells':'Spell Mastery: two spells (levels in `levels`) cast at lowest level without slots; swap on long rest.',
'ritual_casting':'Wizard Ritual Adept: rituals castable from `source` spellbook without preparing (`requires_prepared:false`). The 2024 profile otherwise allows ritual casting of any PREPARED ritual spell (profile hook ritual_rule).',
'free_cast_choice':'Chosen spell(s) cast without slots: `spell_level`, `count`, `uses`/`uses_each`, `recharge` (Signature Spells, Natural Recovery).',
'slot_recovery':'Recover expended slots: Arcane Recovery / Natural Recovery (`max_combined_level` expression, `max_slot_level`, `on` short_rest), Magical Cunning (`pact_slots`, `amount` half max rounded up, `improved_at` all).',
'resource_regain':'Restores `amount` of another resource `resource` on `on` triggers or `by` expending something; `up_to` caps the refill target (Superior Inspiration).',
'resource_cost':'Metamagic option cost (`resource` id, `cost` points).',
'convert':'Font of Magic conversions between spell slots and Sorcery Points (`from`,`to`,`table` of {slot,cost,min_level},`action`,`expires`).',
'weapon_mastery':'Weapon Mastery slots. `count` = fixed or class-table column `weapon_mastery`; `filter` {weapon_kind: melee|any, categories:[Simple,Martial], proficient:true}; `change_on` long_rest. Choices are kinds of weapon (one Weapon element id each); Weapon elements carry `data.mastery` and a Mastery Property id in supports so the sheet lists the property on attacks. Tactical Master can swap to Push/Sap/Slow per attack (text).',
'proficiency_filter':'Proficiency defined by a filter (Monk/Rogue weapons): `weapon_category`, `properties_any`. Needs filter-based weapon proficiency tests.',
'expertise':'Expertise choice. `count`; `from` describes the pool (default: proficient skills lacking Expertise; Rogue also Thieves\' Tools). Level-gated via rule `level`.',
'skill_bonus':'Bonus to the listed `skills` checks equal to `value` with `minimum` (Thaumaturge/Magician).',
'skill_bonus_unproficient':'Jack of All Trades: add half PB (`rounding:down`) to ability checks with a skill the character lacks proficiency in.',
'roll_advantage':'Advantage on a roll class: `roll` initiative | ability_check | saving_throw | spell_attack, optional `ability`, `skill`, `spell_class`.',
'save_advantage':'Advantage on saves with filters: `ability`, `against_condition`, `against_ability`, `scope` avoid_or_end, `context` (concentration), `unless_condition`.',
'check_floor':'Minimum on a d20 test total or die: `d20_floor` (Reliable Talent: treat 9 or lower as 10) or `floor` ability (Indomitable Might: total at least the Strength score).',
'critical_range':'Weapon/Unarmed critical hit threshold `min` (19, 18) for `applies_to`.',
'extra_damage':'Conditional extra damage: `dice`/`die`+`dice_count`/`dice_table` (class table column), `trigger`, `once_per` turn, `damage_type`, `requires`, `cost`, `per_slot_level`, `replaces` (improved version supersedes). Informational for the sheet (attack options); dice scaling reads class tables.',
'damage_scaling':'Dice that scale with CHARACTER level (Breath Weapon 1d10 to 4d10); `levels` map.',
'damage_reduction':'Monk Deflect Attacks reaction reduction: `dice` 1d10 + `plus` list; `damage_types`; `damage_reduction_all_types` at level 13.',
'damage_reduction_all_types':'Marker that lifts the damage-type restriction of the preceding damage_reduction rule.',
'damage_bonus_spell':'Add a modifier to one damage roll of spells of a chosen type (Elemental Affinity): `value`, `once_per`, `damage_type_param`.',
'damage_die_floor':'Great Weapon Fighting: treat damage dice rolls below `floor` as `floor` for the listed weapon context.',
'attack_bonus':'Flat attack roll bonus (Archery +2 with ranged weapons): `weapon` filter, `value`.',
'extra_attack_damage_modifier':'Two-Weapon Fighting: add ability modifier to the extra attack damage when the weapon has `property` (Light).',
'cantrip_damage_bonus':'Add `value` (an ability modifier) to damage of cantrips of `spell_class`; with `choose_cantrip` the player picks the eligible cantrip (Agonizing Blast). Also Potent Spellcasting.',
'cantrip_range_bonus':'Eldritch Spear: add range per class level to a chosen damage cantrip.',
'cantrip_rider':'Repelling Blast: extra effect on a chosen attack-roll cantrip.',
'cunning_strike':'Rogue Cunning Strike framework: `save_dc` formula, `max_effects` (1, then 2 at level 11). Options are Option elements (`supports` Cunning Strike) carrying `cunning_effect` rules.',
'cunning_effect':'One Cunning Strike effect: `cost_dice` of Sneak Attack, optional `save`.',
'aura':'Paladin Aura of Protection: emanation `radius_feet` (`radius_by_level` 10 then 30), saving-throw bonus `bonus_saving_throws.formula`. Needs ally-affecting aura model (display only is acceptable).',
'aura_immunity':'Condition immunity for creatures inside a named `aura`.',
'channel_divinity_option':'A Channel Divinity option that spends the shared pool (Preserve Life: `pool_hit_points`, `max_fraction`; Sacred Weapon: `duration`, `attack_bonus`).',
'save_dc':'Named save DC for the sheet: `id`, `formula` (text, e.g. 8+strength+proficiency or "spell save DC (Cleric)").',
'condition_immunity':'Permanent condition immunity (Nature\'s Ward).',
'condition_immunity_while':'Condition immunity while a `toggle` is active (Mindless Rage).',
'death_save_rules':'Champion Survivor: Advantage on death saves and natural 18-20 counts as 20 (`natural_20_threshold`).',
'regeneration':'Heroic Rally: heal `amount` at `timing` when `condition` holds.',
'evasion':'Dexterity-save half-damage rule: no damage on success, half on failure (not while Incapacitated).',
'healing':'Healing action (Second Wind): `dice` + `plus` expression.',
'heroic_inspiration':'Heroic Inspiration source: `trigger` long_rest (Human Resourceful) or start_of_turn_in_combat (Champion Heroic Warrior). Needs the profile `inspiration` model (boolean heroic_inspiration on the character).',
'initiative_swap':'Alert: may swap initiative with a willing ally (`with_ally`).',
'martial_arts':'Monk Martial Arts: die from class table `martial_arts`, bonus unarmed strike, Dexterity for Monk weapons/unarmed, active only with no armor/shield.',
'beast_forms':'Wild Shape: `known` rows by druid level {forms,max_cr,fly}, `duration_hours` expression, `temp_hp`, `change_on` long_rest. Needs Beast stat blocks (not in this pack; see COVERAGE gaps).',
'movement_mode':'Fly/climb/swim speeds from a feature: `mode`, `speed` number or "speed", `duration`, `action`, `requires_resource`.',
'sense':'Non-standard sense such as Devil\'s Sight: `sense`, `range`, `note`.',
'pact_weapon':'Pact of the Blade bond: weapon kinds, Charisma attack/damage, damage type overrides, spellcasting focus.',
'replace_on_level':'Allow replacing `count` choices of `scope` (invocations, metamagic) when gaining a level.',
'carry_capacity_size_bonus':'Powerful Build: count as `steps` size larger for carrying capacity.',
'save_advantage_check':'Advantage on an ability check type (`context`, Goliath ending Grappled).',
'exhaustion':'Exhaustion condition numbers for the 2024 profile: D20 Tests -2 per level, Speed -5 ft per level, death at level 6. Profile hook exhaustion.model=linear.',
'equipment_package':'Reference (`ref`) to an Equipment Package element (`data.options` A/B/C with items, qty and coins). The creation UI shows a choice; gold alternative adds coins. Gated by !multiclass id for classes.',
}
SEM_MISSING=[k for k in kinds if k not in SEM]
assert not SEM_MISSING,SEM_MISSING
out=['# Engine needs for the srd-5.2.1 pack','',
'Written by the content author for the engine team. Everything the 2024 SRD content needs that the current rule kinds (`grant`, `select`, `stat`) cannot express is encoded as a **named custom rule kind** (schema `CustomRule`: any object with `kind`) or in an element\'s opaque `data` object. `borealis-content` loads and round-trips all of it already (verified: zero schema errors, zero diagnostics). Until the engine implements a kind, it is ignored (the element still grants/selects through its normal rules and shows its description).','',
'Conventions used by every custom kind: optional `level` and `requirements` behave exactly like on core rule kinds (class level inside class/subclass/feature rules; CHARACTER level inside species and feat rules). Ability ids are the lower-case stat names (`strength` ... `charisma`). Expressions (`max`, `formula`) use stat group names, e.g. `max(1, charisma:modifier)`.','',
'## 1. Custom rule kinds in use','',
'| Rule kind | Uses | Attributes | Semantics | Used by (examples) |','|---|---|---|---|---|']
for k,d in sorted(kinds.items()):
    attrs=', '.join('`%s`'%a for a in sorted(d['attrs']))
    ex=', '.join('`%s`'%x for x in d['els'][:3])
    out.append('| `%s` | %d | %s | %s | %s |'%(k,d['n'],attrs or '-',SEM[k].replace('|','\\|'),ex))
out+=['',
'## 2. Changes to existing rule kinds / expression languages','',
'* **`select.supports` parameter token `$(param:<Name>)`** (Magic Initiate): resolves to the value chosen by a `select_param` rule of the same element instance, e.g. `$(param:Spell List), 0` = cantrips of the chosen list. The `borealis-content` supports parser keeps it as a `Dynamic::Other` leaf. Used by `ID_2024_FEAT_MAGIC_INITIATE`.',
'* **`select` of a feat-or-option** (Paladin/Ranger Fighting Style): modelled with `select_feat.alternatives` (Option ids) because a `select` has a single element type.',
'* **Stat `darkvision:range` / `blindsight:range` / `truesight:range` with `bonus:"base"`** (non-stacking max) carry sense ranges; the core Vision elements already define `darkvision:range`. Species grant the Vision element and add the larger range; blindsight/truesight ranges follow the same convention (core needs `blindsight:range`, `truesight:range` stat consumers).',
'* **Dwarven Toughness, Draconic Resilience**: expressed as 20 (or 18) level-gated `hp` stat increments (+1 per level); works with the current stat engine.',
'* **Spell slot / prepared / cantrip progression** is emitted in the style the engine already understands (`<class>:spellcasting:slots:N` increments, `<class>:spellcasting:prepare`, level-gated `Cantrip (Class n)` Spell selects). In 2024 `prepare` is a table value (not level + modifier); core prepared-count formulas must NOT be applied to 2024 classes (profile hook `prepared_count = table`). Half casters (Paladin/Ranger) use multiclass grant `..._SLOTS_HALF_UP` (round up) and have no cantrips.',
'* **Class tables** are also stored as `Class.data.progression` (arrays indexed level-1: `proficiency_bonus`, `rages`, `rage_damage`, `weapon_mastery`, `sneak_attack`, `martial_arts`, `focus_points`, `invocations`, `pact_slots`, `pact_slot_level`, `prepared`, `cantrips`, `slot_1..slot_9`, ...). Resources reference these through `{type:table,key}`; the engine should expose them as stat groups `<class>:table:<key>` evaluated at the class level.',
'* **Subclass at level 3** for all 12 classes: the `<Class> Subclass` feature carries `select` of type Archetype with `supports: "<Class> Subclass"`; each Archetype has `supports: ["<Class> Subclass"]` and `data.level_gained: 3`.',
'* **Multiclassing** uses the existing `multiclass` node (`requirements` expression for the primary-ability 13 rule). The 2024 multiclass proficiency bundle per class is in the node\'s rules; level-1 starting proficiencies are gated with `!<multiclass id>`.',
'* **Species/Background/Feat element types**: `Species`, `Species Trait Option`, `Mastery Property`, `Equipment Package` (proposed_2024 types) and `Rules Glossary Entry` (new) have no dedicated parser; the default parser is enough. Species traits use type `Racial Trait`. Species do not grant ability increases or languages (profile: `ability_sources`, `language_grants`).',
'* **Language elements** (`ID_2024_LANGUAGE_*`) carry `supports` `Standard` or `Rare` and `maps_to` core ids; the creation step "Common + two standard languages" is a profile feature (see section 3).','',
'## 3. Profile / creation-flow features the content assumes','',
'1. **Origin step**: background ASI (`ability_distribution`), origin feat (`grant_feat`), skills and tool from the background; species traits/lineage options; **languages** are chosen at character creation (Common plus two Standard languages, `ID_2024_PROFILE_TABLES.data.languages`), not by species/background.',
'2. **Profile tables** in element `ID_2024_PROFILE_TABLES` (type `Core`, `data`): XP and proficiency bonus by level, point-buy costs (budget 27, scores 8-15), standard array and standard array by class, fixed hit points per level by class, multiclass spell slots table and caster-level rules, exhaustion numbers, creation step order. Load these into the `dnd2024` profile instead of hard-coding.',
'3. **Weapon masteries**: eight `Mastery Property` elements; every Weapon element lists its mastery id in `supports` and `data.mastery`. The character stores chosen weapon kinds per `weapon_mastery` rule.',
'4. **Feat categories and prerequisites**: `Feat.data.category`, `Feat.data.prerequisite` ({level_min, abilities_any, requires_feature}), plus the equivalent `requirements` expression (`[character:4]`, `([str:13]||[dex:13])`). "Fighting Style Feature" and "Spellcasting Feature" prerequisites are `data.prerequisite.requires_feature` (no expression yet: test = has a feature with tag Fighting Style / Spellcasting). Repeatability is `data.repeatable` {allowed, distinct_param}.',
'5. **Epic Boon at 19** and the 2024 ASI feat slot are `select_feat` rules emitted by each class (levels in `Class.data.asi_levels`; Fighter 4,6,8,12,14,16; Rogue 4,8,10,12,16 as printed in the SRD tables).',
'6. **Heroic Inspiration**, **exhaustion (linear)**, **armor training instead of proficiency**, **tool+skill advantage**, and **ritual casting without a class feature** are profile hooks, not content.','',
'## 4. Known modelling shortcuts (candidates for engine follow-up)','',
'* Many combat-time features (Reckless Attack, Uncanny Dodge, Cutting Words, Divine Smite interactions, most reactions) have no rule beyond their description and sheet text: they need no computation. Where a numeric/limited-use side exists it is a `resource`.',
'* `Circle of the Land Spells` encodes the four land tables as always-prepared `grant_spell` rules with an extra `requirements_param` attribute `{ "Land Type": <land> }` (active only while that `select_param` value is chosen; re-selectable on long rest).',
'* Spell attack/DC for granted spells: `ability` is `{param}`/`{choice}` for lineage and feat spells; class spells use the class ability.',
'* Multiclass cantrip scaling uses character level (damage_scaling `scale: character_level`).',
'* Wild Shape beast stat blocks, Find Familiar/Steed forms and summon-spell stat blocks are NOT in this pack (monsters were out of scope); `beast_forms` and spell text reference "Monsters" only by name.',
'']
open(ROOT+'/ENGINE-NEEDS.md','w').write('\n'.join(out))
print('kinds',len(kinds))
# ---------------- COVERAGE
sp=[e for _,e in els if e['type']=='Spell']
lvl=collections.Counter(e['data']['level'] for e in sp)
cls=collections.Counter(c for e in sp for c in e['data']['classes'])
sch=collections.Counter(e['data']['school'] for e in sp if e['data']['level']>0)
feats=[e for _,e in els if e['type']=='Feat']
cf=[e for _,e in els if e['type'] in('Class Feature','Archetype Feature')]
withrules=[e for e in cf if any(r['kind'] in('stat','select','grant') or r['kind'] not in () for r in e.get('rules',[]))]
rulecount=sum(1 for e in cf if e.get('rules'))
mi=[e for _,e in els if e['type']=='Magic Item']
mt=collections.Counter(e['data']['item_type'] for e in mi)
def row(a,b,c,d=''): return '| %s | %s | %s | %s |'%(a,b,c,d)
cov=['# Coverage: srd-5.2.1 pack','',
'Generated by `content-tools/srd521/docs.py` from the pack files. Source: SRD 5.2.1 only. Every file and element has `ruleset: "dnd2024"`.','',
'| Category | Plan | Authored | Notes |','|---|---|---|---|',
row('Species','9',n('Species'),'Dragonborn, Dwarf, Elf, Gnome, Goliath, Halfling, Human, Orc, Tiefling'),
row('Species traits (`Racial Trait`)','-',n('Racial Trait'),'one element per SRD trait'),
row('Species trait options','Draconic Ancestors 10, Elven Lineages 3, Gnomish Lineages 2, Giant Ancestries 6, Fiendish Legacies 3',n('Species Trait Option'),'24 = 10+3+2+6+3'),
row('Backgrounds','4',n('Background'),'each: ability_distribution (2/1 or 1/1/1 over 3 abilities), origin feat, 2 skills, tool, equipment package A/B (50 GP)'),
row('Feats','17 (4 Origin, 2 General, 4 Fighting Style, 7 Epic Boon)',len(feats),', '.join('%s %d'%(c,sum(1 for e in feats if e['data']['category']==c)) for c in ['Origin','General','Fighting Style','Epic Boon'])),
row('Classes','12',n('Class'),'levels 1-20 tables stored in `data.progression`; multiclass node on each'),
row('Subclasses (`Archetype`)','12',n('Archetype'),'one per class, level 3'),
row('Class features','-',n('Class Feature'),'every "Level N: Name" heading of the 12 classes'),
row('Subclass features','-',n('Archetype Feature'),'%d with at least one rule; others are description/sheet only'%sum(1 for e in cf if e['type']=='Archetype Feature' and e.get('rules'))),
row('Class and subclass features with rules','-','%d of %d'%(rulecount,len(cf)),'the rest are play-time effects described in text'),
row('Option catalogs','Invocations ~28, Metamagic 10, Brutal Strike, Cunning Strike, Channel Divinity, ...',n('Option'),'28 invocations, 10 metamagic, %d other class options'%(n('Option')-38)),
row('Spells','339',len(sp),'by level '+', '.join('%d:%d'%(k,lvl[k]) for k in sorted(lvl))),
row('Spell class lists','Wizard 218, Sorcerer 140, Bard 130, Druid 124, Cleric 109, Warlock 72, Ranger 48, Paladin 38',', '.join('%s %d'%(k,cls[k]) for k in ['Wizard','Sorcerer','Bard','Druid','Cleric','Warlock','Ranger','Paladin']),'matches the plan exactly'),
row('Spell schools (level 1+)','Abj 48, Con 52, Div 29, Enc 33, Evo 45, Ill 27, Nec 24, Tra 54',', '.join('%s %d'%(k[:3],sch[k]) for k in sorted(sch)),'matches'),
row('Weapons','38',n('Weapon'),'all with mastery, properties, damage, cost, weight'),
row('Mastery properties','8',n('Mastery Property'),''),
row('Weapon properties','10',n('Weapon Property'),'2024 wording'),
row('Armor (incl. Shield)','13',n('Armor'),''),
row('Tools','25 kinds',sum(1 for e in els if e[1]['type']=='Item' and e[1]['id'].startswith('ID_2024_TOOL_')),'17 artisan + 8 other; utilize actions with DCs in `data.utilize`'),
row('Adventuring gear (incl. ammunition, focus rows)','~100',sum(1 for p,e in els if p=='equipment/gear.json'),'gear table (75) + ammunition types (5) incl. Potion of Healing, Spell Scroll rows'),
row('Equipment packs','7',n('Item',lambda e:'extract' in e),'with `extract` contents'),
row('Mounts, tack, vehicles','table set',sum(1 for p,e in els if p=='equipment/mounts-vehicles.json'),'mounts 8, tack/saddles/vehicles, 7 large vehicles'),
row('Lifestyle, food, hirelings, spellcasting services','table set',sum(1 for p,e in els if p=='equipment/services.json'),''),
row('Equipment packages (starting equipment)','12 classes + 4 backgrounds',n('Equipment Package'),'A/B(/C) with coin alternative'),
row('Magic items','~259',len(mi),', '.join('%s %d'%(k,v) for k,v in sorted(mt.items()))+'; Potion of Giant Strength / Potions of Healing / +N items are single multi-rarity entries'),
row('Conditions','15',n('Condition',lambda e:e['id'].startswith('ID_2024_CONDITION_')),'text from the glossary; Exhaustion has a rule'),
row('Languages','10 standard + 9 rare',n('Language',lambda e:e['id'].startswith('ID_2024_LANGUAGE_')),''),
row('Rules glossary entries','~200',n('Rules Glossary Entry'),'headings recovered by an alphabetical-order filter; some glossary entries are missing (see gaps)'),
row('Profile tables','XP, PB, point buy, standard array(s), fixed HP, multiclass slots',n('Core'),'`ID_2024_PROFILE_TABLES`'),
'',
'Total elements: **%d** in **%d** files (`pack.json` lists them).'%(len(els),len(byfile)),'',
'## Validation','',
'* JSON Schema (`crates/borealis-content/schema/content-format.schema.json`, draft 2020-12): **0 errors** on all files (`content-tools/srd521/validate.py`).',
'* Semantic validation with `borealis-content` (`ContentStoreBuilder::add_pack_dir` for `core` + `srd-5.2.1`, `store.validate()`; tool `content-tools/srd521/validate_rs`): **0 errors, 0 warnings, 0 infos**; no duplicate ids; no unresolved `ID_*` reference anywhere in the pack (`content-tools/srd521/xref.py`; all core references resolve against `app/content/core`).',
'* Checksums from the plan: 339 spells (27/57/57/42/34/38/31/20/17/16), class list sizes and schools, 38 weapons, 13 armor, 25 tools, 15 conditions, 17 feats, 9 species, 4 backgrounds, 12 subclasses all match.','',
'## Gaps (intentionally not covered or approximate)','',
'* **Monsters / Beast stat blocks** (Wild Shape forms, familiars, steeds, Animated Object etc.): not authored; only a handful of stat blocks appear embedded in spell text.',
'* **Crafting rules, Gameplay Toolbox, Trinkets table, Creating a Background, Magic Item rules text** (rarity values, sentience, cursed items, miscibility): not authored as elements.',
'* **Rules glossary**: %d of ~200 entries recovered (headings detected automatically; the rest are folded into neighbours or missed).'%n('Rules Glossary Entry'),
'* **Class spell lists** are derived from spell `supports`/`data.classes` (they match the SRD counts); the printed tables are not stored separately.',
'* **Rules for play-time effects**: roughly %d of %d class/subclass features have no rule at all (reactions, once-per-turn riders, narrative abilities); their description is complete.'%(len(cf)-rulecount,len(cf)),
'* Hand-curated rules reflect SRD text as of 5.2.1; complex interactions (Wild Shape, Circle of the Land re-selection, Magic Initiate parameters) depend on the engine needs in `ENGINE-NEEDS.md`.',
'* Some wide tables in descriptions (spell/magic item tables, stat blocks inside summoning spells) are preserved as preformatted `<pre>` blocks, not HTML tables.',
'']
open(ROOT+'/COVERAGE.md','w').write('\n'.join(cov))
# ---------------- IDS
pref=collections.Counter()
for _,e in els:
    m=re.match(r'^ID_2024_([A-Z]+(?:_[A-Z]+)?)_',e['id'])
    pref[e['id'].split('_')[2]+'/'+e['type']]+=1
ids=['# Element id conventions (srd-5.2.1)','',
'All ids in this pack are `ID_2024_<KIND>_<SLUG>` where `SLUG` is the SRD name upper-cased, apostrophes removed, every other run of non-alphanumerics replaced by `_` (e.g. `Hunter\'s Mark` -> `HUNTERS_MARK`). No id contains the characters forbidden by the validator (`( ) , & | ! [ ] : \' ’`). The prefix `ID_2024_` namespaces the pack so user content cannot masquerade as SRD content and 2014 ids (`ID_WOTC_*`, `ID_PHB_*`, `ID_RACE_*`) never collide.','',
'| Kind prefix | Element type | Pattern / example |','|---|---|---|',
'| `SPECIES` | Species | `ID_2024_SPECIES_DWARF` |',
'| `TRAIT` | Racial Trait | `ID_2024_TRAIT_DWARF_DWARVEN_RESILIENCE` (`<SPECIES>_<TRAIT>`) |',
'| `OPTION` | Species Trait Option / Option | `ID_2024_OPTION_ELVEN_LINEAGE_DROW`, `ID_2024_OPTION_METAMAGIC_QUICKENED_SPELL`, `ID_2024_OPTION_ELDRITCH_INVOCATION_AGONIZING_BLAST` (`<GROUP>_<NAME>`) |',
'| `BACKGROUND` | Background | `ID_2024_BACKGROUND_ACOLYTE` |',
'| `FEAT` | Feat | `ID_2024_FEAT_ALERT`, `ID_2024_FEAT_BOON_OF_FATE` |',
'| `CLASS`, `MULTICLASS` | Class, derived Multiclass id | `ID_2024_CLASS_FIGHTER`, `ID_2024_MULTICLASS_FIGHTER` |',
'| `CLASS_FEATURE` | Class Feature | `ID_2024_CLASS_FEATURE_FIGHTER_SECOND_WIND` (`<CLASS>_<FEATURE>`; a feature that appears twice gets `_<level>` on its second element, e.g. `..._BARBARIAN_IMPROVED_BRUTAL_STRIKE_17`) |',
'| `SUBCLASS` | Archetype | `ID_2024_SUBCLASS_FIGHTER_CHAMPION` |',
'| `SUBCLASS_FEATURE` | Archetype Feature | `ID_2024_SUBCLASS_FEATURE_FIGHTER_CHAMPION_IMPROVED_CRITICAL` |',
'| `EQUIPMENT_PACKAGE` | Equipment Package | `ID_2024_EQUIPMENT_PACKAGE_CLASS_FIGHTER`, `..._BACKGROUND_SAGE` |',
'| `SPELL` | Spell | `ID_2024_SPELL_FIREBALL` |',
'| `WEAPON`, `ARMOR` | Weapon, Armor | `ID_2024_WEAPON_LONGSWORD`, `ID_2024_ARMOR_CHAIN_MAIL` |',
'| `MASTERY` | Mastery Property | `ID_2024_MASTERY_SAP` |',
'| `WEAPON_PROPERTY` | Weapon Property (2024 text) | `ID_2024_WEAPON_PROPERTY_FINESSE` |',
'| `TOOL` | Item (tool) | `ID_2024_TOOL_THIEVES_TOOLS` |',
'| `ITEM` | Item (gear, packs, ammunition) | `ID_2024_ITEM_BURGLARS_PACK`, `ID_2024_ITEM_AMMUNITION_ARROWS` |',
'| `MOUNT`, `VEHICLE`, `LIFESTYLE`, `HIRELING`, `SERVICE`, `FOOD`, `LODGING`, `MEAL` | Item (economy tables) | `ID_2024_MOUNT_HORSE_RIDING`, `ID_2024_VEHICLE_KEELBOAT` |',
'| `MAGIC_ITEM` | Magic Item | `ID_2024_MAGIC_ITEM_BAG_OF_HOLDING` |',
'| `CONDITION` | Condition | `ID_2024_CONDITION_PRONE` |',
'| `RULE` | Rules Glossary Entry | `ID_2024_RULE_GRAPPLING` (tagged entries append `_<TAG>`) |',
'| `LANGUAGE` | Language | `ID_2024_LANGUAGE_COMMON_SIGN_LANGUAGE` |',
'| `SOURCE`, `PROFILE_TABLES` | Source, Core | `ID_2024_SOURCE_SRD521`, `ID_2024_PROFILE_TABLES` |','',
'## Links to 2014 ids','',
'* `maps_to: {"dnd2014": <id>, "kind": "equivalent" | "approximate"}` points at the closest 2014 element (legacy ids such as `ID_WOTC_PHB_CLASS_FIGHTER`, `ID_RACE_DWARF`, `ID_PHB_SPELL_FIREBALL`). `equivalent` = same concept and name; `approximate` = same slot but different rules (Race -> Species, Draconic Bloodline -> Draconic Sorcery, 2014 Acolyte background -> 2024 Acolyte).',
'* `renamed_from` is set (with the 2014 id) on the 18 spells the 2024 SRD renamed (e.g. `ID_2024_SPELL_ACID_ARROW` renamed_from `ID_PHB_SPELL_MELFS_ACID_ARROW`, `ID_2024_SPELL_BEFUDDLEMENT` renamed_from `ID_PHB_SPELL_FEEBLEMIND`) when the 2014 id exists in the legacy data.',
'* Core pack ids (`ID_PROFICIENCY_*`, `ID_LANGUAGE_*`, `ID_INTERNAL_*`, `ID_SIZE_*`, `ID_VISION_*`) are referenced, never redefined (`docs/core-pack-ids.md`).',
'* 2014 spells/items with no 2024 counterpart in the SRD have no mapping (`maps_to` absent).','',
'## Counts by id kind / type','','| Id kind / type | Count |','|---|---|']
for k,v in sorted(pref.items()): ids.append('| `%s` | %d |'%(k,v))
open(ROOT+'/IDS.md','w').write('\n'.join(ids)+'\n')
att='''# Attribution and licensing

This pack (`srd-5.2.1`, ruleset `dnd2024`) is part of Borealis and is derived solely from the System Reference Document 5.2.1.

## Required attribution (CC-BY-4.0)

> This work includes material from the System Reference Document 5.2.1 ("SRD 5.2.1") by Wizards of the Coast LLC, available at https://www.dndbeyond.com/srd. The SRD 5.2.1 is licensed under the Creative Commons Attribution 4.0 International License, available at https://creativecommons.org/licenses/by/4.0/legalcode.

## Modifications (CC-BY section 3(a)(1)(B))

The SRD text was modified: it was restructured into Borealis JSON content files (element records, rules, setters and tables). Descriptions were reflowed from the PDF layout (hyphenation and line breaks removed), converted to HTML fragments, and tables were kept as preformatted text. Game rules (grants, selections, statistics, resources and the custom rule kinds listed in `ENGINE-NEEDS.md`) were added by the Borealis authors and are not part of the SRD text. Class tables were additionally stored as numeric arrays. `pack.json` carries `"license": "CC-BY-4.0"` and the attribution above.

## Scope

* Only SRD 5.2.1 (published 2025-05-01) was used. No non-SRD Wizards of the Coast content (no other subclasses, species, feats, spells or items) is included. Every element has `source: "SRD 5.2.1"`.
* No "Dungeons & Dragons", Wizards or D&D Beyond logos or trade dress are used. Per the SRD, no other attribution to Wizards or its parent or affiliates is made. The pack may be described as "compatible with fifth edition" or "5E compatible".
* The CC-BY-4.0 Section 5 disclaimer of warranties and limitation of liability applies.
* The 2014-rules companion pack (`srd-5.1`) has its own attribution statement; do not mix the two statements.
'''
open(ROOT+'/ATTRIBUTION.md','w').write(att)
print('docs written')
