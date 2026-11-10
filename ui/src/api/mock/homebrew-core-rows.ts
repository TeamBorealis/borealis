import type { HomebrewPickerRow } from "../types";

// The core elements item fields point at (spec 4.9): weapon categories, properties, groups, damage types and the 2024
// mastery properties, by their SRD names and the bundled ids. The mock's sample content has no elements of these
// types, so the reference picker offers these rows on the web mock (the app offers the loaded core pack's).

const row = (id: string, name: string, type: string, summary = "", edition: HomebrewPickerRow["edition"] = null): HomebrewPickerRow => ({
  id, name, type, summary, prerequisites: null, sourceName: "Core", edition, draft: false,
});

const named = (prefix: string, type: string, names: readonly string[]): HomebrewPickerRow[] =>
  names.map((n) => row(`${prefix}${n.toUpperCase().replace(/[^A-Z0-9]+/g, "")}`, n, type));

export const CORE_PICKER_ROWS: readonly HomebrewPickerRow[] = [
  ...["Simple Melee", "Simple Ranged", "Martial Melee", "Martial Ranged"].map((n) =>
    row(`ID_INTERNAL_WEAPON_CATEGORY_${n.toUpperCase().replace(" ", "_")}`, n, "Weapon Category")),
  ...named("ID_INTERNAL_WEAPON_PROPERTY_", "Weapon Property", ["Ammunition", "Finesse", "Heavy", "Light", "Loading", "Range", "Reach", "Thrown", "Two-Handed", "Versatile"]),
  ...named("ID_INTERNAL_DAMAGE_TYPE_", "Damage Type", ["Bludgeoning", "Piercing", "Slashing", "Acid", "Cold", "Fire", "Force", "Lightning", "Necrotic", "Poison", "Psychic", "Radiant", "Thunder"]),
  ...["Axes", "Bows", "Clubs", "Crossbows", "Hammers", "Maces", "Mauls", "Spears", "Swords"].map((n) =>
    row(`ID_INTERNAL_WEAPON_GROUP_${n.toUpperCase()}`, `Weapon Group (${n})`, "Weapon Group")),
  ...["Cleave", "Graze", "Nick", "Push", "Sap", "Slow", "Topple", "Vex"].map((n) =>
    row(`ID_2024_MASTERY_${n.toUpperCase()}`, n, "Mastery Property", "", "dnd2024")),
];
