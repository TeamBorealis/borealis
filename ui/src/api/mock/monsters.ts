import type { MockElement } from "./data-types";

/** A stat block feature: the name with its usage notes as printed, then the text. */
type Feature = readonly [name: string, text: string];

/** The stat block's header as the backend writes it (`compendium_monster.rs`): the type line, the lines above the
 * scores, the six scores, the lines below them. Each line is a label and its text; empty ones are left out. */
export interface StatBlockHeader {
  typeLine: string;
  top: readonly (readonly [string, string])[];
  scores: readonly number[];
  bottom: readonly (readonly [string, string])[];
}

const ABILITY_LABELS = ["STR", "DEX", "CON", "INT", "WIS", "CHA"] as const;
const escapeHtml = (t: string): string => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
/** `+2`, `-1`, `+0`. */
export const signed = (n: number): string => (n < 0 ? String(n) : `+${n}`);
export const modifierOf = (score: number): number => Math.floor((score - 10) / 2);
const line = ([label, value]: readonly [string, string]): string => (value.trim() ? `<p><i>${escapeHtml(label)}.</i> ${escapeHtml(value.trim())}</p>` : "");

/** The header's HTML, as `compendium_monster.rs::header_html` writes it. */
export function statBlockHeaderHtml(h: StatBlockHeader): string {
  const scores = `<table><thead><tr>${ABILITY_LABELS.map((a) => `<th>${a}</th>`).join("")}</tr></thead><tbody><tr>${
    h.scores.map((s) => `<td>${s} (${signed(modifierOf(s))})</td>`).join("")}</tr></tbody></table>`;
  return `<p><i>${escapeHtml(h.typeLine)}</i></p>${h.top.map(line).join("")}${scores}${h.bottom.map(line).join("")}`;
}

interface MonsterDef {
  id: string;
  name: string;
  short: string;
  header: StatBlockHeader;
  traits: Feature[];
  actions: Feature[];
  reactions?: Feature[];
  legendary?: { preamble: string; actions: Feature[] };
}

/** Two SRD 5.1 monsters (CC-BY-4.0, bundled with the app) so the compendium's Monster entry has something to show. */
const MONSTERS: MonsterDef[] = [
  {
    id: "ID_SRD51_MONSTER_GOBLIN", name: "Goblin", short: "Small humanoid (goblinoid), neutral evil.",
    header: {
      typeLine: "Small humanoid (goblinoid), neutral evil",
      top: [["Armor Class", "15 (leather armor, shield)"], ["Hit Points", "7 (2d6)"], ["Speed", "30 ft."]],
      scores: [8, 14, 10, 10, 8, 8],
      bottom: [["Skills", "Stealth +6"], ["Senses", "Darkvision 60 ft., passive Perception 9"], ["Languages", "Common, Goblin"], ["Challenge", "1/4 (50 XP)"]],
    },
    traits: [["Nimble Escape", "The goblin can take the Disengage or Hide action as a bonus action on each of its turns."]],
    actions: [
      ["Scimitar", "Melee Weapon Attack: +4 to hit, reach 5 ft., one target. Hit: 5 (1d6 + 2) slashing damage."],
      ["Shortbow", "Ranged Weapon Attack: +4 to hit, range 80/320 ft., one target. Hit: 5 (1d6 + 2) piercing damage."],
    ],
  },
  {
    id: "ID_SRD51_MONSTER_UNICORN", name: "Unicorn", short: "Large celestial, lawful good.",
    header: {
      typeLine: "Large celestial, lawful good",
      top: [["Armor Class", "12"], ["Hit Points", "67 (9d10+18)"], ["Speed", "50 ft."]],
      scores: [18, 14, 15, 11, 17, 16],
      bottom: [
        ["Damage Immunities", "Poison"], ["Condition Immunities", "Charmed, Paralyzed, Poisoned"],
        ["Senses", "Darkvision 60 ft., passive Perception 13"], ["Languages", "Celestial, Elvish, Sylvan, telepathy 60 ft."], ["Challenge", "5 (1800 XP)"],
      ],
    },
    traits: [
      ["Charge", "If the unicorn moves at least 20 feet straight toward a target and then hits it with a horn attack on the same turn, the target takes an extra 9 (2d8) piercing damage. If the target is a creature, it must succeed on a DC 15 Strength saving throw or be knocked prone."],
      ["Magic Resistance", "The unicorn has advantage on saving throws against spells and other magical effects."],
      ["Magic Weapons", "The unicorn's weapon attacks are magical."],
    ],
    actions: [
      ["Multiattack", "The unicorn makes two attacks: one with its hooves and one with its horn."],
      ["Hooves", "Melee Weapon Attack: +7 to hit, reach 5 ft., one target. Hit: 11 (2d6 + 4) bludgeoning damage."],
      ["Horn", "Melee Weapon Attack: +7 to hit, reach 5 ft., one target. Hit: 8 (1d8 + 4) piercing damage."],
      ["Healing Touch (3/Day)", "The unicorn touches another creature with its horn. The target magically regains 11 (2d8 + 2) hit points. In addition, the touch removes all diseases and neutralizes all poisons afflicting the target."],
    ],
    legendary: {
      preamble: "The unicorn can take 3 legendary actions, choosing from the options below. Only one legendary action option can be used at a time and only at the end of another creature's turn. The unicorn regains spent legendary actions at the start of its turn.",
      actions: [
        ["Hooves", "The unicorn makes one attack with its hooves."],
        ["Shimmering Shield (Costs 2 Actions)", "The unicorn creates a shimmering, magical field around itself or another creature it can see within 60 feet of it. The target gains a +2 bonus to AC until the end of the unicorn's next turn."],
        ["Heal Self (Costs 3 Actions)", "The unicorn magically regains 11 (2d8 + 2) hit points."],
      ],
    },
  },
];

const feature = ([name, text]: Feature): string => `<p><b><i>${name}.</i></b> ${text}</p>`;
/** One stat block section as the backend writes it (`compendium_monster.rs`): a heading, then each feature. */
export const section = (title: string, list: readonly Feature[] | undefined): string => (list?.length ? `<h4>${title}</h4>${list.map(feature).join("")}` : "");

/** The same description HTML the backend builds for a monster: its stat block's header, then its sections in stat block order. */
function statBlock(m: MonsterDef): string {
  const legendary = m.legendary ? `<h4>Legendary actions</h4><p>${m.legendary.preamble}</p>${m.legendary.actions.map(feature).join("")}` : "";
  return `${statBlockHeaderHtml(m.header)}${section("Traits", m.traits)}${section("Actions", m.actions)}${section("Reactions", m.reactions)}${legendary}`;
}

/** The mock's Monster elements, built with the content module's element factory. */
export function monsterElements(make: (id: string, name: string, short: string, extra: Partial<MockElement>) => MockElement): MockElement[] {
  return MONSTERS.map((m) => make(m.id, m.name, m.short, {
    rulesets: ["dnd2014"],
    description: statBlock(m),
    facts: [{ label: "Type", value: "Monster" }],
  }));
}
