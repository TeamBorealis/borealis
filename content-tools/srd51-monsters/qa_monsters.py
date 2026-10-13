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
    return n * (d + 1) // 2 + b if True else 0
M = json.load(open(HERE / "monsters.json"))
bad = 0
for m in M:
    d = m["data"]; n = m["name"]
    if XP[d["cr"]] != d["xp"] and not (d["cr"] == "0" and d["xp"] == 0): print("XP", n, d["cr"], d["xp"]); bad += 1
    f = d["hp"].get("formula")
    if f:
        mm = re.match(r"^(\d+)d(\d+)([+-]\d+)?$", f)
        nn, dd, bb = int(mm[1]), int(mm[2]), int(mm[3] or 0)
        exp = int(nn * (dd + 1) / 2 + bb)
        if exp != d["hp"]["average"] and exp + 1 != d["hp"]["average"]: print("HP", n, d["hp"], exp); bad += 1
        con = mod(d["abilities"]["con"])
        if bb != nn * con and abs(bb - nn * con) > 0: print("HPCON", n, f, "con", d["abilities"]["con"], "expected bonus", nn * con); bad += 1
    pp = d["senses"].get("passive_perception")
    wis = mod(d["abilities"]["wis"]); per = d.get("skills", {}).get("perception", wis)
    if pp != 10 + per: print("PASSIVE", n, pp, 10 + per); bad += 1
    pb = d["proficiency_bonus"]
    for k, v in d.get("saves", {}).items():
        if v - mod(d["abilities"][k]) not in (pb, pb+1, pb-1, pb+2) : print("SAVE", n, k, v); bad += 1
    for sec in ("actions",):
        for a in d[sec]:
            at = a.get("attack")
            if at:
                for dm in at.get("damage", []):
                    if dm.get("dice"):
                        e = avg(dm["dice"])
                        if e != dm["average"]: print("DMG", n, a["name"], dm); bad += 1
    if d["creature_type"] not in "aberration beast celestial construct dragon elemental fey fiend giant humanoid monstrosity ooze plant undead".split(): print("TYPE", n, d["creature_type"]); bad += 1
print("QA flags:", bad, "monsters:", len(M))
