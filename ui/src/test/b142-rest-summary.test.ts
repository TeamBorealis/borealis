import { describe, expect, it } from "vitest";
import type { PlayResource, PlayState, PlayToggle } from "../api/contract";
import { restChanges, restSummary } from "../pages/play/restSummary";

const res = (id: string, name: string, current: number, max = 3): PlayResource => ({
  id, name, current, max, source: "Synth Class", recharge: ["long_rest"], shortRestRegain: null, longRestRegainAll: true,
});
const toggle = (id: string, on: boolean): PlayToggle => ({ id, name: "Synth Fury", on, active: on, activation: "", duration: "", blocks: [], effects: [], source: "Synth Class" });

type Rest = Parameters<typeof restChanges>[0];
const state = (over: Partial<PlayState>): Rest => ({
  resources: [], spellSlots: [], pactSlots: null, trackers: [], hitPoints: { max: 20, current: 20, temp: 0 }, hitDice: [],
  exhaustion: { linear: true, level: 0, maxLevels: 6, d20Penalty: 0, speedPenalty: 0, dead: false }, concentration: null, toggles: [],
  inspiration: false, ...over,
});

// B-142 (tester video C9): the rest toast names what came back or ended.
describe("rest summary", () => {
  it("names every kind of change a rest makes", () => {
    const before = state({
      resources: [res("a", "Synth Pool", 0), res("b", "Full Pool", 3)],
      spellSlots: [{ level: 1, max: 4, used: 3 }, { level: 2, max: 2, used: 1 }],
      pactSlots: { level: 3, max: 2, used: 2 },
      trackers: [{ id: "t1", name: "Luck tokens", max: 3, used: 2, reset: "long_rest", countsUp: false }, { id: "t2", name: "Doom marks", max: 5, used: 4, reset: "long_rest", countsUp: true }],
      hitPoints: { max: 20, current: 8, temp: 0 },
      hitDice: [{ classId: "c", className: "Synth", die: 8, total: 4, used: 3, notation: "1d8" }],
      exhaustion: { linear: true, level: 2, maxLevels: 6, d20Penalty: -4, speedPenalty: -10, dead: false },
      concentration: { id: "s", name: "Synth Ward", level: 1 },
      toggles: [toggle("fury", true)],
    });
    const after = state({
      resources: [res("a", "Synth Pool", 3), res("b", "Full Pool", 3)],
      spellSlots: [{ level: 1, max: 4, used: 0 }, { level: 2, max: 2, used: 0 }],
      pactSlots: { level: 3, max: 2, used: 0 },
      trackers: [{ id: "t1", name: "Luck tokens", max: 3, used: 0, reset: "long_rest", countsUp: false }, { id: "t2", name: "Doom marks", max: 5, used: 0, reset: "long_rest", countsUp: true }],
      hitDice: [{ classId: "c", className: "Synth", die: 8, total: 4, used: 1, notation: "1d8" }],
      exhaustion: { linear: true, level: 1, maxLevels: 6, d20Penalty: -2, speedPenalty: -5, dead: false },
      toggles: [toggle("fury", false)],
      inspiration: true,
    });
    expect(restChanges(before, after)).toEqual([
      "Synth Pool +3", "Spell slots +4", "Pact Magic slots +2", "Luck tokens +2", "Doom marks cleared", "Hit points +12", "Hit dice +2",
      "Exhaustion -1", "Heroic Inspiration", "Synth Fury ended", "Concentration ended",
    ]);
    expect(restSummary("Long Rest", before, after)).toBe("Long Rest: Synth Pool +3, Spell slots +4, Pact Magic slots +2, Luck tokens +2, Doom marks cleared, Hit points +12 and 5 more.");
  });

  it("says when there was nothing to regain", () => {
    const full = state({ resources: [res("a", "Synth Pool", 3)] });
    expect(restSummary("Short Rest", full, full)).toBe("Short Rest: nothing to regain.");
  });
});
