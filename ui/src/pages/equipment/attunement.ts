import type { Attack, CharacterSnapshot, InventoryItem } from "../../api/contract";

/** "Requires attunement by a cleric, druid, or warlock" (the engine sends the "by ..." part only). */
export function restrictionText(item: InventoryItem): string | null {
  const r = item.attunementRestriction?.trim();
  return r ? `Requires attunement ${r}` : null;
}

/** Attune is refused for a barred item that is not attuned yet (ending an attunement always works). */
export const attuneBlocked = (item: InventoryItem): boolean => !!item.attunementBarred && !item.attuned;

/** The engine's `attunement.restricted` warning text, for an attuned item the character may not attune. */
const restrictedMessage = (item: InventoryItem): string =>
  `${item.name} is attuned although it requires attunement ${item.attunementRestriction ?? "by someone else"}`;
const RESTRICTED_NOTICE = /\bis attuned although it requires attunement\b/;

/**
 * The barred-attunement warnings (`attunement.restricted`): attuned items the character does not
 * qualify for, from the inventory lines and from the engine notices, without duplicates.
 */
export function attunementWarnings(snapshot: Pick<CharacterSnapshot, "inventory" | "notices">): string[] {
  const fromItems = snapshot.inventory.items.filter((i) => i.attunementBarred && i.attuned).map(restrictedMessage);
  const fromNotices = snapshot.notices.filter((n) => RESTRICTED_NOTICE.test(n));
  return [...new Set([...fromNotices, ...fromItems])];
}

/** Weapons whose attack line fires this ammunition (`Attack.ammunition` names the inventory line). */
export function weaponsFiring(item: InventoryItem, attacks: Attack[]): string[] {
  const names = new Set([item.displayName, item.name]);
  return [...new Set(attacks.filter((a) => a.ammunition && names.has(a.ammunition)).map((a) => a.name))];
}
