import type { ArmorFacts, SpellFacts, WeaponFacts } from "../types";
import type { ItemDef, SpellDef } from "./data-types";

const DEFAULT_COMPONENTS = "VS";
const INSTANT = "Instantaneous";

export function spellFacts(s: SpellDef): SpellFacts {
  const components = s.components ?? DEFAULT_COMPONENTS;
  return {
    castingTime: s.time,
    range: s.range,
    duration: s.duration ?? (s.concentration ? "Concentration, up to 1 minute" : INSTANT),
    verbal: components.includes("V"),
    somatic: components.includes("S"),
    material: components.includes("M"),
    materialText: s.material ?? "",
    concentration: s.concentration,
    ritual: s.ritual,
  };
}

export function armorFacts(i: ItemDef): ArmorFacts | undefined {
  const a = i.armor;
  if (!a) return undefined;
  const group = a.shield ? "Shield" : a.dexCap === null ? "Light" : a.dexCap === 0 ? "Heavy" : "Medium";
  const addsDex = group === "Light" || group === "Medium";
  const acText = a.shield ? `+${a.base}` : addsDex ? `${a.base} + Dex${a.dexCap === null ? "" : ` (max ${a.dexCap})`}` : String(a.base);
  return {
    group, acText, acBase: a.base, addsDex, dexCap: addsDex ? a.dexCap : null,
    strength: a.strength ?? null, stealthDisadvantage: !!a.stealth,
  };
}

export function weaponFacts(i: ItemDef): WeaponFacts | undefined {
  const w = i.weapon;
  if (!w) return undefined;
  const [damage = "1d4", ...type] = w.damage.split(" ");
  const properties = w.properties ? w.properties.split(",").map((p) => p.trim()).filter(Boolean).map((p) => p.charAt(0).toUpperCase() + p.slice(1)) : [];
  return {
    damage, damageType: type.join(" "), properties, mastery: w.mastery ?? null, range: w.range ?? null,
    proficiency: w.martial ? "Martial" : "Simple", kind: /ammunition/i.test(w.properties) ? "Ranged" : "Melee",
  };
}
