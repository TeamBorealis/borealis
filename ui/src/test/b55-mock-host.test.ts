import { describe, expect, it } from "vitest";
import type { SelectionSlot, SlotOption } from "../api/contract";
import { MockApi } from "../api/mock";
import { groupKey, normalizeName } from "../lib/versions";

/** B-55 (specs/version-rows-and-filters.md sec. 1.1, 1.2, 3, 4): the mock host groups same-name options and switches versions. */
async function character(ruleset: "dnd2014" | "dnd2024") {
  const api = new MockApi({ aiChunkDelayMs: 1 });
  const snap = await api.createCharacter({ request: { name: "B55", ruleset, startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
  const race = snap.slots.find((s) => s.elementType === "race")!;
  return { api, id: snap.id, race };
}

const rowNamed = (items: SlotOption[], name: string) => items.filter((o) => o.name === name);
const slotTitled = (slots: SelectionSlot[], title: string) => slots.find((s) => s.title === title);

describe("name normalization (sec. 1.1)", () => {
  it("folds whitespace, case and typographic quotes, nothing else", () => {
    expect(normalizeName("  Half-Elf ")).toBe("half-elf");
    expect(normalizeName("Half  Elf")).toBe("half elf");
    expect(normalizeName("Smith’s Tools")).toBe(normalizeName("smith's tools"));
    expect(normalizeName("“Quoted”")).toBe('"quoted"');
    expect(normalizeName("Half‐Elf")).not.toBe(normalizeName("Half-Elf"));
    expect(normalizeName("Elf (Legacy)")).toBe("elf (legacy)");
  });

  it("race and subrace are separate families; race and species are one", () => {
    expect(groupKey({ name: "Elf", type: "race" })).toBe(groupKey({ name: "elf", type: "species" as never }));
    expect(groupKey({ name: "Elf", type: "race" })).not.toBe(groupKey({ name: "Elf", type: "subrace" }));
  });
});

describe("mock getSlotOptions grouping", () => {
  it("2024 character: Dragonborn is one row with three versions, imported first, the 2014 one marked", async () => {
    const { api, id, race } = await character("dnd2024");
    const page = await api.getSlotOptions({ characterId: id, slotId: race.id });
    const rows = rowNamed(page.items, "Dragonborn");
    expect(rows).toHaveLength(1);
    const versions = rows[0]!.versions!;
    expect(versions.map((v) => v.sourceName)).toEqual(["Sample Expansion", "Sample Reference 5.2", "Sample Reference 5.1"]);
    expect(rows[0]!.id).toBe(versions[0]!.id);
    expect(versions[0]!.bundled).toBeUndefined();
    expect(versions[1]!.bundled).toBe(true);
    expect(versions[2]!.edition).toBe("dnd2014");
    // Rows with one version carry no `versions`; `total` counts rows.
    expect(rowNamed(page.items, "Human")[0]!.versions).toBeUndefined();
    expect(page.total).toBe(page.items.length);
    expect(new Set(page.items.map((o) => o.name)).size).toBe(page.items.length);
  });

  it("2014 character (strict): Dragonborn is one ordinary row", async () => {
    const { api, id, race } = await character("dnd2014");
    const rows = rowNamed((await api.getSlotOptions({ characterId: id, slotId: race.id })).items, "Dragonborn");
    expect(rows).toHaveLength(1);
    expect(rows[0]!.versions).toBeUndefined();
    expect(rows[0]!.rulesets).toEqual(["dnd2014"]);
  });

  it("the chosen version comes first in versions", async () => {
    const { api, id, race } = await character("dnd2024");
    await api.applySelection({ characterId: id, slotId: race.id, elementId: "dragonborn" });
    const row = rowNamed((await api.getSlotOptions({ characterId: id, slotId: race.id })).items, "Dragonborn")[0]!;
    expect(row.id).toBe("dragonborn");
    expect(row.versions![0]!.id).toBe("dragonborn");
  });

  it("query matches the row name only, not the short description", async () => {
    const { api, id, race } = await character("dnd2024");
    const byName = await api.getSlotOptions({ characterId: id, slotId: race.id, query: "DRAGON" });
    expect(byName.items.map((o) => o.name)).toEqual(["Dragonborn"]);
    expect(byName.total).toBe(1);
    const byText = await api.getSlotOptions({ characterId: id, slotId: race.id, query: "blood of dragons" });
    expect(byText.items).toHaveLength(0);
  });

  it("pages over rows", async () => {
    const { api, id, race } = await character("dnd2024");
    const all = await api.getSlotOptions({ characterId: id, slotId: race.id });
    const first = await api.getSlotOptions({ characterId: id, slotId: race.id, limit: 2 });
    const rest = await api.getSlotOptions({ characterId: id, slotId: race.id, offset: 2 });
    expect(first.total).toBe(all.total);
    expect([...first.items, ...rest.items].map((o) => o.name)).toEqual(all.items.map((o) => o.name));
  });
});

describe("mock switchVersion (sec. 3)", () => {
  async function withDragonborn(versionId: string, ancestry: string) {
    const env = await character("dnd2024");
    const snap = await env.api.applySelection({ characterId: env.id, slotId: env.race.id, elementId: versionId });
    const sub = slotTitled(snap.slots, "Draconic Ancestry")!;
    await env.api.applySelection({ characterId: env.id, slotId: sub.id, elementId: ancestry });
    return env;
  }

  it("keeps a sub-choice the new version offers under the same name", async () => {
    const { api, id, race } = await withDragonborn("dragonborn", "ancestry-red");
    const res = await api.switchVersion({ characterId: id, slotId: race.id, elementId: "dragonborn-2024" });
    expect(res.dropped).toEqual([]);
    expect(res.snapshot.slots.find((s) => s.id === race.id)!.chosen!.id).toBe("dragonborn-2024");
    const sub = slotTitled(res.snapshot.slots, "Draconic Ancestry")!;
    expect(sub.chosen?.id).toBe("ancestry-red-2024");
    expect(sub.chosen?.name).toBe("Red Ancestry");
  });

  it("keeps the same element where the new version offers it", async () => {
    const { api, id, race } = await withDragonborn("dragonborn", "ancestry-blue");
    const res = await api.switchVersion({ characterId: id, slotId: race.id, elementId: "dragonborn-expansion" });
    expect(res.dropped).toEqual([]);
    expect(slotTitled(res.snapshot.slots, "Draconic Ancestry")!.chosen?.id).toBe("ancestry-blue");
  });

  it("drops and reports a sub-choice that does not fit", async () => {
    const { api, id, race } = await withDragonborn("dragonborn", "ancestry-blue");
    const res = await api.switchVersion({ characterId: id, slotId: race.id, elementId: "dragonborn-2024" });
    expect(res.dropped).toEqual(["Draconic Ancestry"]);
    expect(slotTitled(res.snapshot.slots, "Draconic Ancestry")!.chosen).toBeNull();
  });

  it("refuses an element that is not a version of the pick and changes nothing", async () => {
    const { api, id, race } = await withDragonborn("dragonborn", "ancestry-red");
    await expect(api.switchVersion({ characterId: id, slotId: race.id, elementId: "human" })).rejects.toMatchObject({ code: "requirement_failed" });
    const snap = await api.getSnapshot({ characterId: id });
    expect(snap.slots.find((s) => s.id === race.id)!.chosen!.id).toBe("dragonborn");
    expect(slotTitled(snap.slots, "Draconic Ancestry")!.chosen?.id).toBe("ancestry-red");
  });

  it("plain applySelection to another version still drops the dependent pick", async () => {
    const { api, id, race } = await withDragonborn("dragonborn", "ancestry-blue");
    const snap = await api.applySelection({ characterId: id, slotId: race.id, elementId: "dragonborn-2024" });
    expect(slotTitled(snap.slots, "Draconic Ancestry")!.chosen).toBeNull();
  });
});
