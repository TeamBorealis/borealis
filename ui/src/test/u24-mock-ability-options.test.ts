import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";

// The mock host's `?mockAbilityOptions=1` scenario (web e2e of tester B5 on desktop, U24): a 2024 background offers its
// ability increases as the engine does, one list of options, and the chosen option raises the scores it names.
describe("mock 2024 background ability options (B5 scenario)", () => {
  it("offers one list named like the engine's options, and the chosen one raises its abilities", async () => {
    const api = new MockApi({ aiChunkDelayMs: 1, backgroundAbilityList: true });
    const created = await api.createCharacter({ request: { name: "Opt", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "manual", options: {} } });
    const bg = created.slots.find((s) => s.elementType === "background")!;
    const s1 = await api.applySelection({ characterId: created.id, slotId: bg.id, elementId: "acolyte" });
    const list = s1.slots.find((s) => s.title === "Ability Scores (Acolyte)")!;
    expect(list.elementType).toBe("list");
    expect(s1.slots.some((s) => s.title.startsWith("Background increase"))).toBe(false);
    const options = await api.getSlotOptions({ characterId: created.id, slotId: list.id });
    const names = options.items.map((o) => o.name);
    expect(names).toContain("Intelligence +1, Charisma +2");
    expect(names).toContain("Intelligence +1, Wisdom +1, Charisma +1");
    const pick = options.items.find((o) => o.name === "Intelligence +1, Charisma +2")!;
    const s2 = await api.applySelection({ characterId: created.id, slotId: list.id, elementId: pick.id });
    expect(s2.abilities.final.int - s1.abilities.final.int).toBe(1);
    expect(s2.abilities.final.cha - s1.abilities.final.cha).toBe(2);
    expect(s2.play.features.find((f) => f.ty === "Feat")).toMatchObject({ name: "Magic Initiate", grantedBy: "Acolyte" });
  });

  it("is off by default: the two increase picks stay", async () => {
    const api = new MockApi({ aiChunkDelayMs: 1 });
    const created = await api.createCharacter({ request: { name: "Def", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "manual", options: {} } });
    const bg = created.slots.find((s) => s.elementType === "background")!;
    const s1 = await api.applySelection({ characterId: created.id, slotId: bg.id, elementId: "acolyte" });
    expect(s1.slots.filter((s) => s.title.startsWith("Background increase"))).toHaveLength(2);
  });
});
