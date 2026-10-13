#!/usr/bin/env python3
"""Step 2: raw_cols.txt -> monsters.json (scratch). SRD 5.2.1 monster A-Z and animals stat blocks."""
import glob, json, re
from pathlib import Path

HERE = Path(__file__).resolve().parent
APP = HERE.parents[1]
SIZES = ["Tiny", "Small", "Medium", "Large", "Huge", "Gargantuan"]
SECTIONS = {"Traits": "traits", "Actions": "actions", "Bonus Actions": "bonus_actions", "Reactions": "reactions",
            "Legendary Actions": "legendary"}
ABIL = {"Str": "str", "Dex": "dex", "Con": "con", "Int": "int", "Wis": "wis", "Cha": "cha"}
ABIL_LONG = {"Strength": "str", "Dexterity": "dex", "Constitution": "con", "Intelligence": "int", "Wisdom": "wis", "Charisma": "cha"}
SMALL = {"of", "the", "and", "or", "to", "in", "a", "an", "on", "with", "from", "for", "at", "as", "by"}
FOOTER = re.compile(r"^(\d+ System Reference Document 5\.2\.1|System Reference Document 5\.2\.1 \d+)$")
HEADING = re.compile(r"^(Monsters A–Z|Animals)$")
LABELS = ["Skills", "Vulnerabilities", "Resistances", "Immunities", "Gear", "Senses", "Languages", "CR"]
DTYPES = ["acid", "bludgeoning", "cold", "fire", "force", "lightning", "necrotic", "piercing", "poison",
          "psychic", "radiant", "slashing", "thunder"]
SKILLS = ["Acrobatics", "Animal Handling", "Arcana", "Athletics", "Deception", "History", "Insight", "Intimidation",
          "Investigation", "Medicine", "Nature", "Perception", "Performance", "Persuasion", "Religion",
          "Sleight of Hand", "Stealth", "Survival"]
TYPES = ["aberration", "beast", "celestial", "construct", "dragon", "elemental", "fey", "fiend", "giant", "humanoid",
         "monstrosity", "ooze", "plant", "undead"]
INIT_RE = re.compile(r"^AC (\d+) Initiative ([+-]\d+) \((\d+)\)$")


def clean(s):
    s = s.replace("\xad", "").replace("​", "").replace("‐", "-").replace("‑", "-").replace("−", "-")
    s = s.replace(" ", " ")
    return re.sub(r"-{2,}", "-", s)


def num(s):
    return int(s.replace(",", ""))


def sq(s):
    return re.sub(r"\s+", " ", s).strip()


def load_lines():
    out = []
    for l in (HERE / "raw_cols.txt").read_text().split("\n"):
        if l.startswith("@@COL"):
            continue
        l = clean(l).strip()
        if FOOTER.match(l) or HEADING.match(l):
            continue
        out.append(l)
    return out


def segment(lines):
    """-> list of (name, type_line, body_lines). A block starts at 'AC n Initiative ...'; type line and name precede it."""
    starts = [i for i, l in enumerate(lines) if INIT_RE.match(l)]
    meta = []
    for i in starts:
        j = i - 1
        while not lines[j]:
            j -= 1
        k = j - 1
        while not lines[k]:
            k -= 1
        meta.append((k, j, i))   # name line, type line, AC line
    blocks = []
    for n, (k, j, i) in enumerate(meta):
        end = len(lines)
        if n + 1 < len(meta):
            nk, nj, ni = meta[n + 1]
            end = nk
            # optional group heading directly above the next name line
            h = nk - 1
            while h > i and not lines[h]:
                h -= 1
            hl = lines[h]
            if h > i and hl and not hl.endswith((".", ")", ":")) and len(hl.split()) <= 4 and (
                    lines[nk] == hl or h < nk and (not lines[h - 1] or lines[h - 1].endswith((".", ")")))):
                # a heading is a short Title Case line that is not part of the previous block's text
                if lines[nk] == hl or all(w[0].isupper() for w in hl.split() if w.lower() not in SMALL):
                    end = h
        blocks.append((lines[k], lines[j], [lines[i]] + lines[i + 1:end]))
    return blocks


def split_top(s, sep=","):
    parts, depth, cur = [], 0, ""
    for ch in s:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == sep and depth == 0:
            parts.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        parts.append(cur.strip())
    return parts


def parse_type_line(text):
    m = re.match(r"^(Tiny|Small|Medium|Large|Huge|Gargantuan)(?: or (Tiny|Small|Medium|Large|Huge|Gargantuan))? "
                 r"(?:Swarm of (Tiny|Small|Medium) )?([A-Za-z]+?)(?:s)?(?: \(([^)]*)\))?, ([A-Za-z ]+)$", text)
    if not m:
        raise ValueError("type line " + text)
    d = {"size": [m.group(1).lower()] + ([m.group(2).lower()] if m.group(2) else [])}
    if m.group(3):
        d["swarm_of"] = m.group(3).lower()
    ty = m.group(4).lower()
    if ty not in TYPES:
        raise ValueError("type " + text)
    d["creature_type"] = ty
    if m.group(5):
        d["tags"] = [t.strip().lower() for t in m.group(5).split(",")]
    d["alignment"] = m.group(6).strip().lower()
    return d


def parse_hp(text):
    m = re.match(r"^HP (\d+) \((\d+d\d+)(?: ([+-]) (\d+))?\)$", text)
    if not m:
        raise ValueError("hp " + text)
    f = m.group(2) + (m.group(3) + m.group(4) if m.group(3) else "")
    return {"average": int(m.group(1)), "formula": f}


def parse_speed(text):
    text = re.sub(r"^Speed ", "", text)
    sp, notes = {}, []
    parts = split_top(text)
    for idx, part in enumerate(parts):
        m = re.match(r"^(?:(Burrow|Climb|Fly|Swim) )?(\d+) ft\.(?: \(([^)]*)\))?$", part)
        if m:
            kind = (m.group(1) or "Walk").lower()
            val = int(m.group(2))
            extra = m.group(3)
            if extra == "hover":
                sp["hover"] = True
                sp[kind] = val
            elif extra:
                if kind == "walk" and kind not in sp:
                    sp[kind] = val
                notes.append(f"{kind}: {val} ft. ({extra})")
                if kind == "walk" and "walk" in sp and sp["walk"] != val and idx > 0:
                    pass
            else:
                if kind in sp:      # second walk value without qualifier: keep the first
                    notes.append(f"{kind}: {val} ft.")
                else:
                    sp[kind] = val
            continue
        m = re.match(r"^(Climb) or (Fly) (\d+) ft\. \((.*)\)$", part)
        if m:
            sp["climb"] = sp["fly"] = int(m.group(3))
            notes.append(f"climb or fly: {m.group(4)}")
            continue
        raise ValueError("speed " + text)
    if notes:
        sp["note"] = "; ".join(notes)
    return sp


def parse_skills(text):
    out, notes = {}, []
    for part in split_top(text):
        m = re.match(r"^(" + "|".join(SKILLS) + r") ([+-]\d+)(?: \((.*)\))?$", part)
        if not m:
            raise ValueError("skill " + part)
        out[m.group(1).lower()] = int(m.group(2))
        if m.group(3):
            notes.append(f"{m.group(1).lower()}: {m.group(3)}")
    return out, notes


def parse_damage_mods(text):
    out = []
    for clause in [c.strip() for c in text.split(";") if c.strip()]:
        types = [w for w in re.findall(r"[a-z]+", clause.lower()) if w in DTYPES]
        e = {"types": types}
        if re.sub(r"\b(" + "|".join(DTYPES) + r")\b|,|\band\b|\s+", "", clause.lower()):
            e["text"] = clause.lower()
        out.append(e)
    return out


def parse_immunities(text):
    """'Poison, Thunder; Exhaustion, Grappled' -> damage clauses (first ';' part with damage types) + conditions."""
    parts = [p.strip() for p in text.split(";")]
    dmg, cond = [], []
    for p in parts:
        words = [w.strip().lower() for w in split_top(p)]
        if words and all(w in DTYPES or any(w.startswith(t + " ") for t in DTYPES) for w in words):
            dmg.extend(parse_damage_mods(p))
        else:
            cond.extend(words)
    return dmg, cond


def parse_senses(text):
    s = {}
    for part in [p for p in re.split(r"[;,](?![^(]*\))", text) if p.strip()]:
        part = part.strip()
        m = re.match(r"^Passive Perception (\d+)$", part)
        if m:
            s["passive_perception"] = int(m.group(1))
            continue
        m = re.match(r"^(Blindsight|Darkvision|Tremorsense|Truesight) (\d+) ft\.(?: \((.*)\))?$", part)
        if not m:
            raise ValueError("sense " + part)
        k = m.group(1).lower()
        s[k] = int(m.group(2))
        if m.group(3) == "blind beyond this radius":
            s["blind_beyond"] = True
        elif m.group(3):
            s.setdefault("notes", []).append(f"{k}: {m.group(3)}")
    if "passive_perception" not in s:
        raise ValueError("no passive " + text)
    return s


def parse_languages(text):
    if text == "None":
        return [], None
    items, tele = [], None
    for chunk in [c.strip() for c in text.split(";")]:
        m = re.match(r"^telepathy (\d+) ft\.$", chunk)
        if m:
            tele = int(m.group(1))
        elif chunk.startswith("understands "):
            items.append(chunk)
        else:
            items.extend(split_top(chunk))
    return items, tele


def parse_cr(text):
    m = re.match(r"^(\d+(?:/\d+)?) \((?:XP ([\d,]+)(?:, or ([\d,]+) in lair)?|([\d,]+) XP); PB \+(\d+)\)$", text)
    if not m:
        raise ValueError("cr " + text)
    xp = num(m.group(2) or m.group(4))
    return m.group(1), xp, (num(m.group(3)) if m.group(3) else None), int(m.group(5))


# ---------------------------------------------------------------- entries
TITLE_RE = re.compile(r"^((?:[A-Z][\w’'\-]*|\([^)]*\))(?: (?:[A-Z][\w’'\-]*|\([^)]*\)|" + "|".join(SMALL) + r"))*)\.(?: |$)")
LIST_RE = re.compile(r"^(At Will|\d+/Day(?: Each)?):")


def is_title(line):
    m = TITLE_RE.match(line)
    if not m:
        return None
    words = re.sub(r"\([^)]*\)", "(x)", m.group(1)).split()
    if len(words) > 9 or words[0] in SMALL:
        return None
    return m


def split_entries(lines):
    entries = []
    for l in lines:
        if not l:
            continue
        m = is_title(l)
        prev = entries[-1][1] if entries else None
        sentence_end = (not prev) or prev[-1].endswith((".", ")", ":", "!", "?")) or any(LIST_RE.match(x) for x in prev)
        if m and sentence_end:
            entries.append([m.group(1), [l[m.end():]]])
        elif entries:
            entries[-1][1].append(l)
        else:
            entries.append([None, [l]])
    return entries


KEEP_COMPOUND = {"foot-wide"}


def dehyphen_join(t, l):
    """Join a wrapped line: a trailing '-' is a soft hyphen unless the word is numeric ("10-foot"), the next
    fragment is capitalised ("Shape-Shift") or the compound is in KEEP_COMPOUND."""
    if not t.endswith("-"):
        return t + " " + l
    m = re.search(r"([\w’']+)-$", t)
    nxt = re.match(r"[\w’']+", l)
    if not m or not nxt or not l[0].islower() or m.group(1).isdigit() or (m.group(1) + "-" + nxt.group(0)) in KEEP_COMPOUND:
        return t + l
    return t[:-1] + l


def join_text(lines):
    paras, cur = [], []
    for l in lines:
        if not l:
            continue
        if cur and LIST_RE.match(l):
            paras.append(cur)
            cur = []
        cur.append(l)
    if cur:
        paras.append(cur)
    out = []
    for p in paras:
        t = p[0]
        for l in p[1:]:
            t = dehyphen_join(t, l)
        out.append(t.strip())
    return "\n".join(out)


def parse_title(title):
    name, usage, cost, cond = title, None, None, None
    for m in re.finditer(r"\(([^)]*)\)", title):
        inner = m.group(1)
        mm = re.match(r"^Recharge (\d)(?:[–-](\d))?$", inner)
        if mm:
            usage = {"recharge": {"min": int(mm.group(1)), "max": int(mm.group(2) or 6)}}
            continue
        mm = re.match(r"^(\d+)/Day$", inner)
        if mm:
            usage = {"per_day": int(mm.group(1))}
            continue
        mm = re.match(r"^(\d+)/Day, or (\d+)/Day in Lair$", inner)
        if mm:
            usage = {"per_day_in_lair": {"normal": int(mm.group(1)), "lair": int(mm.group(2))}}
            continue
        mm = re.match(r"^Costs (\d+) Actions?$", inner)
        if mm:
            cost = int(mm.group(1))
            continue
        if inner == "Recharges after a Short or Long Rest":
            usage = {"on_rest": "short_or_long"}
            continue
        cond = inner
    return re.sub(r"\s*\([^)]*\)", "", title).strip(), usage, cost, cond


def dmg_parts(text):
    dmg = [{"average": int(x), "dice": re.sub(r"\s+", "", d), "type": t.lower()} for x, d, t in
           re.findall(r"(\d+) \((\d+d\d+(?: ?[+-] ?\d+)?)\) (\w+) damage", text)]
    if dmg:
        return dmg
    return [{"average": int(x), "type": t.lower()} for x, t in re.findall(r"(?<![\d(])(\d+) (\w+) damage", text)]


def parse_attack(text):
    m = re.match(r"^(Melee or Ranged|Melee|Ranged) Attack Roll: ([+-]\d+)(?: to hit)?(?: \(([^)]*)\))?, (.*?)\. Hit: (.*)$", text, re.S)
    if not m:
        return None
    kind = {"Melee": "melee", "Ranged": "ranged", "Melee or Ranged": "melee_or_ranged"}[m.group(1)]
    a = {"kind": kind, "to_hit": int(m.group(2))}
    reach = re.search(r"reach (\d+) ft", m.group(4))
    rng = re.search(r"range (\d+)(?:/(\d+))? ft", m.group(4))
    if reach:
        a["reach"] = int(reach.group(1))
    if rng:
        a["range"] = int(rng.group(1))
        if rng.group(2):
            a["long_range"] = int(rng.group(2))
    hit = re.split(r"(?<=damage)\. |(?<=condition)\. ", m.group(5), maxsplit=1)[0]
    # drop the alternative damage that only applies under a condition ("or 12 (2d8 + 3) Piercing damage if ...")
    hit = re.sub(r"(?:—or|, or) \d+ \([^)]*\) \w+ damage if [^—,.]*", "", hit)
    hit = re.sub(r", plus \d+ \([^)]*\) \w+ damage if [^.]*", "", hit)
    dm = dmg_parts(hit)
    if dm:
        a["damage"] = dm
    return a


def parse_save(text):
    m = re.search(r"\b(Strength|Dexterity|Constitution|Intelligence|Wisdom|Charisma) Saving Throw: DC (\d+)", text)
    if not m:
        return None
    s = {"ability": ABIL_LONG[m.group(1)], "dc": int(m.group(2))}
    rest = text[m.end():]
    fm = re.search(r"Failure: (.*?)(?: Success:| Failure or Success:|$)", rest, re.S)
    if fm:
        dm = dmg_parts(re.split(r"(?<=damage)\. ", fm.group(1), maxsplit=1)[0])
        if dm:
            s["damage"] = dm
    if re.search(r"Success: Half damage", rest):
        s["half_on_success"] = True
    return s


SPELL_IDS = set()
for f in glob.glob(str(APP / "content" / "srd-5.2.1" / "spells" / "*.json")):
    for e in json.load(open(f))["elements"]:
        SPELL_IDS.add(e["id"])


def spell_id(name):
    cand = "ID_2024_SPELL_" + re.sub(r"[^A-Z0-9]+", "_", name.upper()).strip("_")
    return cand if cand in SPELL_IDS else None


def parse_spellcasting(text):
    lines = text.split("\n")
    lists = []
    for l in lines[1:]:
        m = re.match(r"^(At Will|(\d+)/Day( Each)?): (.*)$", l)
        if not m:
            return None if not lists else {"_bad": l}
        freq = "at_will" if m.group(1) == "At Will" else {"per_day": {"uses": int(m.group(2)), "each": bool(m.group(3))}}
        spells = []
        for it in split_top(m.group(4)):
            mm = re.match(r"^([^(]+?)(?: \((.*)\))?$", it)
            if not mm:
                raise ValueError("spell item " + repr(it) + " in " + repr(l))
            sp = {"name": mm.group(1).strip()}
            sid = spell_id(sp["name"])
            if sid:
                sp["id"] = sid
            if mm.group(2):
                sp["note"] = mm.group(2)
            spells.append(sp)
        lists.append({"frequency": freq, "spells": spells})
    if not lists:
        return None
    b = {}
    am = re.search(r"using (Strength|Dexterity|Constitution|Intelligence|Wisdom|Charisma) as the spellcasting ability", lines[0])
    if am:
        b["ability"] = ABIL_LONG[am.group(1)]
    dm = re.search(r"spell save DC (\d+)", lines[0])
    if dm:
        b["save_dc"] = int(dm.group(1))
    hm = re.search(r"([+-]\d+) to hit with spell attacks", lines[0])
    if hm:
        b["attack_bonus"] = int(hm.group(1))
    b["lists"] = lists
    return b


def feature(title, lines):
    name, usage, cost, cond = parse_title(title)
    f = {"name": name, "text": join_text(lines)}
    if usage:
        f["usage"] = usage
    if cost is not None:
        f["cost"] = cost
    if cond:
        f["condition"] = cond
    a = parse_attack(f["text"])
    if a:
        f["attack"] = a
    s = parse_save(f["text"])
    if s:
        f["save"] = s
    if name == "Spellcasting":
        sc = parse_spellcasting(f["text"])
        if sc:
            if "_bad" in sc:
                raise ValueError("spell list " + sc["_bad"])
            f["spellcasting"] = sc
    return f


# ---------------------------------------------------------------- block
# Printed values that contradict the document's own rules (see COVERAGE.md "Source quirks"); the SRD Rules Glossary
# XP table says CR 12 = 8,400 XP while the Archmage block prints 8,000.
OVERRIDES = {"Archmage": {"xp": 8400}}


def parse_block(name, type_line, body):
    d = {"ruleset": "dnd2024"}
    d.update(parse_type_line(type_line))
    m = INIT_RE.match(body[0])
    d["ac"] = [{"value": int(m.group(1))}]
    init_mod, init_score = int(m.group(2)), int(m.group(3))
    d["initiative"] = {"modifier": init_mod, "score": init_score}
    i = 1
    d["hp"] = parse_hp(body[i]); i += 1
    # speed may wrap until the MOD SAVE row
    sp = body[i]; i += 1
    while not body[i].startswith("MOD SAVE"):
        sp += " " + body[i]; i += 1
    d["speed"] = parse_speed(sq(sp))
    i += 1
    abil = " ".join(body[i:i + 2]).replace("\ufffd", ""); i += 2
    # PDF glitch (Young White Dragon): "Int 6 -2 2" lost the minus of the save column; restore it from the modifier
    abil = re.sub(r"(\d+) (-\d+) (\d+)(?= |$)", lambda m: f"{m.group(1)} {m.group(2)} -{m.group(3)}" if m.group(2)[0] == "-" else m.group(0), abil)
    ab, saves = {}, {}
    for k, sc, mod, sv in re.findall(r"(Str|Dex|Con|Int|Wis|Cha) ?(\d+) ([+-]\d+) ([+-]\d+)", abil):
        key = ABIL[k]
        ab[key] = int(sc)
        if int(mod) != (int(sc) - 10) // 2:
            raise ValueError(f"{name}: mod {k} {sc} {mod}")
        if int(sv) != int(mod):
            saves[key] = int(sv)
    if len(ab) != 6:
        raise ValueError(name + " abilities " + abil)
    d["abilities"] = ab
    if saves:
        d["saves"] = saves
    # labelled header lines until CR
    cur, vals = None, {}
    while True:
        l = body[i]; i += 1
        if not l:
            continue
        lab = next((x for x in LABELS if l == x or l.startswith(x + " ")), None)
        if lab and (cur is None or LABELS.index(lab) > LABELS.index(cur)):
            cur = lab
            vals[cur] = l[len(lab):].strip()
        else:
            vals[cur] = (vals[cur] + " " + l).strip()
        if cur == "CR":
            break
    for k in vals:
        vals[k] = sq(vals[k])
    if "Skills" in vals:
        sk, notes = parse_skills(vals["Skills"])
        d["skills"] = sk
        if notes:
            d["skill_notes"] = notes
    if "Vulnerabilities" in vals:
        d["damage_vulnerabilities"] = parse_damage_mods(vals["Vulnerabilities"])
    if "Resistances" in vals:
        d["damage_resistances"] = parse_damage_mods(vals["Resistances"])
    if "Immunities" in vals:
        dm, cond = parse_immunities(vals["Immunities"])
        if dm:
            d["damage_immunities"] = dm
        if cond:
            d["condition_immunities"] = cond
    if "Gear" in vals:
        d["gear"] = split_top(vals["Gear"])
    d["senses"] = parse_senses(vals["Senses"])
    langs, tele = parse_languages(vals["Languages"])
    d["languages"] = langs
    if tele:
        d["telepathy"] = tele
    cr, xp, xp_lair, pb = parse_cr(vals["CR"])
    d["cr"], d["xp"], d["proficiency_bonus"] = cr, xp, pb
    if xp_lair:
        d["xp_lair"] = xp_lair
    for field, value in OVERRIDES.get(name, {}).items():
        d[field] = value
    # sections
    sec, chunks = None, {}
    for l in body[i:]:
        if l in SECTIONS:
            sec = SECTIONS[l]
            chunks[sec] = []
        elif sec is not None:
            chunks[sec].append(l)
        elif l:
            raise ValueError(f"{name}: text before section: {l}")
    for sec in ("traits", "actions", "bonus_actions", "reactions"):
        ents = split_entries(chunks.get(sec, []))
        feats = []
        for title, lines in ents:
            if title is None:
                raise ValueError(f"{name}/{sec}: untitled text {lines[:1]}")
            feats.append(feature(title, lines))
        d[sec] = feats
    if "legendary" in chunks:
        ents = split_entries(chunks["legendary"])
        leg = {"preamble": "", "actions": []}
        for title, lines in ents:
            if title is None:
                txt = join_text(lines)
                leg["preamble"] = txt
                m = re.match(r"^Legendary Action Uses: (\d+)(?: \((\d+) in Lair\))?\. (.*)$", txt, re.S)
                if not m:
                    raise ValueError(name + " legendary " + txt[:60])
                leg["per_round"] = int(m.group(1))
                if m.group(2):
                    leg["lair_uses"] = int(m.group(2))
                leg["preamble"] = txt
            else:
                leg["actions"].append(feature(title, lines))
        d["legendary"] = leg
    return d


def main():
    lines = load_lines()
    out = []
    for name, tl, body in segment(lines):
        out.append({"name": name, "data": parse_block(name, tl, body)})
    (HERE / "monsters.json").write_text(json.dumps(out, indent=1, ensure_ascii=False))
    print("monsters", len(out))


if __name__ == "__main__":
    main()
