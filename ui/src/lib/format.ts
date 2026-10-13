import type { AbilityKey, Ruleset } from "../api/contract";

export const ABILITY_LABEL: Record<AbilityKey, string> = {
  str: "Strength", dex: "Dexterity", con: "Constitution", int: "Intelligence", wis: "Wisdom", cha: "Charisma",
};
export const rulesetLabel = (r: Ruleset): string => (r === "dnd2024" ? "2024" : "2014");
export const methodLabel = (m: string): string =>
  ({ point_buy: "Point buy", standard_array: "Standard array", roll_3d6: "Roll 3d6", roll_4d6: "Roll 4d6, drop lowest", manual: "Manual" })[m] ?? m;
export const formatGp = (cp: number): string => `${Math.round(cp) / 100} gp`;
const NAMES = ["Aldric Thorne", "Mirelle Ashdown", "Tobias Greywater", "Isolde Fenwick", "Garrick Stonehand", "Lyra Quickbrook", "Corwin Hale", "Sable Marrow"];
export const randomName = (): string => NAMES[Math.floor(Math.random() * NAMES.length)]!;
