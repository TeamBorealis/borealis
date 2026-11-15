import type { HomebrewRef } from "../../api/contract";

/** The weapon form's fields this rule reads (the descriptor's keys, `borealis-authoring` `item_kinds.rs`). */
export const WEAPON_KIND = "weapon";
export const PROPERTIES_FIELD = "properties";
export const SLOT_FIELD = "slot";

const TWO_HANDS = "twohand";
const ONE_HAND = "onehand";
/** The slots a weapon held in one hand has (the descriptor's "One hand", "One hand or the off hand", or none yet). */
const ONE_HAND_SLOTS: readonly string[] = ["", ONE_HAND, "onehand,secondary"];

/** The Two-Handed property, by name or id: the SRD's (`..._TWOHANDED`, 2024 `..._TWO_HANDED`) or an imported one. */
const isTwoHanded = (r: HomebrewRef): boolean =>
  r.name.replace(/[\s-]/g, "").toLowerCase() === "twohanded" || /TWO_?HANDED$/.test(r.id);

/**
 * The slot a weapon is held on after its properties change (P3, HC11): the engine wields a weapon in two hands only
 * when its slot says so, so picking Two-Handed moves a weapon held in one hand to Two hands, and taking Two-Handed off
 * moves it back to One hand. Any other slot, or a change that does not add or remove Two-Handed, is the author's.
 */
export function weaponSlotFor(before: readonly HomebrewRef[], after: readonly HomebrewRef[], slot: string): string {
  const was = before.some(isTwoHanded);
  const is = after.some(isTwoHanded);
  if (is && !was && ONE_HAND_SLOTS.includes(slot)) return TWO_HANDS;
  if (was && !is && slot === TWO_HANDS) return ONE_HAND;
  return slot;
}

const refs = (v: unknown): HomebrewRef[] => (Array.isArray(v) ? v.filter((r): r is HomebrewRef => typeof r?.id === "string" && typeof r?.name === "string") : []);

/** A weapon form's fields with `properties` set to `next`, and its slot following the Two-Handed property. */
export function withWeaponProperties(fields: Readonly<Record<string, unknown>>, next: unknown): Record<string, unknown> {
  const slot = typeof fields[SLOT_FIELD] === "string" ? (fields[SLOT_FIELD] as string) : "";
  const moved = weaponSlotFor(refs(fields[PROPERTIES_FIELD]), refs(next), slot);
  return { ...fields, [PROPERTIES_FIELD]: next, ...(moved !== slot ? { [SLOT_FIELD]: moved } : {}) };
}
