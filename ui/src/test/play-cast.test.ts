import { describe, expect, it } from "vitest";
import type { PlayState, SpellEntry } from "../api/contract";
import { castChoices, choiceLabel, defaultChoice } from "../pages/play/cast";

// GitHub #14: which slot a tap on Cast spends. The spell's own level when a slot of it is left, Pact Magic when that is
// the only way, otherwise the menu asks; a higher slot is never spent without a choice.

const spell = (level: number, ritual = false): SpellEntry => ({
  id: "s", name: "Spell", level, school: "Evocation", prepared: true, alwaysPrepared: false, origin: "class", slotId: null, canReplace: false,
  concentration: false, ritual, casting: null, sourceName: "", ready: true,
});
const play = (slots: [number, number, number][], pact: [number, number, number] | null = null) => ({
  spellSlots: slots.map(([level, max, used]) => ({ level, max, used })),
  pactSlots: pact ? { level: pact[0], max: pact[1], used: pact[2] } : null,
}) as unknown as PlayState;

describe("cast choices", () => {
  it("offers every pool at or above the spell's level that has a slot left, then the ritual", () => {
    const choices = castChoices(spell(1, true), play([[1, 4, 4], [2, 3, 1], [3, 2, 0]], [2, 2, 1]));
    expect(choices.map(choiceLabel)).toEqual([
      "Level 2 slot (2 left)", "Level 3 slot (2 left)", "Pact Magic slot, level 2 (1 left)", "As a ritual (no slot)",
    ]);
  });

  it("a tap spends a slot of the spell's own level, ordinary before Pact Magic", () => {
    const p = play([[1, 4, 0], [2, 3, 0]], [1, 1, 0]);
    expect(defaultChoice(spell(1), castChoices(spell(1), p))).toEqual({ kind: "slot", level: 1, pact: false, left: 4 });
    const pactOnly = play([[1, 4, 4]], [1, 1, 0]);
    expect(defaultChoice(spell(1), castChoices(spell(1), pactOnly))).toEqual({ kind: "slot", level: 1, pact: true, left: 1 });
  });

  it("a warlock's only way is the pact slot, at its level", () => {
    const p = play([], [3, 2, 0]);
    expect(defaultChoice(spell(1), castChoices(spell(1), p))).toEqual({ kind: "slot", level: 3, pact: true, left: 2 });
  });

  it("asks when the own level is spent and several higher slots or a ritual remain", () => {
    expect(defaultChoice(spell(1), castChoices(spell(1), play([[1, 2, 2], [2, 3, 0], [3, 1, 0]])))).toBeNull();
    expect(defaultChoice(spell(1, true), castChoices(spell(1, true), play([[1, 2, 2]])))).toBeNull();
    expect(castChoices(spell(1), play([[1, 2, 2]]))).toEqual([]);
  });

  it("a spell its feature casts without a slot offers that first and a tap spends nothing (2024 Pact of the Chain)", () => {
    const familiar = { ...spell(1, true), name: "Find Familiar", noSlot: true };
    const p = play([], [1, 1, 0]);
    const choices = castChoices(familiar, p);
    expect(choices.map(choiceLabel)).toEqual(["Without a slot", "Pact Magic slot, level 1 (1 left)", "As a ritual (no slot)"]);
    expect(defaultChoice(familiar, choices)).toEqual({ kind: "free" });
    // with every slot spent it can still be cast
    expect(castChoices(familiar, play([], [1, 1, 1])).map(choiceLabel)).toEqual(["Without a slot", "As a ritual (no slot)"]);
  });
});
