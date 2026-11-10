import type { PlayState, SpellEntry } from "../../api/contract";

/**
 * One way to cast a leveled spell: a slot of a level (or a Pact Magic slot), as a ritual with no slot, or with no slot
 * through the feature that grants it (`noSlot`: 2024 Pact of the Chain's Find Familiar).
 */
export type CastChoice =
  | { kind: "slot"; level: number; pact: boolean; left: number }
  | { kind: "ritual" }
  | { kind: "free" };

/** The slots a spell of `level` can use now: every pool of that level or higher with a slot left (GitHub #14). */
export function slotChoices(level: number, play: PlayState): CastChoice[] {
  const normal = play.spellSlots
    .filter((s) => s.level >= level && s.used < s.max)
    .map((s): CastChoice => ({ kind: "slot", level: s.level, pact: false, left: s.max - s.used }));
  const p = play.pactSlots;
  const pact = p && p.level >= level && p.used < p.max ? [{ kind: "slot", level: p.level, pact: true, left: p.max - p.used } as CastChoice] : [];
  return [...normal, ...pact];
}

/** Every way to cast the spell: without a slot when its feature allows, its slots, then the ritual when it has the ritual tag. */
export function castChoices(spell: SpellEntry, play: PlayState): CastChoice[] {
  return [
    ...(spell.noSlot ? [{ kind: "free" } as CastChoice] : []),
    ...slotChoices(spell.level, play),
    ...(spell.ritual ? [{ kind: "ritual" } as CastChoice] : []),
  ];
}

/**
 * What a tap on Cast spends: a slot of the spell's own level when one is left, else the only way there is; null asks
 * (the menu opens), so a higher slot is never spent without the user choosing it.
 */
export function defaultChoice(spell: SpellEntry, choices: CastChoice[]): CastChoice | null {
  // a feature that casts the spell without a slot spends nothing, so a tap never takes a slot it does not need
  const free = choices.find((c) => c.kind === "free");
  if (free) return free;
  // the ordinary pools come first, so a Pact Magic slot is the default only when no ordinary slot of that level is left
  const own = choices.find((c) => c.kind === "slot" && c.level === spell.level);
  if (own) return own;
  return choices.length === 1 && choices[0]!.kind === "slot" ? choices[0]! : null;
}

export function choiceLabel(c: CastChoice): string {
  if (c.kind === "free") return "Without a slot";
  if (c.kind === "ritual") return "As a ritual (no slot)";
  return c.pact ? `Pact Magic slot, level ${c.level} (${c.left} left)` : `Level ${c.level} slot (${c.left} left)`;
}
