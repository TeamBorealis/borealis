import type { MockElement } from "./data-types";

/** A stat block feature: the name with its usage notes as printed, then the text. */
type Feature = readonly [name: string, text: string];

interface MonsterDef {
  id: string;
  name: string;
  short: string;
  ac: string;
  hp: string;
  challenge: string;
  traits: Feature[];
  actions: Feature[];
  reactions?: Feature[];
  legendary?: { preamble: string; actions: Feature[] };
}

/** Two SRD 5.1 monsters (CC-BY-4.0, bundled with the app) so the compendium's Monster entry has something to show. */
const MONSTERS: MonsterDef[] = [
  {
    id: "ID_SRD51_MONSTER_GOBLIN", name: "Goblin", short: "Small humanoid (goblinoid), neutral evil.",
    ac: "15 (leather armor, shield)", hp: "7 (2d6)", challenge: "1/4 (50 XP)",
    traits: [["Nimble Escape", "The goblin can take the Disengage or Hide action as a bonus action on each of its turns."]],
    actions: [
      ["Scimitar", "Melee Weapon Attack: +4 to hit, reach 5 ft., one target. Hit: 5 (1d6 + 2) slashing damage."],
      ["Shortbow", "Ranged Weapon Attack: +4 to hit, range 80/320 ft., one target. Hit: 5 (1d6 + 2) piercing damage."],
    ],
  },
  {
    id: "ID_SRD51_MONSTER_UNICORN", name: "Unicorn", short: "Large celestial, lawful good.",
    ac: "12", hp: "67 (9d10+18)", challenge: "5 (1800 XP)",
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
const section = (title: string, list: Feature[] | undefined): string => (list?.length ? `<h4>${title}</h4>${list.map(feature).join("")}` : "");

/** The same description HTML the backend builds for a monster: its sections in stat block order. */
function statBlock(m: MonsterDef): string {
  const legendary = m.legendary ? `<h4>Legendary actions</h4><p>${m.legendary.preamble}</p>${m.legendary.actions.map(feature).join("")}` : "";
  return `${section("Traits", m.traits)}${section("Actions", m.actions)}${section("Reactions", m.reactions)}${legendary}`;
}

/** The mock's Monster elements, built with the content module's element factory. */
export function monsterElements(make: (id: string, name: string, short: string, extra: Partial<MockElement>) => MockElement): MockElement[] {
  return MONSTERS.map((m) => make(m.id, m.name, m.short, {
    rulesets: ["dnd2014"],
    description: statBlock(m),
    facts: [
      { label: "Type", value: "Monster" },
      { label: "Armor Class", value: m.ac },
      { label: "Hit Points", value: m.hp },
      { label: "Challenge", value: m.challenge },
    ],
  }));
}
