import { describe, expect, it } from "vitest";
import type { CharacterSnapshot } from "../api/contract";
import { describeAdopt, describeChange, getStatus, setStatus } from "./status";

const snap = (over: Partial<CharacterSnapshot> = {}): CharacterSnapshot => ({
  name: "Testy",
  level: { total: 1 },
  slots: [{ id: "s1", title: "Race", chosen: null, locked: false }],
  progress: { remaining: 1, nextSlotId: "s1" },
  inventory: { items: [] },
  ...over,
}) as unknown as CharacterSnapshot;

const item = (itemId: string, name: string, quantity: number) => ({ itemId, name, displayName: name, quantity });

describe("status messages", () => {
  it("prompts for the first selection on a fresh character and says Loaded for a built one", () => {
    expect(describeAdopt(snap())).toBe("Choose one from the 'Race' selection option.");
    const built = snap({ slots: [{ id: "s1", title: "Race", chosen: { id: "r", name: "Elf" }, locked: false }] as never, progress: { remaining: 0, nextSlotId: null } as never });
    expect(describeAdopt(built)).toBe("Loaded 'Testy'");
  });

  it("reports added inventory items and level changes", () => {
    const withDagger = snap({ inventory: { items: [item("dagger", "Dagger", 2)] } as never });
    expect(describeChange(snap(), withDagger)).toBe("You have added 2 x 'Dagger' to your inventory.");
    expect(describeChange(withDagger, snap())).toBe("You have removed 2 x 'Dagger' from your inventory.");
    expect(describeChange(snap(), snap({ level: { total: 2 } as never }))).toBe("Your character is now level 2.");
  });

  it("prompts for the next selection after a pick and returns null when nothing changed", () => {
    const picked = snap({ slots: [{ id: "s1", title: "Race", chosen: { id: "r", name: "Elf" }, locked: false }, { id: "s2", title: "Class", chosen: null, locked: false }] as never, progress: { remaining: 1, nextSlotId: "s2" } as never });
    expect(describeChange(snap(), picked)).toBe("Choose one from the 'Class' selection option.");
    expect(describeChange(snap(), snap())).toBeNull();
  });

  it("stores the latest message", () => {
    setStatus("hello");
    expect(getStatus()).toBe("hello");
  });
});
