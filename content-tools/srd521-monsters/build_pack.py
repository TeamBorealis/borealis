#!/usr/bin/env python3
"""Step 3: monsters.json (+ raw_cols.txt for independent counts) -> content/srd-5.2.1-monsters/ (committed output)."""
import json, re
from collections import Counter
from pathlib import Path
HERE = Path(__file__).resolve().parent
APP = HERE.parents[1]
OUT = APP / "content" / "srd-5.2.1-monsters"
SOURCE = "SRD 5.2.1 Monsters"
ATTRIBUTION = ('This work includes material from the System Reference Document 5.2.1 ("SRD 5.2.1") by Wizards of the Coast LLC, '
               'available at https://www.dndbeyond.com/srd. The SRD 5.2.1 is licensed under the Creative Commons Attribution 4.0 '
               'International License, available at https://creativecommons.org/licenses/by/4.0/legalcode. Modified: the SRD stat '
               'blocks were structured into typed JSON data (sizes, types, numbers, attacks, saving throws and spell lists were '
               'parsed from the printed text; hyphenation and line breaks were normalised).')
FILES = [("monsters.json", "monsters", "SRD 5.2.1 Monsters A-Z", "Monsters A-Z from the SRD 5.2.1 monsters chapter."),
         ("animals.json", "animals", "SRD 5.2.1 Animals", "Animals from the SRD 5.2.1 animals chapter (Beast stat blocks, usable for Wild Shape and companions).")]
FIRST_ANIMAL, LAST_MONSTER = "Allosaurus", "Ogre Zombie"
# 2024 names that replace a differently named SRD 5.1 creature (same creature, new name or new stat block)
RENAMED = {
    "Animated Flying Sword": "Flying Sword", "Animated Rug of Smothering": "Rug of Smothering", "Azer Sentinel": "Azer",
    "Bugbear Warrior": "Bugbear", "Centaur Trooper": "Centaur", "Cultist Fanatic": "Cult Fanatic",
    "Shrieker Fungus": "Shrieker", "Gnoll Warrior": "Gnoll", "Goblin Warrior": "Goblin", "Hobgoblin Warrior": "Hobgoblin",
    "Kobold Warrior": "Kobold", "Merfolk Skirmisher": "Merfolk", "Minotaur of Baphomet": "Minotaur",
    "Priest Acolyte": "Acolyte", "Sahuagin Warrior": "Sahuagin", "Sphinx of Lore": "Gynosphinx",
    "Sphinx of Valor": "Androsphinx", "Tough": "Thug", "Warrior Veteran": "Veteran", "Will-o’-Wisp": "Will-o'-Wisp",
    "Giant Venomous Snake": "Giant Poisonous Snake", "Venomous Snake": "Poisonous Snake",
    "Swarm of Venomous Snakes": "Swarm of Poisonous Snakes", "Giant Seahorse": "Giant Sea Horse", "Seahorse": "Sea Horse",
    "Piranha": "Quipper", "Swarm of Piranhas": "Swarm of Quippers", "Succubus": "Succubus/Incubus", "Incubus": "Succubus/Incubus",
}


def make_id(name):
    return "ID_SRD521_MONSTER_" + re.sub(r"[^A-Z0-9]+", "_", name.upper()).strip("_")


def prune(d):
    """Drop empty optional lists so the committed JSON stays compact (the serde defaults restore them)."""
    return {k: v for k, v in d.items() if not (isinstance(v, list) and not v and k not in ("size", "ac"))}


def dump(path, obj):
    path.write_text(json.dumps(obj, indent=2, ensure_ascii=False) + "\n")


def srd51_ids():
    out = {}
    for f in ("monsters", "creatures", "npcs"):
        for e in json.loads((APP / "content" / "srd-5.1-monsters" / f"{f}.json").read_text())["elements"]:
            out[e["name"]] = e["id"]
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    M = json.loads((HERE / "monsters.json").read_text())
    names = [m["name"] for m in M]
    assert names[names.index(FIRST_ANIMAL) - 1] == LAST_MONSTER
    cut = names.index(FIRST_ANIMAL)
    for i, m in enumerate(M):
        m["section"] = "animals" if i >= cut else "monsters"
    ids = Counter(make_id(m["name"]) for m in M)
    assert all(c == 1 for c in ids.values()), [i for i, c in ids.items() if c > 1]
    old = srd51_ids()
    manifest_files = [{"path": "source.json", "kind": "content"}]
    dump(OUT / "source.json", {
        "format": "borealis-content/1", "ruleset": "dnd2024",
        "info": {"name": "SRD 5.2.1 Monsters Source", "description": "Source record and CC-BY-4.0 attribution for the SRD 5.2.1 monster pack."},
        "elements": [{
            "id": "ID_SRD521_MONSTERS_SOURCE", "name": SOURCE, "type": "Source", "source": "Core",
            "description": "<p><strong>System Reference Document 5.2.1</strong> monsters and animals, released by Wizards of the Coast LLC under the Creative Commons Attribution 4.0 International License.</p><p>" + ATTRIBUTION + "</p>",
            "setters": [
                {"name": "abbreviation", "value": "SRD"},
                {"name": "url", "value": "https://www.dndbeyond.com/srd"},
                {"name": "author", "value": "Wizards of the Coast LLC", "attributes": {"abbreviation": "WOTC", "url": "https://dnd.wizards.com"}},
                {"name": "official", "value": "true"},
                {"name": "information", "value": "Content of the System Reference Document 5.2.1 only (CC-BY-4.0).",
                 "attributes": {"url": "https://creativecommons.org/licenses/by/4.0/legalcode"}},
            ]}]})
    counts, mapped, renamed = {}, 0, 0
    for fname, section, title, desc in FILES:
        els = []
        for m in M:
            if m["section"] != section:
                continue
            el = {"id": make_id(m["name"]), "name": m["name"], "type": "Monster", "source": SOURCE, "ruleset": "dnd2024"}
            prev = RENAMED.get(m["name"], m["name"])
            if prev in old:
                el["maps_to"] = {"dnd2014": old[prev], "kind": "approximate"}
                mapped += 1
                if prev != m["name"]:
                    el["renamed_from"] = prev
                    renamed += 1
            el["data"] = prune(m["data"])
            els.append(el)
        counts[section] = len(els)
        dump(OUT / fname, {"format": "borealis-content/1", "ruleset": "dnd2024", "info": {"name": title, "description": desc}, "elements": els})
        manifest_files.append({"path": fname, "kind": "content"})
    dump(OUT / "pack.json", {
        "format": "borealis-pack/1", "id": "srd-5.2.1-monsters", "name": "SRD 5.2.1 Monsters", "version": "1.0.0", "origin": "core",
        "rulesets": ["dnd2024"], "license": "CC-BY-4.0", "attribution": ATTRIBUTION,
        "description": "Every monster and animal stat block of the SRD 5.2.1 as typed Monster elements (ruleset dnd2024), linked to their SRD 5.1 counterparts.",
        "files": manifest_files})
    raw = (HERE / "raw_cols.txt").read_text().split("\n")
    pdf_ac = sum(1 for l in raw if re.match(r"^AC \d+ Initiative [+−-]\d+ \(\d+\)", l.strip()))
    pdf_cr = sum(1 for l in raw if re.match(r"^CR (\d+(/\d+)?) \((XP|[\d,]+ XP)", l.strip()))
    total = sum(counts.values())
    feats = Counter()
    for m in M:
        d = m["data"]
        for k in ("traits", "actions", "bonus_actions", "reactions"):
            feats[k] += len(d[k])
        feats["legendary actions"] += len(d.get("legendary", {}).get("actions", []))
        feats["parsed attacks"] += sum(1 for k in ("actions", "bonus_actions", "reactions") for a in d[k] if "attack" in a)
        feats["parsed saves"] += sum(1 for k in ("traits", "actions", "bonus_actions", "reactions") for a in d[k] if "save" in a)
        feats["spellcasting blocks"] += sum(1 for k in ("traits", "actions") for a in d[k] if "spellcasting" in a)
    legendary = sum(1 for m in M if "legendary" in m["data"])
    beasts = sum(1 for m in M if m["data"]["creature_type"] == "beast")
    gear = sum(1 for m in M if m["data"].get("gear"))
    lair = sum(1 for m in M if m["data"].get("xp_lair"))
    (OUT / "COVERAGE.md").write_text(f"""# SRD 5.2.1 monsters: coverage

Generated by `content-tools/srd521-monsters/build_pack.py` from `input/srd/SRD_CC_v5.2.1.pdf` (CC-BY-4.0, pdf pages 258-364).

| Count | Value |
|---|---|
| Stat blocks in the pack | {total} |
| `AC n Initiative` lines in the SRD monster pages (independent count of the extracted text) | {pdf_ac} |
| `CR` lines in the same pages | {pdf_cr} |
| Monsters A-Z chapter (`monsters.json`) | {counts['monsters']} |
| Animals chapter (`animals.json`) | {counts['animals']} |
| Blocks of type Beast (Wild Shape candidates, plus a few Animals-chapter Monstrosities/Celestials excluded) | {beasts} |
| Blocks with legendary actions | {legendary} |
| Blocks with a `Gear` line | {gear} |
| Blocks with a separate lair XP value | {lair} |
| Traits / actions / bonus actions / reactions | {feats['traits']} / {feats['actions']} / {feats['bonus_actions']} / {feats['reactions']} |
| Legendary action options | {feats['legendary actions']} |
| Actions parsed into a structured `attack` | {feats['parsed attacks']} |
| Features with a parsed `save` | {feats['parsed saves']} |
| `Spellcasting` features with parsed spell lists | {feats['spellcasting blocks']} |
| Elements with `maps_to` an SRD 5.1 monster | {mapped} ({renamed} under a new name, `renamed_from`) |

Pack coverage is {total}/{pdf_ac}: every stat block the PDF prints is present. Nothing from outside the SRD 5.2.1 was used.

## What a 2024 stat block adds over the 5.1 model

`initiative {{modifier, score}}`, `gear[]`, `xp_lair`, `legendary.lair_uses`, usage `per_day_in_lair`, attack kinds
`melee | ranged | melee_or_ranged`, `Feature.save` (saving throw DC/damage/half on success) and `Feature.spellcasting`
(ability, DC, attack bonus, `At Will` / `N/Day [Each]` lists with spell ids into `srd-5.2.1` where the spell exists).
`saves` lists only the saves that differ from the ability modifier (the printed `SAVE` column minus the `MOD` column);
`MonsterData::save_bonus` returns the printed value for any ability.

## Not modelled

* Spell lists are not resolved beyond name/id/note; "(level N version)" stays in `note`.
* The Pit Fiend `Hellfire Spellcasting` is a single prose spell (Fireball) and is kept as text only.
* Alternative damage that applies under a condition ("or 12 (2d8 + 3) Piercing damage if Grappled") is left in the
  feature `text`; the structured `attack.damage` holds the base damage.
* Rules text of the monsters chapter (stat block overview, legendary rules) is not a stat block and is not included.

## Source quirks (kept as printed unless stated)

* Archmage prints `CR 12 (XP 8,000)`; the SRD's own XP table says CR 12 is 8,400. The pack stores 8,400 (override in
  `parse_monsters.py`, `OVERRIDES`) so the XP rule holds.
* Gray Ooze prints `Initiative -2 (13)` and Invisible Stalker `Initiative +7 (22)`; the score should be 10 plus the modifier
  (8 and 17). Both are kept as printed and flagged by `MonsterData::soft_checks`.
* Frog-like CR 0 blocks (Seahorse, Shrieker Fungus) print `XP 0`; CR 0 accepts 0 or 10 XP.
* Young White Dragon: the PDF text lost the minus sign of the Int saving throw (`-2 2`); it is restored from the modifier.
* Some stat blocks print `Con22` without a space and `+17 to hit` (Ancient Gold Dragon Rend) instead of the usual wording.
""")
    print("files written to", OUT, "total", total, "pdf", pdf_ac, "mapped", mapped, "renamed", renamed)


if __name__ == "__main__":
    main()
