import type { CharacterSnapshot, PlayEquipmentPackage } from "../../api/contract";

/** The option label that declines a package ("I'll buy my own"; engine `packages::BUY_OWN`). */
export const BUY_OWN = "none";

/**
 * Starting-equipment packages still open: no option taken and not declined. Tester B3 (B-129, TV2): on a phone each counts
 * as an open choice (the Gear badge, Next selection) until it is taken or dismissed with "I'll buy my own".
 */
export function openPackages(snapshot: CharacterSnapshot): PlayEquipmentPackage[] {
  return snapshot.play.equipmentPackages.filter((p) => !p.chosen && !p.declined);
}
