import type { EquipRequest, InventoryItem } from "../../api/contract";
import type { PressMenuItem } from "../play/rolls/PressMenu";
import { attuneBlocked } from "./attunement";

/** What the inventory menu entries do (each closes the menu first). */
export interface ItemMenuActions {
  details: () => void;
  extract: () => void;
  addToAttacks: () => void;
  equip: (slot: EquipRequest) => void;
  unequip: () => void;
  toggleAttunement: () => void;
  remove: (all: boolean) => void;
}

const isVersatile = (item: InventoryItem): boolean => /versatile/i.test(item.weapon?.properties ?? "");

/**
 * Aurora's inventory context menu (InventorySection.xaml), in its order and words, with the entries that do not
 * apply to `item` left out: Edit Item Details, Extract Item, Add to Attacks & Spellcasting | Equip, Equip in
 * Primary Hand, Equip in Secondary Hand, Equip as Versatile | Toggle Attunement | Remove Item, Remove Items (All
 * Selected Stackables). An equipped item offers Unequip where Aurora's Equip would stand.
 */
export function itemMenuItems(item: InventoryItem, act: ItemMenuActions): PressMenuItem[] {
  const out: PressMenuItem[] = [{ label: "Edit Item Details", run: act.details }];
  if (item.isPack) out.push({ label: "Extract Item", run: act.extract });
  if (item.isWeapon) out.push({ label: "Add to Attacks & Spellcasting", run: act.addToAttacks });
  const equip: PressMenuItem[] = [];
  if (item.equipped) equip.push({ label: "Unequip", run: act.unequip });
  else if (item.equippable) {
    equip.push({ label: "Equip", run: () => act.equip("auto") });
    if (item.isWeapon) {
      equip.push({ label: "Equip in Primary Hand", run: () => act.equip("primary") });
      equip.push({ label: "Equip in Secondary Hand", run: () => act.equip("secondary") });
      if (isVersatile(item)) equip.push({ label: "Equip as Versatile", run: () => act.equip("versatile") });
    }
  }
  const attune: PressMenuItem[] = item.requiresAttunement
    ? [{ label: "Toggle Attunement", run: act.toggleAttunement, disabled: !item.attuned && attuneBlocked(item) }]
    : [];
  const remove: PressMenuItem[] = [{ label: "Remove Item", run: () => act.remove(false) }];
  if (item.quantity > 1) remove.push({ label: "Remove Items (All Selected Stackables)", run: () => act.remove(true) });
  for (const group of [equip, attune, remove]) {
    group.forEach((entry, i) => out.push(i === 0 ? { ...entry, separator: true } : entry));
  }
  return out;
}
