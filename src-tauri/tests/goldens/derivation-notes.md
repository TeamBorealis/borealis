# Derivation notes (agent A)

These are the rule readings agent A made while deriving `expected/` by hand. Each cites the SRD section it rests on.
Agent B: read this file only **after** your own sheets are written.

## Both editions

1. **Fixed hit points.** Level 1 uses the maximum hit die. Each later level uses the fixed value: SRD 5.1 class
   "Hit Points" ("1d10 (or 6) + your Constitution modifier per ... level after 1st"); SRD 5.2.1 "Level Advancement > Fixed
   Hit Points by Class". A multiclass character gets level-1 hit points only from its first class (SRD 5.1
   "Multiclassing > Hit Points and Hit Dice"; SRD 5.2.1 "Multiclassing > Hit Points and Hit Dice").
2. **Con changes apply to every level.** A Constitution modifier change re-scores every level already gained (SRD 5.1
   "Using Ability Scores > Constitution > Hit Points"; SRD 5.2.1 "Gaining a Level", step 5). HP is therefore
   computed as Σ(level die values) + Con mod x total level.
3. **Proficiency bonus uses total character level**, including for multiclass characters (both "Multiclassing > Proficiency Bonus").
4. **Cantrip damage scales with total character level** (5 / 11 / 17), including for racial and feat cantrips
   (SRD 5.1 spell texts, e.g. Fire Bolt "This spell's damage increases by 1d10 when you reach 5th level"; SRD 5.2.1
   "Cantrip Upgrade"; SRD 5.2.1 "Multiclassing > Spellcasting > Cantrips").
5. **Spell save DC / attack bonus** = 8 + mod + PB / mod + PB, listed per casting source with that source's ability
   (SRD 5.1 "Spellcasting Ability" in each class; SRD 5.2.1 "Character Creation > Spellcasting"). A species trait or
   feat uses the ability the spec chose for it (SRD 5.2.1 Elven Lineage, Gnomish Lineage, Fiendish Legacy, Magic
   Initiate; SRD 5.1 High Elf Cantrip = Intelligence, Tiefling Infernal Legacy = Charisma).
6. **Passive Perception** = 10 + every modifier on Wisdom (Perception) checks (SRD 5.1 "Using Ability Scores > Passive
   Checks"; SRD 5.2.1 "Character Creation", Passive Perception formula). No character has a feature that changes it by
   a flat amount.
7. **Weapon attacks.** to-hit = ability mod + PB, and damage = die + the same mod (SRD 5.1 "Making an Attack > Modifiers
   to the Roll"; SRD 5.2.1 "Character Creation > Attacks"). Finesse uses the better of Str and Dex. Thrown melee
   weapons keep their melee ability (SRD 5.1 / 5.2.1 "Weapon properties: Thrown"). The Archery fighting style adds +2
   only to ranged weapons (SRD 5.1 Fighting Style: Archery; SRD 5.2.1 Archery feat).
8. **Situational damage is left out of attack lines** (README conventions). That covers Rage, Sneak Attack, Colossus
   Slayer, Hunter's Mark, Divine Smite, Sacred Weapon, Frenzy, Savage Attacker, Great Weapon Fighting and Hex. Bonuses
   that apply on every hit with no choice of resource are included: Improved Divine Smite, Agonizing Blast, Elemental
   Affinity and Martial Arts.
9. **Speeds list only always-on movement.** Dragon Wings (SRD 5.1 Draconic Bloodline), Draconic Flight and Large Form
   (SRD 5.2.1 Dragonborn, Goliath) need an action or a limited use, so they are listed as features, not speeds.
10. **Heavy armor and Str.** Each heavy-armor wearer meets the Str requirement (13 for chain mail), so there is no speed
    penalty (SRD 5.1 "Armor > Heavy Armor"; SRD 5.2.1 Armor table "Strength" column). Hill Dwarf speed is 25 and is not
    reduced by heavy armor in any case (SRD 5.1 Dwarf "Speed").

## SRD 5.1 (2014)

11. **Jack of All Trades applies to Initiative** (srd51-bard-3: +1). The 2014 feature covers "any ability check you make
    that doesn't already include your proficiency bonus", and Initiative is a Dexterity check (SRD 5.1 Bard "Jack of
    All Trades"; "Combat > Initiative"). It does not apply to saving throws.
12. **Remarkable Athlete applies to Initiative** and to every Str/Dex/Con skill without proficiency, at +ceil(PB/2) = +2 for
    srd51-fighter-11 (SRD 5.1 Champion "Remarkable Athlete": "half your proficiency bonus (round up) to any Strength,
    Dexterity, or Constitution check you make that doesn't already use your proficiency bonus"). Athletics is
    proficient, so it does not get the bonus.
13. **Aura of Protection counts for the paladin itself.** It adds Cha mod (+5) to all six saves of srd51-paladin-20
    (SRD 5.1 Paladin "Aura of Protection": "you or a friendly creature within 10 feet"; 30 feet at 18th level).
14. **Improved Divine Smite** (+1d8 radiant) is shown on every melee-weapon line, including the javelin, which is a
    melee weapon. The die is the same when the javelin is thrown (SRD 5.1 Paladin "Improved Divine Smite": "whenever
    you hit a creature with a melee weapon").
15. **Diamond Soul**: srd51-monk-17 is proficient in all saving throws (SRD 5.1 Monk "Diamond Soul").
16. **Monk weapons (2014)** are shortswords and simple melee weapons without two-handed or heavy. Darts are simple ranged
    weapons, so they get no Martial Arts die. They still use Dex because darts are finesse (SRD 5.1 Monk "Martial Arts";
    "Weapons" table).
17. **Ki save DC** = 8 + PB + Wis mod (SRD 5.1 Monk "Ki").
18. **Draconic Resilience (2014)** adds +1 HP per sorcerer level from level 1 (17 total), and unarmored AC is 13 + Dex
    (SRD 5.1 Draconic Bloodline "Draconic Resilience").
19. **Elemental Affinity (2014)** adds Cha mod to one damage roll of a spell of the ancestry's type. Fire Bolt is shown
    as 4d10+5 because red = fire (SRD 5.1 Draconic Bloodline "Elemental Affinity"; "Dragon Ancestor" table).
20. **Agonizing Blast** adds Cha mod to each Eldritch Blast beam. The number of beams follows character level: 2 at 5th
    (SRD 5.1 Eldritch Invocations "Agonizing Blast"; Eldritch Blast spell).
21. **Pact of the Tome cantrips** count as warlock cantrips and use Charisma (SRD 5.1 Pact Boon "Pact of the Tome").
22. **Multiclass spell slots (2014)**: paladin levels count half, **rounded down**. Paladin 2 gives caster level 1, so
    two 1st-level slots. Warlock levels do not count. Pact Magic slots (2 x 2nd level) are tracked separately (SRD 5.1
    "Multiclassing > Spellcasting > Spell Slots" and "Pact Magic").
23. **Multiclass proficiencies**: the Warlock multiclass grants light armor and simple weapons (the paladin already
    has both) and no skill. Saving throws come from the first class only (SRD 5.1 "Multiclassing > Proficiencies").
24. **Prepared counts**: Cleric / Druid / Wizard = mod + class level; Paladin = Cha mod + half paladin level rounded down.
    Domain, circle and oath spells are always prepared and do not count against the limit (SRD 5.1 class
    "Preparing and Casting Spells", "Domain Spells", "Circle Spells", "Oath Spells").
25. **Circle of the Land bonus cantrip** is a fourth druid cantrip at druid 8. Natural Recovery = half druid level,
    rounded up (4) (SRD 5.1 Circle of the Land).
26. **Wild Shape (2014)**: 2 uses, regained on a short or long rest; max CR 1 at 8th level with no movement limits
    (SRD 5.1 Druid "Wild Shape", Beast Shapes table).
27. **Favored Enemy: Beasts** gives no language, because beasts speak none (SRD 5.1 Ranger "Favored Enemy").
28. **Situational racial and background bonuses** stay out of skill totals: Stonecunning (History about stonework) and
    Artificer's Lore (History about magic items). A Lightfoot Halfling's Lucky and Naturally Stealthy are rerolls or
    conditions, not numbers (SRD 5.1 Dwarf, Rock Gnome, Halfling traits).
29. **Human (2014) has no named traits** besides Ability Score Increase, Speed and Languages, so its race feature list is empty.
30. **Score generation (2014).** SRD 5.1 has no rules for generating ability scores. The 2014 specs therefore use the
    standard array and the 27-point buy costs from SRD 5.2.1 "Character Creation > Step 3: Ability Scores > Generate Your Scores"
    (srd51-rogue-11: 8 = 0, 10 = 2, 14 = 7, 15 = 9). The assigned base scores are fixed inputs in each spec, so the
    sheet does not depend on how they were generated.

## SRD 5.2.1 (2024)

31. **Background ability increases** are +2 / +1 to two of the three listed scores, never above 20 (SRD 5.2.1
    "Character Backgrounds > Ability Scores").
32. **Jack of All Trades (2024) does not apply to Initiative**. It needs "an ability check ... that uses a skill
    proficiency you lack", and Initiative uses no skill (SRD 5.2.1 Bard "Level 2: Jack of All Trades").
33. **Alert** adds PB to Initiative. This applies when Alert comes from a background, from Human Versatile, or from
    Lessons of the First Ones (SRD 5.2.1 "Origin Feats: Alert"; Warlock "Lessons of the First Ones").
34. **Primal Order: Magician and Divine Order: Thaumaturge** add Wis mod (min +1) to every Intelligence (Arcana or Nature)
    / (Arcana or Religion) check. The bonus goes into those skill totals and stacks with proficiency and expertise
    (SRD 5.2.1 Druid "Level 1: Primal Order"; Cleric "Level 1: Divine Order").
35. **Primal Champion is applied last**: Str 21 (after Boon of Irresistible Offense) + 4 = 25, the cap; Con 20 + 4 = 24
    (SRD 5.2.1 Barbarian "Level 20: Primal Champion"; "Boon of Irresistible Offense", max 30).
36. **Barbarian Weapon Mastery is melee only**, so srd521-barbarian-20's shortbow gets none (SRD 5.2.1 Barbarian
    "Level 1: Weapon Mastery").
37. **Draconic Resilience (2024)** gives +3 HP at sorcerer 3 and +1 for each later sorcerer level (5 at sorcerer 5).
    Unarmored AC is 10 + Dex + Cha (SRD 5.2.1 Draconic Sorcery "Level 3: Draconic Resilience").
38. **Dwarven Toughness (2024)** is +1 HP per character level (SRD 5.2.1 Dwarf).
39. **Breath Weapon (2024)** DC = 8 + Con mod + PB. Damage is 1d10 below character level 5. Uses = PB per long rest
    (SRD 5.2.1 Dragonborn "Breath Weapon").
40. **Pact of the Blade (2024)** uses Cha for attack and damage with the bonded weapon, and grants proficiency with it
    (SRD 5.2.1 Warlock "Pact of the Blade"). Thirsting Blade gives 2 attacks with it.
41. **Monk weapons (2024)** are simple melee weapons and martial melee weapons with Light. The Martial Arts die (d8 at 5)
    replaces the weapon die: spear 1d6 / 1d8 versatile becomes 1d8, dagger 1d4 becomes 1d8 (SRD 5.2.1 Monk "Level 1:
    Martial Arts").
42. **Multiclass (2024)**: per-class prepared counts and cantrips; slots from the Multiclass Spellcaster table at
    wizard 5 + cleric 3 = 8. The Cleric multiclass gives Light and Medium armor and Shield training, which is why the
    leather + shield AC is valid, and no skill. Divine Order is a level 1 feature, so it is gained (SRD 5.2.1
    "Multiclassing > Spellcasting"; Cleric "As a Multiclass Character").
43. **Evocation Savant at wizard 5**: reaching level 3 slots "in this class" adds one free Evocation spell
    (Lightning Bolt). The multiclass caster level does not trigger it (SRD 5.2.1 Evoker "Level 3: Evocation Savant").
44. **Duplicate tool proficiency**: srd521-rogue-17 gets Thieves' Tools from both Rogue and Criminal. SRD 5.2.1 has no
    replacement rule for backgrounds, so no extra proficiency is taken.
45. **Purchased gear (option B)**: srd521-cleric-5 spends 95 of its 110 GP and the multiclass spends 25 of its 50 GP,
    at SRD 5.2.1 "Equipment" prices: Chain Mail 75 GP, Shield 10 GP, Mace 5 GP, Leather Armor 10 GP, Holy Symbol
    (Amulet) 5 GP.
46. **Uses tied to Proficiency Bonus** (Stonecunning, Adrenaline Rush, Giant Ancestry, Breath Weapon, Forest Gnome Speak
    with Animals) equal the current PB.
47. **Point buy** (srd521-warlock-8) uses SRD 5.2.1 "Generate Your Scores > Point Cost" and the Ability Score Point Costs table: 27 points; 8 = 0, 12 = 4, 14 = 7, 15 = 9.
