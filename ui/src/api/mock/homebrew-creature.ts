import type { ElementFact, HomebrewCreatureEntry, HomebrewForm, HomebrewProblem } from "../types";
import { section } from "./monsters";

// The creature form's parts the web mock does the way `borealis-authoring` and the backend do (WB-20, spec 4.11): the
// sentences the stat block validator says at the fields (`check::monster_messages`), the codec's own (`codec::creature`)
// and the compendium's stat block for the live preview (`compendium_monster.rs`: AC, HP, CR facts, then the sections).

const XP: Readonly<Record<string, number>> = {
  "0": 10, "1/8": 25, "1/4": 50, "1/2": 100, 1: 200, 2: 450, 3: 700, 4: 1100, 5: 1800, 6: 2300, 7: 2900, 8: 3900, 9: 5000, 10: 5900, 11: 7200,
  12: 8400, 13: 10000, 14: 11500, 15: 13000, 16: 15000, 17: 18000, 18: 20000, 19: 22000, 20: 25000, 21: 33000, 22: 41000, 23: 50000, 24: 62000,
  25: 75000, 26: 90000, 27: 105000, 28: 120000, 29: 135000, 30: 155000,
};

const obj = (v: unknown): Record<string, unknown> => (typeof v === "object" && v !== null && !Array.isArray(v) ? v as Record<string, unknown> : {});
const text = (v: unknown): string => (typeof v === "string" ? v.trim() : "");
const dice = (t: string): [number, number, number] | null => {
  const m = /^(\d+)d(\d+)([+-]\d+)?$/.exec(t.replace(/\s+/g, ""));
  return m && Number(m[1]) >= 1 && Number(m[2]) >= 2 ? [Number(m[1]), Number(m[2]), Number(m[3] ?? 0)] : null;
};
const average = (t: string): number | null => { const d = dice(t); return d ? Math.floor((d[0] * (d[1] + 1)) / 2) + d[2] : null; };
const entries = (v: unknown): HomebrewCreatureEntry[] => (Array.isArray(v) ? v as HomebrewCreatureEntry[] : []);
const escape = (t: string): string => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** A creature draft's problems: the creature type first (nothing is written without it), then its stat block's. */
export function creatureProblems(form: HomebrewForm, elementId: string | null): HomebrewProblem[] {
  if (form.kind !== "creature" || !text(form.fields.creatureType)) return [];
  const f = form.fields;
  const at = (key: string, message: string, code = "invalid_monster_data", severity: HomebrewProblem["severity"] = "error"): HomebrewProblem =>
    ({ severity, path: `fields.${key}`, elementId, code, message });
  const out: HomebrewProblem[] = [];
  const hp = obj(f.hitPoints);
  if (text(hp.formula) && !dice(text(hp.formula))) return [at("hitPoints", "Write the hit dice as dice, like 2d8+2.", "form")];
  const ac = obj(f.armorClass);
  if (!(Array.isArray(ac.entries) && ac.entries.length) && !text(ac.note)) {
    out.push(at("armorClass", "Give the creature an Armor Class above 0, or words that say how it is worked out."));
  }
  if (typeof hp.average !== "number" && average(text(hp.formula)) === null && !text(hp.note)) {
    out.push(at("hitPoints", "Give the creature hit points above 0, or words that say how they are worked out."));
  }
  if (!["walk", "burrow", "climb", "fly", "swim"].some((k) => typeof obj(f.speed)[k] === "number")) out.push(at("speed", "Give the creature at least one speed."));
  for (const key of ["traits", "actions", "bonusActions", "reactions"]) {
    entries(f[key]).forEach((e, i) => {
      if (!text(e.name) || !text(e.text)) out.push(at(`${key}.${i}`, "Each entry needs a name and its text."));
    });
  }
  const c = obj(f.challenge);
  const cr = text(c.cr);
  if (cr && typeof c.xp === "number" && c.xp !== XP[cr]) {
    out.push(at("challenge", "The experience points differ from the usual amount for this challenge rating (kept as written).", "non_standard_monster_xp", "warning"));
  }
  return out;
}

/** The compendium's facts of a creature draft: Armor Class, Hit Points and Challenge as the stat block prints them. */
export function creatureFacts(form: HomebrewForm): ElementFact[] {
  const f = form.fields;
  const ac = obj(f.armorClass);
  const acText = Array.isArray(ac.entries) && ac.entries.length
    ? ac.entries.map((e) => { const x = obj(e); return `${String(x.value)}${text(x.source) ? ` (${text(x.source)})` : ""}${text(x.condition) ? ` ${text(x.condition)}` : ""}`; }).join(", ")
    : text(ac.note) || "—";
  const hp = obj(f.hitPoints);
  const avg = typeof hp.average === "number" ? hp.average : average(text(hp.formula));
  const hpText = avg !== null ? `${avg}${text(hp.formula) ? ` (${text(hp.formula).replace(/\s+/g, "")})` : ""}` : text(hp.note) || "—";
  const c = obj(f.challenge);
  const cr = text(c.cr);
  const xp = typeof c.xp === "number" ? c.xp : XP[cr];
  const challenge = cr ? `${cr} (${xp} XP)` : text(c.note) || "—";
  return [
    { label: "Type", value: "Monster" }, { label: "Armor Class", value: acText }, { label: "Hit Points", value: hpText }, { label: "Challenge", value: challenge },
  ];
}

/** The stat block's sections as the description pane shows them. */
export function creatureHtml(form: HomebrewForm): string {
  const list = (key: string) => entries(form.fields[key]).map((e) => [escape(text(e.name)), escape(text(e.text))] as const);
  return [["Traits", "traits"], ["Actions", "actions"], ["Bonus actions", "bonusActions"], ["Reactions", "reactions"]]
    .map(([title, key]) => section(title!, list(key!))).join("");
}
