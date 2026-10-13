# Element id conventions (srd-5.2.1)

All ids in this pack are `ID_2024_<KIND>_<SLUG>` where `SLUG` is the SRD name upper-cased, apostrophes removed, every other run of non-alphanumerics replaced by `_` (e.g. `Hunter's Mark` -> `HUNTERS_MARK`). No id contains the characters forbidden by the validator (`( ) , & | ! [ ] : ' ’`). The prefix `ID_2024_` namespaces the pack so user content cannot masquerade as SRD content and 2014 ids (`ID_WOTC_*`, `ID_PHB_*`, `ID_RACE_*`) never collide.

| Kind prefix | Element type | Pattern / example |
|---|---|---|
| `SPECIES` | Species | `ID_2024_SPECIES_DWARF` |
| `TRAIT` | Racial Trait | `ID_2024_TRAIT_DWARF_DWARVEN_RESILIENCE` (`<SPECIES>_<TRAIT>`) |
| `OPTION` | Species Trait Option / Option | `ID_2024_OPTION_ELVEN_LINEAGE_DROW`, `ID_2024_OPTION_METAMAGIC_QUICKENED_SPELL`, `ID_2024_OPTION_ELDRITCH_INVOCATION_AGONIZING_BLAST` (`<GROUP>_<NAME>`) |
| `BACKGROUND` | Background | `ID_2024_BACKGROUND_ACOLYTE` |
| `FEAT` | Feat | `ID_2024_FEAT_ALERT`, `ID_2024_FEAT_BOON_OF_FATE` |
| `CLASS`, `MULTICLASS` | Class, derived Multiclass id | `ID_2024_CLASS_FIGHTER`, `ID_2024_MULTICLASS_FIGHTER` |
| `CLASS_FEATURE` | Class Feature | `ID_2024_CLASS_FEATURE_FIGHTER_SECOND_WIND` (`<CLASS>_<FEATURE>`; a feature that appears twice gets `_<level>` on its second element, e.g. `..._BARBARIAN_IMPROVED_BRUTAL_STRIKE_17`) |
| `SUBCLASS` | Archetype | `ID_2024_SUBCLASS_FIGHTER_CHAMPION` |
| `SUBCLASS_FEATURE` | Archetype Feature | `ID_2024_SUBCLASS_FEATURE_FIGHTER_CHAMPION_IMPROVED_CRITICAL` |
| `EQUIPMENT_PACKAGE` | Equipment Package | `ID_2024_EQUIPMENT_PACKAGE_CLASS_FIGHTER`, `..._BACKGROUND_SAGE` |
| `SPELL` | Spell | `ID_2024_SPELL_FIREBALL` |
| `WEAPON`, `ARMOR` | Weapon, Armor | `ID_2024_WEAPON_LONGSWORD`, `ID_2024_ARMOR_CHAIN_MAIL` |
| `MASTERY` | Mastery Property | `ID_2024_MASTERY_SAP` |
| `WEAPON_PROPERTY` | Weapon Property (2024 text) | `ID_2024_WEAPON_PROPERTY_FINESSE` |
| `TOOL` | Item (tool) | `ID_2024_TOOL_THIEVES_TOOLS` |
| `ITEM` | Item (gear, packs, ammunition) | `ID_2024_ITEM_BURGLARS_PACK`, `ID_2024_ITEM_AMMUNITION_ARROWS` |
| `MOUNT`, `VEHICLE`, `LIFESTYLE`, `HIRELING`, `SERVICE`, `FOOD`, `LODGING`, `MEAL` | Item (economy tables) | `ID_2024_MOUNT_HORSE_RIDING`, `ID_2024_VEHICLE_KEELBOAT` |
| `MAGIC_ITEM` | Magic Item | `ID_2024_MAGIC_ITEM_BAG_OF_HOLDING` |
| `CONDITION` | Condition | `ID_2024_CONDITION_PRONE` |
| `RULE` | Rules Glossary Entry | `ID_2024_RULE_GRAPPLING` (tagged entries append `_<TAG>`) |
| `LANGUAGE` | Language | `ID_2024_LANGUAGE_COMMON_SIGN_LANGUAGE` |
| `SOURCE`, `PROFILE_TABLES` | Source, Core | `ID_2024_SOURCE_SRD521`, `ID_2024_PROFILE_TABLES` |

## Links to 2014 ids

* `maps_to: {"dnd2014": <id>, "kind": "equivalent" | "approximate"}` points at the closest 2014 element (Aurora/legacy ids such as `ID_WOTC_PHB_CLASS_FIGHTER`, `ID_RACE_DWARF`, `ID_PHB_SPELL_FIREBALL`). `equivalent` = same concept and name; `approximate` = same slot but different rules (Race -> Species, Draconic Bloodline -> Draconic Sorcery, 2014 Acolyte background -> 2024 Acolyte).
* `renamed_from` is set (with the 2014 id) on the 18 spells the 2024 SRD renamed (e.g. `ID_2024_SPELL_ACID_ARROW` renamed_from `ID_PHB_SPELL_MELFS_ACID_ARROW`, `ID_2024_SPELL_BEFUDDLEMENT` renamed_from `ID_PHB_SPELL_FEEBLEMIND`) when the 2014 id exists in the legacy data.
* Core pack ids (`ID_PROFICIENCY_*`, `ID_LANGUAGE_*`, `ID_INTERNAL_*`, `ID_SIZE_*`, `ID_VISION_*`) are referenced, never redefined (`docs/core-pack-ids.md`).
* 2014 spells/items with no 2024 counterpart in the SRD have no mapping (`maps_to` absent).

## Counts by id kind / type

| Id kind / type | Count |
|---|---|
| `ARMOR/Armor` | 13 |
| `BACKGROUND/Background` | 4 |
| `CLASS/Class` | 12 |
| `CLASS/Class Feature` | 174 |
| `CONDITION/Condition` | 15 |
| `EQUIPMENT/Equipment Package` | 16 |
| `FEAT/Feat` | 17 |
| `FOOD/Item` | 5 |
| `HIRELING/Item` | 3 |
| `ITEM/Item` | 97 |
| `LANGUAGE/Language` | 19 |
| `LIFESTYLE/Item` | 7 |
| `LODGING/Item` | 6 |
| `MAGIC/Magic Item` | 258 |
| `MASTERY/Mastery Property` | 8 |
| `MEAL/Item` | 6 |
| `MOUNT/Item` | 8 |
| `OPTION/Option` | 77 |
| `OPTION/Species Trait Option` | 24 |
| `PROFILE/Core` | 1 |
| `RULE/Rules Glossary Entry` | 137 |
| `SERVICE/Item` | 7 |
| `SOURCE/Source` | 1 |
| `SPECIES/Species` | 9 |
| `SPELL/Spell` | 339 |
| `SUBCLASS/Archetype` | 12 |
| `SUBCLASS/Archetype Feature` | 58 |
| `TOOL/Item` | 25 |
| `TRAIT/Racial Trait` | 33 |
| `VEHICLE/Item` | 7 |
| `WEAPON/Weapon` | 38 |
| `WEAPON/Weapon Property` | 10 |
