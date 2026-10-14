#!/usr/bin/env python3
"""Generate app/content/core/*.json (the Borealis CORE rules pack).

Usage: python3 -I gen_core_pack.py <migrated-fixture-dir> <out-dir>

<migrated-fixture-dir> is the output of `borealis-migrate` over the (never committed)
legacy id fixtures (maintainer-local, not included). It is read ONLY for identifiers, names, tag lists and catalogue metadata
(ids must match so imported characters resolve). All rules of the "engine-semantic" families
(levels, base grants, multiclass tables, skills, saves, expertise, ASI, templates, options)
are generated here from the engine rules; descriptions are our own text.
"""
import glob, itertools, json, os, sys

SRC, OUT = sys.argv[1], sys.argv[2]
FMT = "borealis-content/1"
ABIL = ["strength", "dexterity", "constitution", "intelligence", "wisdom", "charisma"]
ABBR = ["str", "dex", "con", "int", "wis", "cha"]
SKILLS = [("ACROBATICS", "Acrobatics", "dexterity"), ("ANIMALHANDLING", "Animal Handling", "wisdom"),
          ("ARCANA", "Arcana", "intelligence"), ("ATHLETICS", "Athletics", "strength"),
          ("DECEPTION", "Deception", "charisma"), ("HISTORY", "History", "intelligence"),
          ("INSIGHT", "Insight", "wisdom"), ("INTIMIDATION", "Intimidation", "charisma"),
          ("INVESTIGATION", "Investigation", "intelligence"), ("MEDICINE", "Medicine", "wisdom"),
          ("NATURE", "Nature", "intelligence"), ("PERCEPTION", "Perception", "wisdom"),
          ("PERFORMANCE", "Performance", "charisma"), ("PERSUASION", "Persuasion", "charisma"),
          ("RELIGION", "Religion", "intelligence"), ("SLEIGHTOFHAND", "Sleight of Hand", "dexterity"),
          ("STEALTH", "Stealth", "dexterity"), ("SURVIVAL", "Survival", "wisdom")]
XP = [0, 300, 900, 2700, 6500, 14000, 23000, 34000, 48000, 64000, 85000, 100000, 120000,
      140000, 165000, 195000, 225000, 265000, 305000, 355000]
# Multiclass caster-level -> spell slots (standard multiclass table; cumulative rules derived below).
SLOTS = {1: [2], 2: [3], 3: [4, 2], 4: [4, 3], 5: [4, 3, 2], 6: [4, 3, 3], 7: [4, 3, 3, 1],
         8: [4, 3, 3, 2], 9: [4, 3, 3, 3, 1], 10: [4, 3, 3, 3, 2], 11: [4, 3, 3, 3, 2, 1],
         12: [4, 3, 3, 3, 2, 1], 13: [4, 3, 3, 3, 2, 1, 1], 14: [4, 3, 3, 3, 2, 1, 1],
         15: [4, 3, 3, 3, 2, 1, 1, 1], 16: [4, 3, 3, 3, 2, 1, 1, 1],
         17: [4, 3, 3, 3, 2, 1, 1, 1, 1], 18: [4, 3, 3, 3, 3, 1, 1, 1, 1],
         19: [4, 3, 3, 3, 3, 2, 1, 1, 1], 20: [4, 3, 3, 3, 3, 2, 2, 1, 1]}


def load(name):
    p = glob.glob(os.path.join(SRC, "*core." + name + ".json"))
    assert len(p) == 1, name
    return json.load(open(p[0]))["elements"]


def el(id_, name, ty, source="Core", **kw):
    e = {"id": id_, "name": name, "type": ty, "source": source}
    e.update({k: v for k, v in kw.items() if v not in (None, [], "")})
    return e


def stat(name, value, **kw):
    r = {"kind": "stat", "name": name, "value": str(value)}
    r.update({k: v for k, v in kw.items() if v is not None})
    return r


def grant(id_, ty="Grants", **kw):
    r = {"kind": "grant", "id": id_, "type": ty}
    r.update(kw)
    return r


def clean(e, source=None):
    """Copy catalogue metadata, dropping prose and book-specific fields."""
    out = {k: v for k, v in e.items() if k in ("id", "name", "type", "supports", "setters", "rules", "requirements", "spellcasting")}
    out["source"] = source or ("Internal" if e["source"] == "Internal" else "Core")
    if "setters" in out:
        out["setters"] = [s for s in out["setters"] if s["name"] not in ("short",)]
        if not out["setters"]:
            del out["setters"]
    return out


files = {}

# ---- levels and base grants -------------------------------------------------------------------
levels = []
for n in range(1, 21):
    rules = []
    if n in (1, 5, 9, 13, 17):
        rules.append(stat("proficiency", 2 if n == 1 else 1, alt=f"Level {n}"))
    if n == 1:
        rules += [grant(i) for i in ("ID_INTERNAL_GRANTS_CHARACTER_BASE", "ID_INTERNAL_GRANTS_SPELLCASTING_BASE",
                                     "ID_INTERNAL_GRANTS_ARMOR_CLASS_BASE", "ID_INTERNAL_GRANTS_ARMOR_CLASS_DEXTERITY_MODIFIER",
                                     "ID_INTERNAL_GRANTS_HP_CONSTITUTION_MODIFIER")]
        rules += [{"kind": "select", "type": t, "name": t} for t in ("Race", "Class", "Background", "Alignment")]
        rules.append({"kind": "select", "type": "Deity", "name": "Deity", "optional": True})
    else:
        rules.append({"kind": "select", "type": "Multiclass", "name": f"Multiclass (Level {n})", "level": n,
                      "requirements": f"ID_INTERNAL_GRANTS_MULTICLASSING_PREREQUISITE&&ID_INTERNAL_MULTICLASS_LEVEL_{n}"})
    levels.append(el(f"ID_LEVEL_{n}", str(n), "Level", "Internal",
                     setters=[{"name": "Level", "value": str(n)}, {"name": "Experience", "value": str(XP[n - 1])}],
                     rules=rules, description=f"Character level {n}."))
base = []
for a in ABIL:
    base.append(stat(f"{a}:max", 20))
base += [stat("attunement:max", 3, bonus="base"),
         stat("ac:unarmored:base", 10, alt="Base"),
         stat("ac:unarmored:dexterity", "dexterity:modifier", alt="Dexterity Modifier"),
         stat("ac:unarmored", "ac:unarmored:base", alt="Initial"),
         stat("ac:unarmored", "ac:unarmored:dexterity", alt="Dexterity Modifier"),
         stat("ac:calculation", "ac:unarmored", bonus="calculation", equipped="[armor:none]", alt="Unarmored", merge=True),
         stat("ac:armored:dexterity:cap", 2, bonus="base"),
         stat("ac:armored:misc", 0), stat("ac:armored:enhancement", 0),
         stat("ac:armored:dexterity", "dexterity:modifier", bonus="modifier", equipped="[armor:light]"),
         stat("ac:armored:dexterity", "dexterity:modifier", bonus="modifier", equipped="[armor:medium]", maximum="ac:armored:dexterity:cap"),
         stat("ac:armored:dexterity", 0, bonus="modifier", equipped="[armor:heavy]")]
for part in ("armor", "dexterity", "enhancement", "misc"):
    base.append(stat("ac:armored", f"ac:armored:{part}", equipped="[armor:any]"))
base += [stat("ac:calculation", "ac:armored", bonus="calculation", equipped="[armor:any]", alt="Armored", merge=True),
         stat("ac:dexterity", "dexterity:modifier", bonus="base:modifier", equipped="[armor:none]"),
         stat("ac:dexterity", "dexterity:modifier", bonus="base:modifier", equipped="[armor:light]"),
         stat("ac:dexterity", "dexterity:modifier", bonus="base:modifier", equipped="[armor:medium]", maximum="ac:armored:dexterity:cap"),
         stat("ac:dexterity", 0, bonus="base:modifier", equipped="[armor:heavy]")]
for mv in ("", ":climb", ":fly", ":swim", ":burrow"):
    base += [stat(f"innate speed{mv}:calculation", f"innate speed{mv}", merge=True),
             stat(f"innate speed{mv}:calculation", f"innate speed{mv}:misc", merge=True)]
for mv in ("", ":climb", ":fly", ":swim", ":burrow"):
    base += [stat(f"speed{mv}", f"innate speed{mv}:calculation", bonus="base", alt="Innate", merge=True),
             stat(f"speed{mv}", f"speed{mv}:misc", alt="Misc", merge=True)]
spell = [stat("spellcasting:attack:proficiency", "proficiency", bonus="proficiency"),
         stat("spellcasting:dc:base", 8, bonus="base"),
         stat("spellcasting:dc:proficiency", "proficiency", bonus="proficiency"),
         stat("spellcasting:attack", "spellcasting:attack:proficiency", alt="Proficiency"),
         stat("spellcasting:dc", "spellcasting:dc:base", alt="Base"),
         stat("spellcasting:dc", "spellcasting:dc:proficiency", alt="Proficiency"),
         stat("spellcasting:attack:misc", 0), stat("spellcasting:dc:misc", 0),
         stat("spellcasting:attack", "spellcasting:attack:misc"), stat("spellcasting:dc", "spellcasting:dc:misc")]
for kind in ("attack", "dc"):
    for ab in ABBR:
        spell.append(stat(f"spellcasting:{kind}:{ab}:misc", 0))
    for ab, full in zip(ABBR, ABIL):
        spell += [stat(f"spellcasting:{kind}:{ab}", f"spellcasting:{kind}"),
                  stat(f"spellcasting:{kind}:{ab}", f"{full}:modifier", bonus="ability", alt="Ability Modifier"),
                  stat(f"spellcasting:{kind}:{ab}", f"spellcasting:{kind}:{ab}:misc")]
# the attack block lists all misc stats first, then dc; regroup to keep one block per kind
grants_core = [
    el("ID_INTERNAL_GRANTS_CHARACTER_BASE", "Base", "Grants", "Internal", rules=base, description="Baseline statistics every character has: ability caps, armor class plumbing, speeds, attunement limit."),
    el("ID_INTERNAL_GRANTS_ARMOR_CLASS_BASE", "Base Armor Class", "Grants", "Internal", description="Placeholder for armor class additions."),
    el("ID_INTERNAL_GRANTS_SPELLCASTING_BASE", "Spellcasting Base", "Grants", "Internal", rules=spell, description="Generic spell save DC and spell attack statistics."),
    el("ID_INTERNAL_GRANTS_ARMOR_CLASS_DEXTERITY_MODIFIER", "Dexterity Modifier", "Grants", "Internal",
       rules=[stat("initiative", "dexterity:modifier", bonus="ability")], description="Initiative uses the Dexterity modifier."),
    el("ID_INTERNAL_GRANTS_HP_CONSTITUTION_MODIFIER", "Constitution Modifier", "Grants", "Internal",
       rules=[stat("hp", "constitution:modifier", level=n) for n in range(1, 21)],
       description="Constitution modifier is added to hit points once per character level."),
    el("ID_INTERNAL_IGNORE_AC_DEXTERITY_MODIFIER", "Ignore ac:dexterity", "Ignore", "Internal", description="Marker kept for content compatibility."),
]
files["core-levels.json"] = levels + grants_core

# ---- multiclassing -----------------------------------------------------------------------------
mc = [el("ID_INTERNAL_OPTION_ALLOW_MULTICLASSING", "Multiclassing", "Option", "Internal",
         setters=[{"name": "default", "value": "true"}, {"name": "locked", "value": "true"},
                  {"name": "short", "value": "Allows gaining levels in more than one class."}],
         rules=[grant("ID_INTERNAL_GRANTS_MULTICLASS_SPELLCASTING", requirements="ID_INTERNAL_GRANTS_MULTICLASSING_PREREQUISITE")]),
      el("ID_INTERNAL_GRANTS_MULTICLASSING_PREREQUISITE", "Multiclassing Prerequisite", "Grants", "Internal",
         setters=[{"name": "allow duplicate", "value": "false"}], description="Granted when the main class allows taking a second class.")]
for n in range(2, 21):
    mc.append(el(f"ID_INTERNAL_MULTICLASS_LEVEL_{n}", f"Multiclassing (Level {n})", "Grants", "Internal",
                 setters=[{"name": "level", "value": "2"}] if n == 2 else None))
tbl = []
prev = [0] * 9
for cl in range(1, 21):
    row = SLOTS[cl] + [0] * (9 - len(SLOTS[cl]))
    for i in range(9):
        d = row[i] - prev[i]
        if d > 0:
            tbl.append(stat(f"multiclass:spellcasting:slot:{i + 1}", d, level=cl))
    prev = row
mc.append(el("ID_INTERNAL_GRANTS_MULTICLASS_SPELLCASTING", "Multiclass Spellcasting", "Grants", "Internal", rules=tbl,
             description="Spell slots by combined caster level (cumulative rules)."))
mc.append(el("ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_SOLO", "Multiclass Spellcasting Slots Solo", "Grants", "Internal"))


def caster_levels(kind):
    out = []
    for L in range(1, 21):
        c = {"FULL": True, "HALF": L % 2 == 0, "HALF_UP": L % 2 == 1, "THIRD": L % 3 == 0, "THIRD_UP": L % 3 == 2 and L >= 2,
             "FOURTH": L % 4 == 0, "FOURTH_UP": L % 4 == 2, "FIFTH": L % 5 == 0}[kind]
        if c:
            out.append(L)
    return out


for kind in ("FULL", "HALF", "THIRD", "FOURTH", "FIFTH", "HALF_UP", "THIRD_UP", "FOURTH_UP"):
    mc.append(el(f"ID_INTERNAL_GRANT_MULTICLASS_SPELLCASTING_SLOTS_{kind}",
                 "Multiclass Spellcasting Slots " + kind.title().replace("_", " "), "Grants", "Internal",
                 rules=[stat("multiclass:spellcasting:level", 1, level=L) for L in caster_levels(kind)]))
files["multiclass.json"] = mc

# ---- internal options, supports, templates, marker grants ------------------------------------
internal = [
    el("ID_INTERNAL_OPTION_ALLOW_FEATS", "Feats", "Option", "Internal", setters=[{"name": "default", "value": "true"}, {"name": "short", "value": "Feats may replace ability score improvements."}]),
    el("ID_INTERNAL_OPTION_ALLOW_AVERAGE_HP", "Average Hit Points", "Option", "Internal", setters=[{"name": "default", "value": "false"}, {"name": "short", "value": "Hit points use the fixed average value per level."}]),
    el("ID_INTERNAL_TEMPLATE_CLASS_FEATURE_ABILITY_4", "Ability Score Improvement", "Class Feature",
       rules=[{"kind": "select", "type": "Ability Score Improvement", "name": "Ability Score Increase (Level 4)", "number": 2, "supports": "Ability Score Improvement&&Class"}],
       description="Template copied for each class and improvement level."),
    el("ID_INTERNAL_TEMPLATE_CLASS_FEATURE_FEAT_4", "Feat", "Class Feature", requirements="ID_INTERNAL_OPTION_ALLOW_FEATS",
       setters=[{"name": "allow duplicate", "value": "true"}], rules=[{"kind": "select", "type": "Feat", "name": "Feat (Level 4)"}],
       description="Template for taking a feat instead of an ability score improvement."),
]
for e in load("internal.core-internal-grants") + load("internal.internal-supports"):
    internal.append(clean(e) | {"description": f"{e['name']} (internal marker)."})
files["internal.json"] = internal

# ---- abilities: ASI elements, saves, skills, expertise, workarounds -----------------------------
asi = []
for prefix, tag, stat_prefix in (("ID_INTERNAL_ASI_", "Class", ""), ("ID_INTERNAL_COMPANION_ASI_", "Companion", "companion:"),
                                 ("ID_INTERNAL_RACIAL_ASI_", "Race", "")):
    for a in ABIL:
        asi.append(el(prefix + a.upper(), a.title(), "Ability Score Improvement", supports=["Ability Score Improvement", tag, a.title()],
                      setters=[{"name": "allow duplicate", "value": "true"}],
                      rules=[stat(stat_prefix + a, 1, alt="Ability Score Increase")], description=f"+1 {a.title()}."))
files["abilities.json"] = asi

fx_skills = {e["id"]: e for e in load("proficiency.proficiency-skills")}
skills = [clean(fx_skills["ID_INTERNAL_SUPPORT_PROFICIENCY_SKILL"]) | {"description": "Skill proficiency tag."}]
for sid, name, ab in SKILLS:
    src = fx_skills["ID_PROFICIENCY_SKILL_" + sid]
    skills.append(el("ID_PROFICIENCY_SKILL_" + sid, name, "Proficiency", supports=src.get("supports"),
                     rules=[stat(f"{name.lower()}:proficiency", "proficiency", bonus="proficiency")], description=f"Proficiency in {name}."))
for sid, name, ab in SKILLS:
    skills.append(el("ID_INTERNAL_GRANTS_ADVANTAGE_SKILL_" + sid, f"Advantage ({name})", "Grants", "Internal"))
files["skills.json"] = skills

saves = []
for a in ABIL:
    saves.append(el("ID_PROFICIENCY_SAVINGTHROW_" + a.upper(), f"Saving Throw Proficiency ({a.title()})", "Proficiency", supports=["Saving Throw"],
                    rules=[stat(f"{a}:save:proficiency", "proficiency", bonus="proficiency", alt="Proficiency")]))
files["saving-throws.json"] = saves

expertise = []
fx_exp = {e["id"]: e for e in load("proficiency.expertise-skills")}
for sid, name, ab in SKILLS:
    expertise.append(el("ID_EXPERTISE_SKILL_" + sid, f"Skill Expertise ({name})", "Class Feature", supports=["Expertise", "Skill", "ID_INTERNAL_SUPPORT_EXPERTISE"],
                        requirements="ID_PROFICIENCY_SKILL_" + sid, rules=[stat(f"{name.lower()}:proficiency", "proficiency", bonus="double")],
                        description=f"Double proficiency bonus for {name}."))
t = fx_exp["ID_EXPERTISE_TOOL_THIEVES_TOOLS"]
expertise.append(clean(t) | {"description": "Double proficiency bonus with thieves' tools."})
files["expertise.json"] = expertise

work = []
for sid, name, ab in SKILLS:
    k = name.lower()
    work += [el(f"ID_INTERNAL_GRANTS_PROFICIENCY_SKILL_OR_DOUBLE_{sid}", f"{name} Proficiency or Double", "Grants", "Internal",
                rules=[grant(f"ID_INTERNAL_GRANTS_PROFICIENCY_SKILL_{sid}", requirements=f"!ID_PROFICIENCY_SKILL_{sid}"),
                       grant(f"ID_INTERNAL_GRANTS_PROFICIENCY_SKILL_{sid}_DOUBLE", requirements=f"ID_PROFICIENCY_SKILL_{sid}")]),
             el(f"ID_INTERNAL_GRANTS_PROFICIENCY_SKILL_{sid}", f"{name} Proficiency", "Grants", "Internal", rules=[grant(f"ID_PROFICIENCY_SKILL_{sid}", "Proficiency")]),
             el(f"ID_INTERNAL_GRANTS_PROFICIENCY_SKILL_{sid}_DOUBLE", f"{name} Proficiency Double", "Grants", "Internal",
                rules=[stat(f"{k}:proficiency", "proficiency", bonus="double", requirements=f"!ID_INTERNAL_GRANTS_PROFICIENCY_SKILL_{sid}", alt="Double Proficiency")])]
files["skill-helpers.json"] = work

# ---- catalogues copied as metadata only ----------------------------------------------------------
def catalogue(fname, names, desc):
    out = []
    for n in names:
        for e in load(n):
            if e["id"].startswith("ID_WOTC_SOURCE_"):
                continue
            out.append(clean(e) | {"description": desc(e)})
    files[fname] = out


catalogue("catalogues.json", ["core-size", "core-vision", "core-alignments", "core-languages", "damage-type", "conditions"],
          lambda e: f"{e['name']} ({e['type']}).")
catalogue("proficiencies.json", ["proficiencies.proficiency-armor", "proficiencies.proficiency-weapons", "proficiency.proficiency-tools-artisans",
                                "proficiency.proficiency-tools-gamingsets", "proficiency.proficiency-tools-kits", "proficiency.proficiency-tools-misc",
                                "proficiency.proficiency-tools-musical-instruments", "proficiency.items-spellcasting-focus"],
          lambda e: f"Proficiency: {e['name']}.")
catalogue("properties.json", ["properties.properties-armor", "properties.properties-firearms", "properties.properties-weapons", "support.properties-tools"],
          lambda e: f"{e['name']} ({e['type']}).")

# ---- Aurora compatibility: internal ids newer Aurora content references --------------------
# Race tags, the background-ASI marker, custom/background ability score options, sizes and
# creature types, feat +1 options, internal expertise, PHB 2024 tiefling markers. Our own
# minimal definitions (ids only must match Aurora's so community content resolves).
AB = [("STRENGTH","Strength","STR"),("DEXTERITY","Dexterity","DEX"),("CONSTITUTION","Constitution","CON"),
      ("INTELLIGENCE","Intelligence","INT"),("WISDOM","Wisdom","WIS"),("CHARISMA","Charisma","CHA")]
compat = []
def grants(id_, name, desc):
    compat.append({"id": id_, "name": name, "type": "Grants", "source": "Internal", "description": desc})
RACES = ["AARAKOCRA","AASIMAR","BUGBEAR","DRAGONBORN","DWARF","ELF","FIRBOLG","GENASI","AIR_GENASI","EARTH_GENASI",
 "FIRE_GENASI","WATER_GENASI","GNOME","GOBLIN","GOBLINOID","GOLIATH","HALF_ELF","HALF_ORC","HALFLING","HOBGOBLIN",
 "HUMAN","KENKU","KOBOLD","LIZARDFOLK","ORC","TABAXI","TIEFLING","TORTLE","TRITON","WARFORGED","YUAN_TI"]
for r in RACES:
    label = r.replace("_"," ").title().replace("Half Elf","Half-Elf").replace("Half Orc","Half-Orc").replace("Yuan Ti","Yuan-Ti")
    grants(f"ID_INTERNAL_GRANT_RACE_{r}", f"Race {label}", f"Marks the character as {label} for requirements (Aurora race tag).")
grants("ID_INTERNAL_GRANTS_BACKGROUND_ASI", "Background Ability Score Improvement",
       "Marks that the background provides the ability score increases; races that check it skip their own.")
grants("ID_INTERNAL_GRANTS_MULTICLASS_UNLOCKER", "Multiclass Unlocker", "Internal marker used by multiclass requirements.")
grants("ID_INTERNAL_GRANTS_SPELLCASTING_FEATURE", "Spellcasting Feature", "Marks a feature that provides the Spellcasting feature.")
grants("ID_INTERNAL_GRANTS_PACT_MAGIC_FEATURE", "Pact Magic Feature", "Marks a feature that provides Pact Magic.")
grants("ID_INTERNAL_GRANTS_FIGHTING_STYLE_FEATURE", "Fighting Style Feature", "Marks a feature that provides a Fighting Style.")
grants("ID_INTERNAL_GRANT_ARMOR_IGNORE_STRENGTH_REQUIREMENT", "Ignore Armor Strength Requirement",
       "Heavy armor does not reduce speed for a low Strength score.")
grants("ID_INTERNAL_GRANT_JACK_OF_ALL_TRADES_DISABLE", "Disable Jack of All Trades", "Internal marker that switches Jack of All Trades off.")
grants("ID_INTERNAL_FEATURE_REPLACEMENT_CLERIC_DIVINE_STRIKE_OR_POTENT_SPELLCASTING", "Cleric Divine Strike or Potent Spellcasting Replaced",
       "Marks that a feature replaces the cleric's level 8 domain feature.")
# sizes
for sz, label in [("TINY","Tiny"),("SMALL","Small"),("MEDIUM","Medium")]:
    compat.append({"id": f"ID_INTERNAL_RACIAL_TRAIT_{sz}", "name": label, "type": "Racial Trait", "source": "Internal",
                "rules": [{"kind":"grant","type":"Size","id":f"ID_SIZE_{sz}"}], "description": f"Your size is {label}."})
for t in ["ABERRATION","BEAST","CELESTIAL","CONSTRUCT","DRAGON","ELEMENTAL","FEY","FIEND","GIANT","HUMANOID","MONSTROSITY","OOZE","PLANT","UNDEAD"]:
    label = t.title()
    compat.append({"id": f"ID_INTERNAL_RACIAL_TRAIT_TYPE_{t}", "name": f"Creature Type ({label})", "type": "Racial Trait",
                "source": "Internal", "description": f"Your creature type is {label}."})
# feat +1 options
for key, name, _ in AB:
    compat.append({"id": f"ID_INTERNAL_ABILITY_SCORE_IMPROVEMENT_FEAT_{key}", "name": f"{name} +1", "type": "Ability Score Improvement",
                "source": "Internal", "supports": ["Feat Ability Score Increase"],
                "setters": [{"name":"allow duplicate","value":"true"}],
                "rules": [{"kind":"stat","name":name.lower(),"value":"1","alt":"Ability Score Increase"}],
                "description": f"+1 {name}."})
# custom ASI (+1 / +2 per ability), Aurora's customized-origin ids
for key, name, _ in AB:
    for n in (1, 2):
        compat.append({"id": f"ID_WOTC_TCOE_OPTION_CUSTOMIZED_ASI_{key}_INCREASE_{n}", "name": f"{name} +{n}",
                    "type": "Ability Score Improvement", "source": "Internal", "supports": [f"Custom Ability Score Increase {n}"],
                    "rules": [{"kind":"stat","name":name.lower(),"value":str(n),"alt":"Ability Score Increase"}],
                    "description": f"+{n} {name}."})
compat.append({"id": "ID_INTERNAL_CUSTOM_ASI_OPTION_PLUS_2_PLUS_1", "name": "Ability Score Increase +2, +1", "type": "Ability Score Improvement",
            "source": "Internal", "supports": ["Custom Ability Score Improvement Option"],
            "rules": [{"kind":"select","type":"Ability Score Improvement","name":"Ability Score Increase +2","supports":"Custom Ability Score Increase 2"},
                      {"kind":"select","type":"Ability Score Improvement","name":"Ability Score Increase +1","supports":"Custom Ability Score Increase 1"}],
            "description": "Increase one ability score by 2 and a different one by 1."})
compat.append({"id": "ID_INTERNAL_CUSTOM_ASI_OPTION_PLUS_1_PLUS_1_PLUS_1", "name": "Ability Score Increase +1, +1, +1", "type": "Ability Score Improvement",
            "source": "Internal", "supports": ["Custom Ability Score Improvement Option"],
            "rules": [{"kind":"select","type":"Ability Score Improvement","name":"Ability Score Increase +1","number":3,"supports":"Custom Ability Score Increase 1"}],
            "description": "Increase three different ability scores by 1."})
# background combinations (2024-style backgrounds)
for combo in itertools.combinations(AB, 3):
    code = "_".join(c[2] for c in combo)
    names = [c[1] for c in combo]
    base = f"ID_INTERNAL_ABILITY_SCORE_IMPROVEMENT_COMBINATION_{code}"
    listed = ", ".join(names)
    compat.append({"id": base, "name": listed, "type": "Ability Score Improvement", "source": "Internal",
                "supports": ["Custom Background Ability Score Combination"],
                "rules": [{"kind":"select","type":"Ability Score Improvement","name":"Background Ability Scores","supports":f"{base}_1|{base}_2"}],
                "description": f"Increase one of {listed} by 2 and another by 1, or all three by 1."})
    plus = lambda n: "|".join(f"ID_WOTC_TCOE_OPTION_CUSTOMIZED_ASI_{c[0]}_INCREASE_{n}" for c in combo)
    compat.append({"id": f"{base}_1", "name": "Ability Score Increase +2, +1", "type": "Ability Score Improvement", "source": "Internal",
                "rules": [{"kind":"select","type":"Ability Score Improvement","name":"Background Ability Score +2","supports":plus(2)},
                          {"kind":"select","type":"Ability Score Improvement","name":"Background Ability Score +1","supports":plus(1)}],
                "description": f"Increase one of {listed} by 2 and a different one of them by 1."})
    compat.append({"id": f"{base}_2", "name": "Ability Score Increase +1, +1, +1", "type": "Ability Score Improvement", "source": "Internal",
                "rules": [{"kind":"stat","name":c[1].lower(),"value":"1","alt":"Ability Score Increase (Background)"} for c in combo],
                "description": f"Increase {listed} by 1 each."})
for sk in ["ACROBATICS","ANIMALHANDLING:animal handling","ARCANA","ATHLETICS","DECEPTION","HISTORY","INSIGHT","INTIMIDATION",
           "INVESTIGATION","MEDICINE","NATURE","PERCEPTION","PERFORMANCE","PERSUASION","RELIGION","SLEIGHTOFHAND:sleight of hand",
           "STEALTH","SURVIVAL"]:
    key, _, stat = sk.partition(":")
    stat = stat or key.lower()
    compat.append({"id": f"ID_INTERNAL_PROFICIENCY_SKILL_EXPERTISE_{key}", "name": f"Skill Expertise ({stat.title()})",
                "type": "Proficiency", "source": "Internal",
                "supports": ["Expertise", "Skill Expertise", "ID_INTERNAL_SUPPORT_EXPERTISE"],
                "rules": [{"kind":"stat","name":f"{stat}:proficiency","value":"proficiency","bonus":"double"}],
                "description": f"Double proficiency bonus for {stat.title()}."})
compat.append({"id": "ID_VISION_LESSER_DARKVISION", "name": "Darkvision", "type": "Vision", "source": "Internal",
            "description": "Darkvision whose range is set by the feature that grants it."})
for ab in ["CHARISMA","INTELLIGENCE","WISDOM"]:
    grants(f"ID_WOTC_PHB24_GRANTS_TIEFLING_FIENDISH_LEGACY_SPELLCASTING_ABILITY_{ab}", f"Fiendish Legacy Spellcasting Ability ({ab.title()})",
           "Marker naming the spellcasting ability of Fiendish Legacy spells (Aurora PHB 2024 id).")
files["aurora-compat.json"] = compat

# ---- write -------------------------------------------------------------------------------------
os.makedirs(OUT, exist_ok=True)
order = ["core-levels.json", "multiclass.json", "internal.json", "aurora-compat.json", "abilities.json", "saving-throws.json", "skills.json",
         "skill-helpers.json", "expertise.json", "catalogues.json", "proficiencies.json", "properties.json"]
for f in order:
    els = files[f]
    for e in els:
        if "description" in e and not e["description"]:
            del e["description"]
    with open(os.path.join(OUT, f), "w") as fh:
        json.dump({"format": FMT, "ruleset": "any", "elements": els}, fh, indent=1, ensure_ascii=False)
        fh.write("\n")
manifest = {"format": "borealis-pack/1", "id": "core", "name": "Borealis Core Rules", "version": "1.0.0", "origin": "core",
            "rulesets": ["dnd2014", "dnd2024"], "description": "Base mechanics: abilities, skills, levels, multiclass tables, proficiencies and catalogues.",
            "license": "MIT", "files": [{"path": f, "kind": "content"} for f in order]}
json.dump(manifest, open(os.path.join(OUT, "pack.json"), "w"), indent=1)
print("wrote", sum(len(files[f]) for f in order), "elements")
