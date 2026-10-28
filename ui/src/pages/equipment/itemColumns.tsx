import type { ReactNode } from "react";
import type { ArmorFacts, Ruleset, WeaponFacts } from "../../api/contract";
import { editionMarks, EditionSuffix } from "../../components";

export interface ItemRow {
  id: string;
  name: string;
  weight: number;
  costCp: number;
  category: string;
  sourceName?: string;
  armor?: ArmorFacts | null;
  weapon?: WeaponFacts | null;
  /** B-55 W3: set only on a version written for the other edition than the character's. */
  edition?: Ruleset;
}

export type Align = "l" | "c" | "r";

export interface Column {
  key: string;
  label: string;
  align: Align;
  cell: (r: ItemRow) => ReactNode;
}

const CP_PER_SP = 10;
const CP_PER_GP = 100;

/** Price in the largest whole coin (classic builder: "5 gp", "5 sp", "1 cp"). */
export function priceText(cp: number): string {
  if (!cp) return "";
  if (cp % CP_PER_GP === 0) return `${cp / CP_PER_GP} gp`;
  if (cp % CP_PER_SP === 0) return `${cp / CP_PER_SP} sp`;
  return `${cp} cp`;
}
export const weightText = (w: number): string => (w ? `${w} lb.` : "");
/** "14 + Dex (max 2)" reads "14 + Dex modifier (max 2)" in the Armor grid. */
export const armorClassText = (a: ArmorFacts): string => a.acText.replace(/\bDex\b(?! modifier)/, "Dex modifier");

const price = (align: Align): Column => ({ key: "price", label: "Price", align, cell: (r) => priceText(r.costCp) });
const weight = (align: Align): Column => ({ key: "weight", label: "Weight", align, cell: (r) => weightText(r.weight) });
/** The source name, with " (2014)" / " (2024)" after it on other-edition rows (B-55 sec. 1.3). */
const sourceCell = (r: ItemRow): ReactNode =>
  r.sourceName ? <span className="eq-src">{r.sourceName}<EditionSuffix marks={editionMarks(r)} sourceName={r.sourceName} /></span> : "";
const source = (align: Align): Column => ({ key: "source", label: "Source", align, cell: sourceCell });

export const ARMOR_COLUMNS: Column[] = [
  price("c"), weight("c"),
  { key: "ac", label: "Armor class", align: "c", cell: (r) => (r.armor ? armorClassText(r.armor) : "") },
  { key: "str", label: "Strength", align: "c", cell: (r) => r.armor?.strength ?? "" },
  { key: "stealth", label: "Stealth", align: "c", cell: (r) => (r.armor?.stealthDisadvantage ? "Disadvantage" : "") },
  { key: "group", label: "Group", align: "c", cell: (r) => r.armor?.group ?? "" },
  source("l"),
];

export const WEAPON_COLUMNS: Column[] = [
  price("c"), weight("c"),
  { key: "damage", label: "Damage", align: "c", cell: (r) => (r.weapon ? `${r.weapon.damage} ${r.weapon.damageType}`.trim() : "") },
  { key: "props", label: "Properties", align: "l", cell: (r) => r.weapon?.properties.join(", ") ?? "" },
  { key: "mastery", label: "Mastery", align: "c", cell: (r) => r.weapon?.mastery ?? "" },
  { key: "group", label: "Group", align: "c", cell: (r) => (r.weapon ? `${r.weapon.proficiency} ${r.weapon.kind}`.trim() : "") },
  source("l"),
];

/** Packs, gear, tools, magic items: NAME, PRICE, WEIGHT, SOURCE. */
export const DEFAULT_COLUMNS: Column[] = [price("r"), weight("l"), source("r")];

export function columnsFor(category: string): Column[] {
  if (/armor/i.test(category)) return ARMOR_COLUMNS;
  if (/weapons/i.test(category)) return WEAPON_COLUMNS;
  return DEFAULT_COLUMNS;
}
