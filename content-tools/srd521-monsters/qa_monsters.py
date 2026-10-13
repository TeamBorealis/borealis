#!/usr/bin/env python3
"""QA over monsters.json: soft consistency checks that flag extraction errors or SRD source quirks."""
import json, re
from pathlib import Path
HERE = Path(__file__).resolve().parent
XP = {"0":10,"1/8":25,"1/4":50,"1/2":100,"1":200,"2":450,"3":700,"4":1100,"5":1800,"6":2300,"7":2900,"8":3900,"9":5000,
      "10":5900,"11":7200,"12":8400,"13":10000,"14":11500,"15":13000,"16":15000,"17":18000,"18":20000,"19":22000,
      "20":25000,"21":33000,"22":41000,"23":50000,"24":62000,"25":75000,"26":90000,"27":105000,"28":120000,"29":135000,"30":155000}
def mod(s): return (s - 10) // 2
def avg(f):
    m = re.match(r"^(\d+)d(\d+)([+-]\d+)?$", f); n, d, b = int(m[1]), int(m[2]), int(m[3] or 0)
    return n * (d + 1) // 2 + b
M = json.load(open(HERE / "monsters.json"))
bad = 0
def flag(*a):
    global bad
    print(*a); bad += 1
for m in M:
    d = m["data"]; n = m["name"]
    if XP[d["cr"]] != d["xp"]: flag("XP", n, d["cr"], d["xp"])
    f = d["hp"]["formula"]; mm = re.match(r"^(\d+)d(\d+)([+-]\d+)?$", f); nn, bb = int(mm[1]), int(mm[3] or 0)
    if avg(f) != d["hp"]["average"]: flag("HP", n, d["hp"])
    if bb != nn * mod(d["abilities"]["con"]): flag("HPCON", n, f)
    pp = d["senses"]["passive_perception"]
    per = d.get("skills", {}).get("perception", mod(d["abilities"]["wis"]))
    if pp != 10 + per: flag("PASSIVE", n, pp, 10 + per)
    pb = d["proficiency_bonus"]
    for k, v in d.get("saves", {}).items():
        if v - mod(d["abilities"][k]) not in (pb, pb + 1, pb - 1): flag("SAVE", n, k, v)
    ini = d["initiative"]
    if ini["score"] != 10 + ini["modifier"]: flag("INIT", n, ini)
    for sec in ("traits", "actions", "bonus_actions", "reactions"):
        for a in d[sec] + (d.get("legendary", {}).get("actions", []) if sec == "actions" else []):
            for dm in (a.get("attack", {}).get("damage", []) + a.get("save", {}).get("damage", [])):
                if dm.get("dice") and avg(dm["dice"]) != dm["average"]: flag("DMG", n, a["name"], dm)
            if a["name"] != "Spellcasting" and not a["text"].rstrip().endswith((".", ")", ":")): flag("ENDS", n, a["name"], a["text"][-40:])
            if a["name"] == "Spellcasting" and "spellcasting" not in a: flag("NOSPELLS", n)
            if re.search(r"Attack Roll: [+-]\d", a["text"]) and "attack" not in a: flag("NOATTACK", n, a["name"])
            if re.search(r"Saving Throw: DC", a["text"]) and "save" not in a: flag("NOSAVE", n, a["name"])
print("QA flags:", bad, "monsters:", len(M))
