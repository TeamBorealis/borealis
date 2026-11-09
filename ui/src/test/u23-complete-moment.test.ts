import { describe, expect, it } from "vitest";
import type { CharacterSnapshot } from "../api/contract";
import { NO_MOMENT, dismissMoment, stepCompleteMoment, type CompleteMoment } from "../components/shell/completeMoment";

// U23 (B-143): the "Character complete" line is a moment, not a fixture. It comes when the session's last open pick is
// made, and goes when View sheet or Play is tapped, when it is dismissed, or when the user leaves the creation tabs.

/** A snapshot with `open` open selections and, when `pkg`, an untaken starting-equipment package. */
function snap(open: number, { id = "c1", pkg = false }: { id?: string; pkg?: boolean } = {}): CharacterSnapshot {
  return {
    id,
    progress: { remaining: open, nextSlotId: open > 0 ? "slot-1" : null },
    play: { equipmentPackages: pkg ? [{ package: "p1", chosen: null, declined: false }] : [] },
  } as unknown as CharacterSnapshot;
}

/** Runs the snapshots and routes through the moment in order, as the status bar sees them. */
function run(steps: Array<[CharacterSnapshot | null, string]>, start: CompleteMoment = NO_MOMENT): CompleteMoment {
  return steps.reduce((m, [s, path]) => stepCompleteMoment(m, s, path), start);
}

describe("creation's complete moment (U23)", () => {
  it("a character opened already complete gets no line", () => {
    expect(run([[null, "/characters"], [snap(0), "/builder"]]).shown).toBe(false);
    expect(run([[snap(0), "/builder/class"], [snap(0), "/spells"], [snap(0), "/equipment"]]).shown).toBe(false);
  });

  it("the last open pick made on Build, Magic or Gear brings the line", () => {
    for (const path of ["/builder/feats", "/spells", "/equipment"]) {
      expect(run([[snap(2), path], [snap(1), path], [snap(0), path]]).shown).toBe(true);
    }
  });

  it("it stays while the user moves between the creation tabs", () => {
    expect(run([[snap(1), "/builder"], [snap(0), "/builder"], [snap(0), "/spells"], [snap(0), "/equipment"]]).shown).toBe(true);
  });

  it("leaving the creation tabs (View sheet, Play or another tab) takes it away, and coming back does not bring it back", () => {
    const made = run([[snap(1), "/builder"], [snap(0), "/builder"]]);
    for (const away of ["/sheet", "/play", "/characters", "/more"]) {
      const left = stepCompleteMoment(made, snap(0), away);
      expect(left.shown).toBe(false);
      expect(stepCompleteMoment(left, snap(0), "/builder").shown).toBe(false);
    }
  });

  it("dismissed, it stays away", () => {
    const made = run([[snap(1), "/builder"], [snap(0), "/builder"]]);
    const gone = dismissMoment(made);
    expect(gone.shown).toBe(false);
    expect(run([[snap(0), "/builder"], [snap(0), "/spells"]], gone).shown).toBe(false);
  });

  it("an untaken starting-equipment package holds it back until it is taken or declined (B-129)", () => {
    const slotsDone = run([[snap(1, { pkg: true }), "/builder"], [snap(0, { pkg: true }), "/builder"]]);
    expect(slotsDone.shown).toBe(false);
    expect(stepCompleteMoment(slotsDone, snap(0), "/equipment").shown).toBe(true);
  });

  it("picks that open again (a level up) and are made bring it back", () => {
    const gone = dismissMoment(run([[snap(1), "/builder"], [snap(0), "/builder"]]));
    expect(run([[snap(2), "/builder"], [snap(0), "/builder"]], gone).shown).toBe(true);
  });

  it("another character starts over: one opened complete after an unfinished one gets no line", () => {
    expect(run([[snap(3, { id: "a" }), "/builder"], [snap(0, { id: "b" }), "/builder"]]).shown).toBe(false);
  });

  it("the last pick made away from the creation tabs brings no line there later", () => {
    expect(run([[snap(1), "/play"], [snap(0), "/play"], [snap(0), "/builder"]]).shown).toBe(false);
  });
});
