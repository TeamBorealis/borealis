import type {
  CreatureAbility, HomebrewArmorClassValue, HomebrewAttack, HomebrewAttackKind, HomebrewChallengeValue, HomebrewCreatureEntry, HomebrewDamagePart,
  HomebrewHitPointsValue, HomebrewSensesValue, HomebrewUsage, Ruleset,
} from "../../../api/contract";

/** The six abilities in stat block order. */
export const ABILITIES: readonly { key: CreatureAbility; label: string; short: string }[] = [
  { key: "str", label: "Strength", short: "STR" }, { key: "dex", label: "Dexterity", short: "DEX" }, { key: "con", label: "Constitution", short: "CON" },
  { key: "int", label: "Intelligence", short: "INT" }, { key: "wis", label: "Wisdom", short: "WIS" }, { key: "cha", label: "Charisma", short: "CHA" },
];

/** Attack kinds with the words each edition's stat block opens the attack with. */
export const ATTACK_KINDS: readonly { value: HomebrewAttackKind; label: string }[] = [
  { value: "melee", label: "Melee Attack Roll" }, { value: "ranged", label: "Ranged Attack Roll" }, { value: "melee_or_ranged", label: "Melee or Ranged Attack Roll" },
  { value: "melee_weapon", label: "Melee Weapon Attack" }, { value: "ranged_weapon", label: "Ranged Weapon Attack" },
  { value: "melee_or_ranged_weapon", label: "Melee or Ranged Weapon Attack" }, { value: "melee_spell", label: "Melee Spell Attack" },
  { value: "ranged_spell", label: "Ranged Spell Attack" },
];

export const DAMAGE_TYPES: readonly string[] = ["acid", "bludgeoning", "cold", "fire", "force", "lightning", "necrotic", "piercing", "poison", "psychic", "radiant", "slashing", "thunder"];

/** An ability modifier: floor((score - 10) / 2). */
export const modifier = (score: number): number => Math.floor((score - 10) / 2);
export const signed = (n: number): string => (n >= 0 ? `+${n}` : `${n}`);
export const capitalised = (t: string): string => t.charAt(0).toUpperCase() + t.slice(1);

const obj = (v: unknown): Record<string, unknown> => (typeof v === "object" && v !== null && !Array.isArray(v) ? v as Record<string, unknown> : {});
const num = (v: unknown): number | undefined => (typeof v === "number" && Number.isFinite(v) ? v : undefined);
const str = (v: unknown): string => (typeof v === "string" ? v : "");

/** `NdS`, `NdS+M`, `NdS-M` (spaces allowed) as `[n, s, m]`, or null. */
export function parseDice(text: string): [number, number, number] | null {
  const m = /^(\d+)d(\d+)([+-]\d+)?$/.exec(text.replace(/\s+/g, ""));
  if (!m) return null;
  const n = Number(m[1]); const s = Number(m[2]);
  return n >= 1 && s >= 2 ? [n, s, Number(m[3] ?? 0)] : null;
}

/** The average a stat block prints for dice, rounded down (`2d8+2` -> 11). */
export function diceAverage(text: string): number | null {
  const d = parseDice(text);
  return d ? Math.floor((d[0] * (d[1] + 1)) / 2) + d[2] : null;
}

/** Dice as the stat block shows them in text: `1d6 + 2`. */
export function diceWords(text: string): string {
  const d = parseDice(text);
  if (!d) return text.trim();
  return d[2] === 0 ? `${d[0]}d${d[1]}` : `${d[0]}d${d[1]} ${d[2] > 0 ? "+" : "-"} ${Math.abs(d[2])}`;
}

export function armorClassOf(v: unknown): HomebrewArmorClassValue {
  const o = obj(v);
  const entries = Array.isArray(o.entries) ? o.entries.map((e) => {
    const x = obj(e);
    return { value: num(x.value) ?? 10, ...(typeof x.source === "string" ? { source: x.source } : {}), ...(typeof x.condition === "string" ? { condition: x.condition } : {}) };
  }) : [];
  return { entries, note: str(o.note) };
}

export function hitPointsOf(v: unknown): HomebrewHitPointsValue {
  const o = obj(v);
  return { average: num(o.average) ?? null, formula: str(o.formula), note: str(o.note) };
}

export function sensesOf(v: unknown): HomebrewSensesValue {
  const o = obj(v);
  return { ...(o as Partial<HomebrewSensesValue>), passive_perception: num(o.passive_perception) ?? null };
}

export function challengeOf(v: unknown): HomebrewChallengeValue {
  const o = obj(v);
  const n = (k: string) => num(o[k]) ?? null;
  return { cr: str(o.cr), note: str(o.note), xp: n("xp"), xpLair: n("xpLair"), proficiencyBonus: n("proficiencyBonus") };
}

/** XP by challenge rating (`Cr::XP_TABLE`). */
const XP: Readonly<Record<string, number>> = {
  "0": 10, "1/8": 25, "1/4": 50, "1/2": 100, 1: 200, 2: 450, 3: 700, 4: 1100, 5: 1800, 6: 2300, 7: 2900, 8: 3900, 9: 5000, 10: 5900, 11: 7200,
  12: 8400, 13: 10000, 14: 11500, 15: 13000, 16: 15000, 17: 18000, 18: 20000, 19: 22000, 20: 25000, 21: 33000, 22: 41000, 23: 50000, 24: 62000,
  25: 75000, 26: 90000, 27: 105000, 28: 120000, 29: 135000, 30: 155000,
};

/** The standard XP of a challenge rating, or null for none. */
export const standardXp = (cr: string): number | null => XP[cr] ?? null;

/** The proficiency bonus of a challenge rating: 2 up to CR 4, then 1 more every 4. */
export function crProficiency(cr: string): number | null {
  if (!(cr in XP)) return null;
  const n = cr.includes("/") ? 0 : Number(cr);
  return n <= 1 ? 2 : 2 + Math.floor((n - 1) / 4);
}

/** The usual passive Perception: 10 + the Perception bonus, else the Wisdom modifier. */
export function usualPassive(abilities: unknown, skills: unknown): number {
  const wis = num(obj(abilities).wis) ?? 10;
  return 10 + (num(obj(skills).perception) ?? modifier(wis));
}

export function entriesOf(v: unknown): HomebrewCreatureEntry[] {
  return Array.isArray(v) ? v.filter((e): e is HomebrewCreatureEntry => typeof e === "object" && e !== null).map((e) => ({ ...e, name: str(e.name), text: str(e.text) })) : [];
}

/** How often an entry is used, as the stat block shows it after the name. */
export function usageText(u: HomebrewUsage | undefined): string {
  if (!u) return "";
  if ("recharge" in u) return u.recharge.min === u.recharge.max ? `Recharge ${u.recharge.min}` : `Recharge ${u.recharge.min}–${u.recharge.max}`;
  if ("per_day" in u) return `${u.per_day}/Day`;
  if ("per_day_in_lair" in u) return `${u.per_day_in_lair.normal}/Day, or ${u.per_day_in_lair.lair}/Day in Lair`;
  return u.on_rest === "long" ? "Recharges after a Long Rest" : "Recharges after a Short or Long Rest";
}

/** The row's title: `Bite (Recharge 5–6)`. */
export function entryTitle(e: HomebrewCreatureEntry): string {
  const notes = [e.condition?.trim(), usageText(e.usage), e.cost && e.cost > 1 ? `Costs ${e.cost} Actions` : ""].filter(Boolean);
  const name = e.name.trim() || "Unnamed";
  return notes.length ? `${name} (${notes.join("; ")})` : name;
}

function damageWords(parts: readonly HomebrewDamagePart[], upper: boolean): string {
  return parts.map((d) => {
    const type = upper ? capitalised(d.type) : d.type;
    return d.dice ? `${d.average} (${diceWords(d.dice)}) ${type} damage` : `${d.average} ${type} damage`;
  }).join(" plus ");
}

/** The words of an attack as the stat block of `edition` prints them (the entry's text, written for the user). */
export function attackText(a: HomebrewAttack, edition: Ruleset | ""): string {
  const label = ATTACK_KINDS.find((k) => k.value === a.kind)?.label ?? "Attack";
  const isNew = edition !== "dnd2014";
  const melee = a.kind.startsWith("melee");
  const ranged = a.kind.includes("ranged");
  const where = [
    melee && a.reach ? `reach ${a.reach} ft.` : "",
    ranged && a.range ? `range ${a.range}${a.long_range ? `/${a.long_range}` : ""} ft.` : "",
  ].filter(Boolean).join(" or ");
  const damage = damageWords(a.damage ?? [], isNew);
  if (isNew) {
    const head = `${label}: ${signed(a.to_hit)}${where ? `, ${where}` : ""}`;
    return `${head}${head.endsWith(".") ? "" : "."} Hit: ${damage || "no damage"}.`;
  }
  return `${label}: ${signed(a.to_hit)} to hit${where ? `, ${where}` : ""}, one target. Hit: ${damage || "no damage"}.`;
}

/** A new damage part. */
export const newDamage = (): HomebrewDamagePart => ({ average: 1, type: "slashing" });
