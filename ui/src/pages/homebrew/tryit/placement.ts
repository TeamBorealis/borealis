import type { CharacterSnapshot, HomebrewKindDescriptor, InventoryItem } from "../../../api/contract";

/** Where the draft sits on the test character (spec 9 "plus the draft in its slot"), read from the kind's element type. */
export type TrialSlot = "class" | "subclass" | "species" | "background" | "feat" | "spell" | "item" | "creature" | "grant";

const SLOT_BY_TYPE: Readonly<Record<string, TrialSlot>> = {
  Class: "class", Archetype: "subclass", Species: "species", Race: "species", "Sub Race": "species", "Race Variant": "species",
  Background: "background", Feat: "feat", Spell: "spell", Item: "item", Weapon: "item", Armor: "item", "Magic Item": "item",
  "Item Pack": "item", "Equipment Package": "item", Monster: "creature",
};

export function trialSlotOf(kind: Pick<HomebrewKindDescriptor, "elementTypes">): TrialSlot {
  return SLOT_BY_TYPE[kind.elementTypes[0] ?? ""] ?? "grant";
}

/** The line under "Your <kind>" in the setup: how the test character takes the draft. */
const PLACEMENT: Readonly<Record<TrialSlot, string>> = {
  class: "The test character takes it as its class.",
  subclass: "The test character takes its class, and this subclass at the subclass level.",
  species: "The test character takes it as its species.",
  background: "The test character takes it as its background.",
  feat: "The test character takes it at its first feat slot.",
  spell: "Granted through the class's spell list, or as a test spell when no list has it.",
  item: "Added to the inventory, and equipped when it is worn or held.",
  creature: "Shown as a stat block, and as the companion when the build has a companion slot.",
  grant: "Granted to the test character.",
};

/** How the test character carries an item draft, read from its inventory: never "attuned" for an item that needs no
 * attunement (a mundane weapon). */
export function itemPlacement(item: Pick<InventoryItem, "equipped" | "attuned"> | undefined): string {
  if (!item) return PLACEMENT.item;
  if (!item.equipped) return "Added to the inventory.";
  return item.attuned ? "Added to the inventory, equipped and attuned." : "Added to the inventory and equipped.";
}

export const placementText = (slot: TrialSlot, asSecondClass: boolean, item?: Pick<InventoryItem, "equipped" | "attuned">): string =>
  slot === "class" && asSecondClass ? "The test character starts as a Fighter, then takes levels in this class."
    : slot === "item" ? itemPlacement(item) : PLACEMENT[slot];

/** The draft item in the test character's inventory (the last of its name). */
export function draftItem(snapshot: CharacterSnapshot, name: string): InventoryItem | undefined {
  const wanted = name.trim().toLowerCase();
  return [...snapshot.inventory.items].reverse().find((i) => i.name.trim().toLowerCase() === wanted);
}

/** The level the draft shows up at, when it is a feature of the test character ("Taken at level 4"). */
export function takenAtLevel(snapshot: CharacterSnapshot, name: string): number | null {
  const wanted = name.trim().toLowerCase();
  if (!wanted) return null;
  return snapshot.play.features.find((f) => f.name.trim().toLowerCase() === wanted)?.level ?? null;
}
