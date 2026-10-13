import { describe, expect, it } from "vitest";
import { MockApi } from "../mock";
import { COMMANDS } from "../contract";

describe("MockApi 2024 play state and conversion", () => {
  const hero = async (api: MockApi) => (await api.listCharacters()).find((c) => c.name.startsWith("Brannoc"))!.id;

  it("registers every new command", () => {
    for (const name of ["previewConvertTo2024", "applyConvertTo2024", "spendResource", "restoreResource", "shortRest", "longRest", "setToggle", "setExhaustion", "setHeroicInspiration", "chooseWeaponMasteries", "setWildShapeForms", "chooseEquipmentPackage"] as const) {
      expect(COMMANDS[name]).toMatch(/^[a-z0-9_]+$/);
    }
  });

  it("toggling Rage flips the resistances and a long rest refills and lowers exhaustion", async () => {
    const api = new MockApi();
    const characterId = await hero(api);
    const off = await api.getSnapshot({ characterId });
    expect(off.play.defenses.resistances.every((r) => !r.active)).toBe(true);
    const on = await api.setToggle({ characterId, toggleId: "rage", on: true });
    expect(on.play.defenses.resistances.some((r) => r.name === "bludgeoning" && r.active)).toBe(true);
    const spent = await api.spendResource({ characterId, resourceId: "rage", uses: 2 });
    const rage = spent.play.resources.find((r) => r.id === "rage")!;
    expect(rage.current).toBe(rage.max - 2);
    await api.setExhaustion({ characterId, level: 3 });
    const rested = await api.longRest({ characterId });
    expect(rested.play.resources.find((r) => r.id === "rage")!.current).toBe(rage.max);
    expect(rested.play.toggles.every((t) => !t.on)).toBe(true);
    expect(rested.play.exhaustion.level).toBe(off.ruleset === "dnd2024" ? 2 : 3);
  });

  it("converts only 2014 characters, into a new one", async () => {
    const api = new MockApi();
    const characterId = await hero(api);
    const base = await api.getSnapshot({ characterId });
    if (base.ruleset === "dnd2014") {
      const preview = await api.previewConvertTo2024({ characterId });
      expect(preview.name).toMatch(/\(2024\)$/);
      const applied = await api.applyConvertTo2024({ characterId });
      expect(applied.newCharacterId).not.toBe(characterId);
      expect(applied.snapshot.ruleset).toBe("dnd2024");
      expect((await api.getSnapshot({ characterId })).ruleset).toBe("dnd2014");
      await expect(api.previewConvertTo2024({ characterId: applied.newCharacterId })).rejects.toMatchObject({ code: "requirement_failed" });
    }
  });

  it("limits weapon masteries", async () => {
    const api = new MockApi();
    const characterId = (await api.applyConvertTo2024({ characterId: await hero(api) })).newCharacterId;
    await expect(api.chooseWeaponMasteries({ characterId, weaponIds: ["a", "a"] })).rejects.toMatchObject({ code: "invalid_argument" });
    const s = await api.chooseWeaponMasteries({ characterId, weaponIds: ["greatsword"] });
    expect(s.play.masteries).toHaveLength(1);
    await expect(api.chooseWeaponMasteries({ characterId, weaponIds: ["a", "b", "c", "d"] })).rejects.toMatchObject({ code: "requirement_failed" });
  });
});

describe("MockApi Druid and Wild Shape", () => {
  it("a level 4 Druid gets Wild Shape options that respect the CR and flying limits", async () => {
    const api = new MockApi();
    const created = await api.createCharacter({ request: { name: "Moss", ruleset: "dnd2024", startingLevel: 4, gender: "", abilityMethod: "standard_array", options: {} } });
    const slot = created.slots.find((s) => s.elementType === "class")!;
    const druid = await api.applySelection({ characterId: created.id, slotId: slot.id, elementId: "druid" });
    const ws = druid.play.wildShape!;
    expect(ws.druidLevel).toBe(4);
    expect(ws.options.map((f) => f.id)).toEqual(["wolf", "riding-horse"]);
    const learned = await api.setWildShapeForms({ characterId: created.id, formIds: ["wolf"] });
    expect(learned.play.wildShape!.known.map((f) => f.id)).toEqual(["wolf"]);
    await expect(api.setWildShapeForms({ characterId: created.id, formIds: ["giant-eagle"] })).rejects.toMatchObject({ code: "requirement_failed" });
  });
});
