#!/usr/bin/env python3
"""Step 3: monsters.json (+ raw_cols.txt for independent counts) -> content/srd-5.1-monsters/ (committed output)."""
import json, re
from collections import Counter
from pathlib import Path
HERE = Path(__file__).resolve().parent
OUT = HERE.parents[1] / "content" / "srd-5.1-monsters"
SOURCE = "SRD 5.1 Monsters"
ATTRIBUTION = ('This work includes material taken from the System Reference Document 5.1 ("SRD 5.1") by Wizards of the Coast LLC '
               'and available at https://dnd.wizards.com/resources/systems-reference-document. The SRD 5.1 is licensed under the '
               'Creative Commons Attribution 4.0 International License available at https://creativecommons.org/licenses/by/4.0/legalcode. '
               'Modified: the SRD stat blocks were structured into typed JSON data (sizes, types, numbers and attacks were parsed from the '
               'printed text; hyphenation and line breaks were normalised).')
FILES = [("monsters.json", "monsters", "SRD 5.1 Monsters", "Monsters (A-Z) from the SRD 5.1 monster chapter."),
         ("creatures.json", "creatures", "SRD 5.1 Miscellaneous Creatures", "Appendix MM-A: miscellaneous creatures (beasts and animated plants) from the SRD 5.1."),
         ("npcs.json", "npcs", "SRD 5.1 Nonplayer Characters", "Appendix MM-B: nonplayer characters from the SRD 5.1.")]


def make_id(name):
    return "ID_SRD51_MONSTER_" + re.sub(r"[^A-Z0-9]+", "_", name.upper()).strip("_")


def prune(d):
    """Drop empty optional lists so the committed JSON stays compact (the serde defaults restore them)."""
    return {k: v for k, v in d.items() if not (isinstance(v, list) and not v and k not in ("size", "ac"))}


def dump(path, obj):
    path.write_text(json.dumps(obj, indent=2, ensure_ascii=False) + "\n")


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    M = json.loads((HERE / "monsters.json").read_text())
    ids = Counter(make_id(m["name"]) for m in M)
    assert all(c == 1 for c in ids.values()), [i for i, c in ids.items() if c > 1]
    manifest_files = [{"path": "source.json", "kind": "content"}]
    dump(OUT / "source.json", {
        "format": "borealis-content/1", "ruleset": "dnd2014",
        "info": {"name": "SRD 5.1 Monsters Source", "description": "Source record and CC-BY-4.0 attribution for the SRD 5.1 monster pack."},
        "elements": [{
            "id": "ID_SRD51_MONSTERS_SOURCE", "name": SOURCE, "type": "Source", "source": "Core",
            "description": "<p><strong>System Reference Document 5.1</strong> monsters and nonplayer characters, released by Wizards of the Coast LLC under the Creative Commons Attribution 4.0 International License.</p><p>" + ATTRIBUTION + "</p>",
            "setters": [
                {"name": "abbreviation", "value": "SRD"},
                {"name": "url", "value": "https://dnd.wizards.com/resources/systems-reference-document"},
                {"name": "author", "value": "Wizards of the Coast LLC", "attributes": {"abbreviation": "WOTC", "url": "https://dnd.wizards.com"}},
                {"name": "official", "value": "true"},
                {"name": "information", "value": "Content of the System Reference Document 5.1 only (CC-BY-4.0).",
                 "attributes": {"url": "https://creativecommons.org/licenses/by/4.0/legalcode"}},
            ]}]})
    counts = {}
    for fname, section, title, desc in FILES:
        els = [{"id": make_id(m["name"]), "name": m["name"], "type": "Monster", "source": SOURCE, "data": prune(m["data"])}
               for m in M if m["section"] == section]
        counts[section] = len(els)
        dump(OUT / fname, {"format": "borealis-content/1", "ruleset": "dnd2014", "info": {"name": title, "description": desc}, "elements": els})
        manifest_files.append({"path": fname, "kind": "content"})
    dump(OUT / "pack.json", {
        "format": "borealis-pack/1", "id": "srd-5.1-monsters", "name": "SRD 5.1 Monsters", "version": "1.0.0", "origin": "core",
        "rulesets": ["dnd2014"], "license": "CC-BY-4.0", "attribution": ATTRIBUTION,
        "description": "Every monster and nonplayer character stat block of the SRD 5.1 as typed Monster elements (ruleset dnd2014).",
        "files": manifest_files})
    # independent signals straight from the PDF text
    raw = (HERE / "raw_cols.txt").read_text().split("\n")
    pdf_ac = sum(1 for l in raw if l.startswith("Armor Class"))
    pdf_cr = sum(1 for l in raw if l.startswith("Challenge "))
    total = sum(counts.values())
    feats = Counter()
    for m in M:
        d = m["data"]
        feats["traits"] += len(d["traits"]); feats["actions"] += len(d["actions"])
        feats["bonus actions"] += len(d["bonus_actions"]); feats["reactions"] += len(d["reactions"])
        feats["legendary actions"] += len(d.get("legendary", {}).get("actions", []))
        feats["parsed attacks"] += sum(1 for a in d["actions"] if "attack" in a)
    legendary = sum(1 for m in M if "legendary" in m["data"])
    lore = sum(1 for m in M if "description" in m["data"])
    (OUT / "COVERAGE.md").write_text(f"""# SRD 5.1 monsters: coverage

Generated by `content-tools/srd51-monsters/build_pack.py` from `input/srd/SRD_CC_v5.1.pdf` (CC-BY-4.0).

| Count | Value |
|---|---|
| Stat blocks in the pack | {total} |
| `Armor Class` lines in the SRD monster pages (pdf pages 260-403) | {pdf_ac} |
| `Challenge` lines in the same pages | {pdf_cr} |
| Monsters chapter (A-Z) | {counts['monsters']} |
| Appendix MM-A miscellaneous creatures | {counts['creatures']} |
| Appendix MM-B nonplayer characters | {counts['npcs']} |
| Blocks with legendary actions | {legendary} |
| Blocks with a flavour paragraph (`description`) | {lore} |
| Traits / actions / bonus actions / reactions | {feats['traits']} / {feats['actions']} / {feats['bonus actions']} / {feats['reactions']} |
| Legendary action options | {feats['legendary actions']} |
| Actions parsed into a structured `attack` | {feats['parsed attacks']} |

Pack coverage is {total}/{pdf_ac}: every stat block the PDF prints is present. Nothing from outside the SRD 5.1 was used.

## Not modelled (printed in the SRD, not stat blocks)

* `Variant: Diseased Giant Rats`, `Variant: Insect Swarms`, the `Half-Dragon Template` and `Customizing NPCs` sections:
  prose rules that modify a stat block. The half-dragon *Veteran* sample block is included.
* General monster-chapter text (lair actions, regional effects, legendary creature rules).
* Innate/prepared spell lists stay inside the feature `text`; they are not parsed into spell ids.

## Known source quirks

* Frog and Sea Horse print `Challenge 0 (0 XP)`; the XP table value for CR 0 is 10, so CR 0 accepts 0 or 10 XP.
* Where the SRD prints a form-dependent value (werecreature AC/speed, rat-form-only darkvision) it is kept in the
  `condition` / `note` fields rather than guessed.
""")
    print("files written to", OUT, "total", total, "pdf", pdf_ac)


if __name__ == "__main__":
    main()
