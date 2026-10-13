#!/usr/bin/env python3
"""Step 2: raw_cols.txt -> monsters.json (scratch).  See docs/monster-model.md for the target shape.

Pipeline: segment stat blocks (size line + Armor Class line) -> header fields -> entries -> typed values.
Anything that cannot be parsed raises, so extraction problems are loud rather than silent.
"""
import json, re, sys
from pathlib import Path
HERE = Path(__file__).resolve().parent

SIZES = ["Tiny", "Small", "Medium", "Large", "Huge", "Gargantuan"]
LABELS = ["Armor Class", "Hit Points", "Speed", "Saving Throws", "Skills", "Damage Vulnerabilities",
          "Damage Resistances", "Damage Resistance", "Damage Immunities", "Condition Immunities", "Senses", "Languages", "Challenge"]
SECTIONS = {"Actions": "actions", "Bonus Actions": "bonus_actions", "Reactions": "reactions",
            "Legendary Actions": "legendary"}
ABIL = {"Str": "str", "Dex": "dex", "Con": "con", "Int": "int", "Wis": "wis", "Cha": "cha"}
SMALL = {"of", "the", "and", "or", "to", "in", "a", "an", "on", "with", "from", "for", "at", "as", "by"}
FOOTER = re.compile(r"^System Reference Document 5\.1 \d+$")
HEADING = re.compile(r"^(Monsters \([A-Z](?:[–-][A-Z])?\)|Appendix MM-[AB]:.*|Miscellaneous Creatures|Nonplayer Characters|Monsters)$")


def clean(s):
    """Normalise PDF artefacts: soft hyphens vanish, hyphen/minus variants become '-', line-end '-\xad\u2010' pairs collapse."""
    s = s.replace("\xad", "").replace("\u2010", "-").replace("\u2011", "-").replace("\u2212", "-").replace("\u2009", " ")
    return re.sub(r"-{2,}", "-", s)


def num(s):
    return int(s.replace(",", ""))


# ---------------------------------------------------------------- segmentation
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


STOP = re.compile(r"^(Half-Dragon Template|Variant: .*|Customizing NPCs|Appendix .*|Nonplayer|Nonplayer Characters)$")
SIZE_RE = re.compile(r"^(Tiny|Small|Medium|Large|Huge|Gargantuan)\b")


def strip_trailing_headings(body):
    """Drop group headings ("Golems", "Dragons, Chromatic" + "Black Dragon") that precede the next block."""
    body = list(body)
    while body and not body[-1]:
        body.pop()
    TERM = (".", ")", ":", "!", "?")
    k = len(body)
    while k > 0 and (not body[k - 1] or (len(body[k - 1]) <= 40 and not body[k - 1].endswith(TERM))):
        k -= 1
    run = [x for x in body[k:] if x]
    if run and len(run) <= 3 and k > 0 and body[k - 1].endswith(TERM):
        return body[:k]
    return body


def segment(lines):
    starts = []
    for i, l in enumerate(lines):
        if not SIZE_RE.match(l):
            continue
        if lines[i + 1].startswith("Armor Class") or (i + 2 < len(lines) and lines[i + 2].startswith("Armor Class")
                                                       and lines[i + 1] and not lines[i + 1].startswith(("STR", "Armor"))):
            j = i - 1
            while not lines[j]:
                j -= 1
            starts.append((j, i))
    blocks = []
    for k, (n, t) in enumerate(starts):
        end = starts[k + 1][0] if k + 1 < len(starts) else len(lines)
        body = lines[t:end]
        for q, l in enumerate(body):          # prose sections between/after stat blocks are not part of the block
            if STOP.match(l):
                body = body[:q]
                break
        body = strip_trailing_headings(body)
        blocks.append((lines[n], body))
    return blocks


# ---------------------------------------------------------------- header
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
    m = re.match(r"^(Tiny|Small|Medium|Large|Huge|Gargantuan)(?: or (Tiny|Small|Medium|Large|Huge|Gargantuan))? (.*)$", text)
    size, alt, rest = m.group(1), m.group(2), m.group(3)
    sizes = [size.lower()] + ([alt.lower()] if alt else [])
    tm = re.match(r"^(?:swarm of (Tiny|Small|Medium|Large|Huge|Gargantuan) )?([a-z]+)(?: \(([^)]*)\))?, (.*)$", rest)
    if not tm:
        raise ValueError("type " + rest)
    swarm = tm.group(1).lower() if tm.group(1) else None
    ctype = tm.group(2)
    if swarm and ctype.endswith("s"):
        ctype = ctype[:-1]
    tags = [t.strip() for t in tm.group(3).split(",")] if tm.group(3) else []
    align = tm.group(4)
    return sizes, swarm, ctype, tags, align.strip()


def parse_ac(text):
    # "15 (leather armor, shield)" | "10 in humanoid form, 11 (natural armor) in bear and hybrid form"
    out = []
    for part in split_top(text):
        m = re.match(r"^(\d+)(?: \(([^)]*)\))?(?: (.*))?$", part)
        if not m:
            raise ValueError("ac " + text)
        e = {"value": int(m.group(1))}
        if m.group(2):
            e["source"] = m.group(2)
        if m.group(3):
            e["condition"] = m.group(3)
        out.append(e)
    return out


def parse_hp(text):
    m = re.match(r"^(\d+)(?: \((\d+d\d+(?: ?[+-] ?\d+)?)\))?$", text)
    if not m:
        raise ValueError("hp " + text)
    e = {"average": int(m.group(1))}
    if m.group(2):
        e["formula"] = re.sub(r"\s+", "", m.group(2))
    return e


def parse_speed(text):
    sp = {}
    for part in split_top(text):
        m = re.match(r"^(?:(walk|burrow|climb|fly|swim) )?(\d+) ft\.(?: \((.*)\))?$", part)
        if not m:
            raise ValueError("speed " + text)
        mode = m.group(1) or "walk"
        sp[mode] = int(m.group(2))
        if m.group(3) == "hover":
            sp["hover"] = True
        elif m.group(3):
            sp["note"] = (sp.get("note", "") + "; " if sp.get("note") else "") + f"{mode}: {m.group(3)}"
    return sp


def parse_bonus_map(text, keymap=None):
    out, notes = {}, []
    for part in split_top(text):
        m = re.match(r"^(.*?) ([+-]\d+)(?: \((.*)\))?$", part)
        if not m:
            raise ValueError("bonus " + text)
        k = m.group(1)
        k = keymap[k] if keymap else k.lower()
        out[k] = int(m.group(2))
        if m.group(3):
            notes.append(f"{m.group(1)}: {m.group(3)}")
    return out, notes


DTYPES = ["acid", "bludgeoning", "cold", "fire", "force", "lightning", "necrotic", "piercing", "poison",
          "psychic", "radiant", "slashing", "thunder"]


def parse_damage_mods(text):
    out = []
    for clause in [c.strip() for c in text.split(";") if c.strip()]:
        words = re.findall(r"[a-z]+", clause)
        types = [w for w in words if w in DTYPES]
        e = {"types": types}
        stripped = re.sub(r"\b(" + "|".join(DTYPES) + r")\b|,|\band\b|\s+", "", clause)
        if stripped:
            e["text"] = clause
        out.append(e)
    return out


def parse_senses(text):
    s = {}
    for part in split_top(text):
        m = re.match(r"^passive Perception (\d+)$", part)
        if m:
            s["passive_perception"] = int(m.group(1))
            continue
        m = re.match(r"^(blindsight|darkvision|tremorsense|truesight) (\d+) ft\.(?: or (\d+) ft\. while deafened)?(?: \((.*)\))?$", part)
        if not m:
            raise ValueError("sense " + part)
        s[m.group(1)] = int(m.group(2))
        if m.group(3):
            s["deafened_range"] = int(m.group(3))
        if m.group(4) == "blind beyond this radius":
            s["blind_beyond"] = True
        elif m.group(4):
            s.setdefault("notes", []).append(f"{m.group(1)}: {m.group(4)}")
    return s


def parse_languages(text):
    if text in ("—", "-", "–"):
        return [], None
    items, tele = [], None
    for it in split_top(text):
        m = re.match(r"^telepathy (\d+) ft\.$", it)
        if m:
            tele = int(m.group(1))
        else:
            items.append(it)
    return items, tele


def parse_cr(text):
    m = re.match(r"^(\d+(?:/\d+)?) \(([\d,]+) XP\)$", text)
    if not m:
        raise ValueError("cr " + text)
    return m.group(1), num(m.group(2))


def pb_for(cr):
    v = eval_cr(cr)
    return max(2, int(-(-v // 4)) + 1) if v >= 1 else 2


def eval_cr(cr):
    if "/" in cr:
        a, b = cr.split("/")
        return int(a) / int(b)
    return int(cr)


# ---------------------------------------------------------------- entries
TITLE_RE = re.compile(r"^((?:[A-Z][\w’'\-]*|\([^)]*\))(?: (?:[A-Z][\w’'\-]*|\([^)]*\)|" + "|".join(SMALL) + r"))*)\.(?: |$)")
LIST_RE = re.compile(r"^(At will|\d+/day|Cantrips|\d+(?:st|nd|rd|th) level|Constant)", re.I)


def is_title(line):
    m = TITLE_RE.match(line)
    if not m:
        return None
    title = m.group(1)
    words = re.sub(r"\([^)]*\)", "(x)", title).split()
    if len(words) > 9 or words[0] in SMALL:
        return None
    return m


def split_entries(lines):
    """lines -> list of [title, [text lines]]; text before any title gets title None."""
    entries = []
    for l in lines:
        if not l:
            if entries:
                entries[-1][1].append("")
            continue
        m = is_title(l)
        prev = entries[-1][1] if entries else None
        prev_last = next((x for x in reversed(prev) if x), "") if prev else ""
        sentence_end = (not prev) or prev_last.endswith((".", ")", ":", "!", "?")) or LIST_RE.match(prev_last) is not None \
            or bool(prev is not None and prev and prev[-1] == "")
        if m and sentence_end:
            entries.append([m.group(1), [l[m.end():]]])
        elif entries:
            entries[-1][1].append(l)
        else:
            entries.append([None, [l]])
    return entries


def join_text(lines):
    """Join wrapped lines. Blank lines are column/page-break artefacts and are ignored; spell-list lines
    ("At will: ...", "1st level (4 slots): ...") stay on their own line (separated by \n)."""
    paras, cur = [], []
    for l in lines:
        if l == "":
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
            t += l if t.endswith("-") else " " + l
        out.append(t.strip())
    return "\n".join(out)


def split_lore(lines):
    """The SRD prints a flavour paragraph after some stat blocks. It sits in the last entry, after a blank line,
    starts with a capital letter and follows a sentence end. Returns (entry_lines, lore_text|None)."""
    paras, cur = [], []
    for l in lines:
        if l == "":
            if cur:
                paras.append(cur)
                cur = []
        else:
            cur.append(l)
    if cur:
        paras.append(cur)
    for k in range(1, len(paras)):
        if paras[k][0][0].isupper() and paras[k - 1][-1].endswith(".") and not LIST_RE.match(paras[k][0]):
            keep = [x for p in paras[:k] for x in p]
            lore = join_text([x for p in paras[k:] for x in p])
            return keep, lore
    return lines, None


def parse_title(title):
    name, usage, cost, cond = title, None, None, None
    for m in re.finditer(r"\(([^)]*)\)", title):
        inner = m.group(1)
        mm = re.match(r"^Recharge (\d)(?:[–-](\d))?$", inner)
        if mm:
            lo = int(mm.group(1))
            usage = {"recharge": {"min": lo, "max": int(mm.group(2) or 6)}}
            continue
        mm = re.match(r"^(\d+)/Day$", inner)
        if mm:
            usage = {"per_day": int(mm.group(1))}
            continue
        mm = re.match(r"^Costs (\d+) Actions?$", inner)
        if mm:
            cost = int(mm.group(1))
            continue
        if inner == "Recharges after a Short or Long Rest":
            usage = {"on_rest": "short_or_long"}
            continue
        cond = inner
    name = re.sub(r"\s*\([^)]*\)", "", title).strip()
    return name, usage, cost, cond




def parse_attack(text):
    m = re.match(r"^(Melee|Ranged|Melee or Ranged) (Weapon|Spell) Attack: ([+-]\d+) to hit(?: \([^)]*\))?, ([^.]*?)\.\s*(?:(.*?)\.\s*)?Hit: (.*)$", text, re.S)
    if not m:
        return None
    kind = {"Melee": "melee", "Ranged": "ranged", "Melee or Ranged": "melee_or_ranged"}[m.group(1)] + "_" + m.group(2).lower()
    reach = re.search(r"reach (\d+) ft", m.group(4))
    rng = re.search(r"range (\d+)(?:/(\d+))? ft", m.group(4))
    a = {"kind": kind, "to_hit": int(m.group(3))}
    if reach:
        a["reach"] = int(reach.group(1))
    if rng:
        a["range"] = int(rng.group(1))
        if rng.group(2):
            a["long_range"] = int(rng.group(2))
    dmg = [{"average": int(x), "dice": re.sub(r"\s+", "", d), "type": t} for x, d, t in
           re.findall(r"(\d+) \((\d+d\d+(?: ?[+-] ?\d+)?)\) (\w+) damage", m.group(6))]
    flat = [{"average": int(x), "dice": None, "type": t} for x, t in re.findall(r"(?<![\d(])(\d+) (\w+) damage", m.group(6))
            if not any(str(x) == str(d["average"]) for d in dmg)]
    if dmg:
        a["damage"] = dmg
    elif flat:
        a["damage"] = [{"average": f["average"], "type": f["type"]} for f in flat]
    return a


def feature(title, lines, with_attack):
    name, usage, cost, cond = parse_title(title)
    f = {"name": name, "text": join_text(lines)}
    if usage:
        f["usage"] = usage
    if cost is not None:
        f["cost"] = cost
    if cond:
        f["condition"] = cond
    if with_attack:
        a = parse_attack(f["text"])
        if a:
            f["attack"] = a
    return f


# ---------------------------------------------------------------- block
def parse_block(name, body, appendix):
    errs = []
    i = 0
    tline = body[0]
    i = 1
    if not body[1].startswith("Armor Class"):
        tline += " " + body[1]
        i = 2
    sizes, swarm, ctype, tags, align = parse_type_line(tline)
    # header fields
    fields, cur, abil = {}, None, None
    while i < len(body):
        l = body[i]
        if re.match(r"^STR DEX CON INT WIS CHA$", l):
            j = i + 1
            while not body[j]:
                j += 1
            vals = re.findall(r"(\d+) \(([+-]\d+)\)", body[j])
            if len(vals) != 6:
                raise ValueError(f"{name}: ability row {body[j]!r}")
            abil = dict(zip(["str", "dex", "con", "int", "wis", "cha"], [int(v[0]) for v in vals]))
            i = j + 1
            cur = None
            continue
        sm = re.match(r"^(.+?[a-z]) (Senses (?:passive|blindsight|darkvision).*)$", l)
        if sm and cur:  # PDF quirk: a label glued onto the end of the previous wrapped line
            fields[cur] += " " + sm.group(1)
            l = sm.group(2)
        lab = next((x for x in LABELS if l.startswith(x + " ") or l == x), None)
        if lab:
            cur = lab
            fields[lab] = l[len(lab):].strip()
            if lab == "Challenge":
                i += 1
                break
        elif l and cur:
            fields[cur] += " " + l
        i += 1
    rest = body[i:]
    for k in ("Armor Class", "Hit Points", "Speed", "Senses", "Languages", "Challenge"):
        if k not in fields:
            raise ValueError(f"{name}: missing {k}")
    if abil is None:
        raise ValueError(f"{name}: abilities")
    cr, xp = parse_cr(fields["Challenge"])
    d = {"ruleset": "dnd2014", "size": sizes}
    if swarm:
        d["swarm_of"] = swarm
    d.update(creature_type=ctype, tags=tags, alignment=align, ac=parse_ac(fields["Armor Class"]),
             hp=parse_hp(fields["Hit Points"]), speed=parse_speed(fields["Speed"]), abilities=abil)
    if "Saving Throws" in fields:
        d["saves"], _ = parse_bonus_map(fields["Saving Throws"], ABIL)
    if "Skills" in fields:
        d["skills"], notes = parse_bonus_map(fields["Skills"])
        if notes:
            d["skill_notes"] = notes
    for lab, key in (("Damage Vulnerabilities", "damage_vulnerabilities"), ("Damage Resistances", "damage_resistances"), ("Damage Resistance", "damage_resistances"),
                     ("Damage Immunities", "damage_immunities")):
        if lab in fields:
            d[key] = parse_damage_mods(fields[lab])
    if "Condition Immunities" in fields:
        d["condition_immunities"] = [c.strip() for c in fields["Condition Immunities"].split(",")]
    d["senses"] = parse_senses(fields["Senses"])
    d["languages"], tele = parse_languages(fields["Languages"])
    if tele:
        d["telepathy"] = tele
    d.update(cr=cr, xp=xp, proficiency_bonus=pb_for(cr))
    # sections
    sections = {"traits": []}
    cur_sec = "traits"
    buf = {"traits": []}
    for l in rest:
        if l in SECTIONS:
            cur_sec = SECTIONS[l]
            buf.setdefault(cur_sec, [])
            continue
        buf[cur_sec].append(l)
    for sec in ("traits", "actions", "bonus_actions", "reactions"):
        ents = split_entries(buf.get(sec, []))
        feats = []
        for t, ls in ents:
            if t is None:
                if any(ls):
                    raise ValueError(f"{name}: text before title in {sec}: {ls[:2]}")
                continue
            feats.append([t, ls])
        d[sec] = feats
    last_sec = next((x for x in ("reactions", "bonus_actions", "actions", "traits") if d[x]), None)
    if appendix and "legendary" not in buf and last_sec:
        ls2, lore = split_lore(d[last_sec][-1][1])
        d[last_sec][-1][1] = ls2
        if lore:
            d["description"] = lore
    for sec in ("traits", "actions", "bonus_actions", "reactions"):
        d[sec] = [feature(t, ls, sec != "traits") for t, ls in d[sec]]
    if "legendary" in buf:
        ents = split_entries(buf["legendary"])
        pre = []
        feats = []
        for t, ls in ents:
            if t is None:
                pre = ls
                continue
            feats.append(feature(t, ls, False))
        # preamble: text before first title (split_entries gave None-first) or first lines
        pre_text = join_text(pre)
        m = re.search(r"can take (\d+) legendary action", pre_text)
        d["legendary"] = {"per_round": int(m.group(1)) if m else None, "preamble": pre_text, "actions": feats}
    return d


def main():
    lines = load_lines()
    blocks = segment(lines)
    out = []
    problems = 0
    first_app = next(i for i, (n, _) in enumerate(blocks) if n == "Ape")  # Appendix MM-A starts at "Ape"
    first_npc = next(i for i, (n, _) in enumerate(blocks) if n == "Acolyte")  # Appendix MM-B
    for idx, (name, body) in enumerate(blocks):
        name = re.sub(r"\s+", " ", name.strip()).replace("’", "'")
        try:
            out.append({"name": name, "section": "monsters" if idx < first_app else ("npcs" if idx >= first_npc else "creatures"), "data": parse_block(name, body, idx >= first_app)})
        except Exception as e:  # noqa
            problems += 1
            print("FAIL", name, "->", repr(e)[:200], file=sys.stderr)
    (HERE / "monsters.json").write_text(json.dumps(out, indent=1, ensure_ascii=False))
    print("parsed", len(out), "failed", problems)


if __name__ == "__main__":
    main()
