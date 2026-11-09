import { afterEach, describe, expect, it } from "vitest";
import type { CharacterSnapshot, SelectionSlot } from "../api/contract";
import { setPhoneForTests } from "../lib/phone";
import { describeAdopt, nextSelectionPrompt } from "../state/status";

// Tester B7 (B-133): "Choose one from the 'Magic Initiate Cantrips' selection option." was not read as a to-do. On a phone the
// prompt (TV2) uses plain words and names the tab of the next pick; desktop keeps Aurora's text (U7).
const slot = (over: Partial<SelectionSlot>): SelectionSlot =>
  ({ id: "s1", tab: "race", title: "Race", ordinal: 1, of: 1, chosen: null, locked: false, required: true, ...over }) as SelectionSlot;

const snap = (next: SelectionSlot, ruleset = "dnd2024"): CharacterSnapshot => ({
  name: "Testy", ruleset, slots: [next], progress: { remaining: 1, nextSlotId: next.id }, inventory: { items: [] },
}) as unknown as CharacterSnapshot;

afterEach(() => setPhoneForTests(false));

describe("phone prompt names the tab of the next slot (B7)", () => {
  it("an open spell pick names the Magic tab and its place among the slot's picks", () => {
    setPhoneForTests(true);
    const text = nextSelectionPrompt(snap(slot({ tab: "spells", title: "Magic Initiate Cantrips", ordinal: 1, of: 2 })));
    expect(text).toBe("Next: choose Magic Initiate Cantrips, 1 of 2 (Magic tab)");
    expect(text).not.toContain("selection option");
  });

  it("a Build pick names the Build tab and its section, in the character's own words", () => {
    setPhoneForTests(true);
    expect(nextSelectionPrompt(snap(slot({ tab: "languages", title: "Language" })))).toBe("Next: choose Language (Build tab, Languages)");
    expect(nextSelectionPrompt(snap(slot({ tab: "race", title: "Species" })))).toBe("Next: choose Species (Build tab, Species)");
    expect(nextSelectionPrompt(snap(slot({ tab: "race", title: "Race" }), "dnd2014"))).toBe("Next: choose Race (Build tab, Race)");
    expect(nextSelectionPrompt(snap(slot({ tab: "abilities", title: "Ability Score Improvement" })))).toBe("Next: choose Ability Score Improvement (Build tab, Ability Scores)");
  });

  it("a freshly opened character greets with the same plain prompt", () => {
    setPhoneForTests(true);
    expect(describeAdopt(snap(slot({ tab: "class", title: "Class" })))).toBe("Next: choose Class (Build tab, Class)");
  });

  it("desktop keeps Aurora's prompt word for word", () => {
    expect(nextSelectionPrompt(snap(slot({ tab: "spells", title: "Magic Initiate Cantrips", of: 2 })))).toBe("Choose one from the 'Magic Initiate Cantrips' selection option.");
  });
});
