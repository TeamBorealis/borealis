import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { CharacterSnapshot } from "../api/contract";
import type { ShownRow, VersionLike } from "../lib/versions";
import { EMPTY_FILTER, narrowRows } from "../pages/spells/SpellFilter";
import { renderApp } from "./utils";

/**
 * GitHub #21 (B-158): the SPELLCASTING filter bar acts on the list of the class whose spells are being chosen (the
 * highlighted slot's catalogue), as Aurora's filter acts on the selected expander. Before the fix it skipped that list:
 * a single-class caster's filters did nothing, and a multiclass caster's narrowed the other class's grid.
 */

async function freshWizard(api: MockApi): Promise<CharacterSnapshot> {
  const created = await api.createCharacter({ request: { name: "Filters", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "manual", options: {} } });
  const classSlot = created.slots.find((s) => s.elementType === "class")!;
  return api.applySelection({ characterId: created.id, slotId: classSlot.id, elementId: "wizard" });
}

/** Wizard 1 / Cleric 1: the Cleric already knows four spells (Bless, Cure Wounds, Detect Magic, Healing Word). */
async function wizardCleric(api: MockApi): Promise<CharacterSnapshot> {
  const wizard = await freshWizard(api);
  const added = await api.addMulticlass({ characterId: wizard.id });
  const slot = added.slots.find((s) => s.elementType === "multiclass" && !s.chosen)!;
  return api.applySelection({ characterId: wizard.id, slotId: slot.id, elementId: "multi-cleric" });
}

const slotTitled = (snap: CharacterSnapshot, title: string) => snap.slots.find((s) => s.tab === "spells" && s.title === title && !s.chosen)!;
const names = (table: HTMLElement) => [...table.querySelectorAll("tr[data-spell-id] .sp-name .ap-name")].map((b) => b.textContent);
const catalogue = async (slotId: string, name: string) => {
  const table = await screen.findByRole("table", { name });
  expect(table.getAttribute("data-slot-id")).toBe(slotId);
  await waitFor(() => expect(names(table).length).toBeGreaterThan(0));
  return table;
};

describe("Spell filters on the SPELLCASTING tab (GitHub #21)", () => {
  it("narrow a single-class caster's list of spells to choose from: name, school, level, and the badge counts it", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await freshWizard(api);
    const slot = slotTitled(snap, "Spellbook spell");
    renderApp(`/spells?slot=${encodeURIComponent(slot.id)}`, { api, openCharacterId: snap.id });
    const table = await catalogue(slot.id, "Wizard spells");
    const all = names(table);
    expect(all).toContain("Magic Missile");
    expect(all.length).toBeGreaterThan(3);
    const badge = screen.getByLabelText("Filtered spells");
    expect(badge.textContent).toBe(`${all.length} / ${all.length}`);

    await user.type(screen.getByRole("textbox", { name: "Filter spells" }), "magic missile");
    expect(names(table)).toEqual(["Magic Missile"]);
    expect(badge.textContent).toBe(`1 / ${all.length}`);
    await user.click(screen.getByRole("button", { name: "Clear filter text" }));
    expect(names(table)).toEqual(all);

    // The school list comes from the spells on offer (a fresh caster knows none yet).
    await user.selectOptions(screen.getByLabelText("School filter"), "Abjuration");
    const abjuration = names(table);
    expect(abjuration.length).toBeGreaterThan(0);
    expect(abjuration.length).toBeLessThan(all.length);
    for (const row of table.querySelectorAll("tr[data-spell-id] .sp-lvl")) expect(row.textContent).toContain("Abjuration");
    await user.selectOptions(screen.getByLabelText("School filter"), "");

    await user.click(within(screen.getByRole("group", { name: "Spell levels" })).getByRole("button", { name: "2" }));
    expect(names(table)).toEqual([]);
    expect(within(table).getByText(/Nothing matches/)).toBeTruthy();
  });

  it("narrow a cantrip slot's list by the level toggles", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await freshWizard(api);
    const slot = slotTitled(snap, "Wizard cantrip");
    renderApp(`/spells?slot=${encodeURIComponent(slot.id)}`, { api, openCharacterId: snap.id });
    const table = await catalogue(slot.id, "Wizard spells");
    const all = names(table);
    const levels = within(screen.getByRole("group", { name: "Spell levels" }));
    await user.click(levels.getByRole("button", { name: "1" }));
    expect(names(table)).toEqual([]);
    await user.click(levels.getByRole("button", { name: "0" }));
    expect(names(table)).toEqual(all);
  });

  it("act on the class whose spells are being chosen, not the other class of a multiclass caster", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await wizardCleric(api);
    const cleric = snap.spellcasting.classes.find((c) => c.className === "Cleric")!;
    expect(cleric.spells.length).toBe(4);
    const slot = slotTitled(snap, "Spellbook spell");
    renderApp(`/spells?slot=${encodeURIComponent(slot.id)}`, { api, openCharacterId: snap.id });
    const wizardList = await catalogue(slot.id, "Wizard spells");
    const clericGrid = screen.getByRole("table", { name: "Cleric spells" });
    const clericAll = names(clericGrid);
    expect(clericAll).toHaveLength(4);

    await user.type(screen.getByRole("textbox", { name: "Filter spells" }), "shield");
    expect(names(wizardList)).toEqual(["Shield"]);
    expect(names(clericGrid)).toEqual(clericAll);
  });

  it("act on the Cleric's list while a Cleric slot is highlighted, and leave the Wizard's alone", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await wizardCleric(api);
    const slot = slotTitled(snap, "Cleric cantrip");
    renderApp(`/spells?slot=${encodeURIComponent(slot.id)}`, { api, openCharacterId: snap.id });
    const clericList = await catalogue(slot.id, "Cleric spells");
    expect(screen.queryAllByRole("table", { name: "Wizard spells" }).every((t) => t.getAttribute("data-slot-id") === null)).toBe(true);

    await user.type(screen.getByRole("textbox", { name: "Filter spells" }), "flame");
    expect(names(clericList)).toEqual(["Sacred Flame"]);
  });

  it("still narrow the class grids while no slot is highlighted", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await wizardCleric(api);
    renderApp("/spells", { api, openCharacterId: snap.id });
    const clericGrid = await screen.findByRole("table", { name: "Cleric spells" });
    await user.type(screen.getByRole("textbox", { name: "Filter spells" }), "bless");
    expect(names(clericGrid)).toEqual(["Bless"]);
    expect(screen.getByLabelText("Filtered spells").textContent).toBe("1 / 4");
  });
});

describe("narrowRows (GitHub #21)", () => {
  const v = (id: string, sourceName: string, level = 1): VersionLike => ({ id, name: "Shield", sourceName, level, school: "Abjuration", rulesets: ["dnd2024"] });
  const a = v("shield-a", "Book A");
  const b = v("shield-b", "Book B");
  const row = (chosen: VersionLike | null): ShownRow => ({ key: "shield", name: "Shield", all: [a, b], versions: [a, b], shown: chosen ?? a, chosen });

  it("keeps a row when another of its versions passes, and shows that version", () => {
    const [out] = narrowRows([row(null)], { ...EMPTY_FILTER, source: "Book B" }, "dnd2024");
    expect(out!.versions).toEqual([b]);
    expect(out!.shown).toBe(b);
  });

  it("drops a row no version of which passes, but always keeps the chosen row", () => {
    const f = { ...EMPTY_FILTER, levels: [3] };
    expect(narrowRows([row(null)], f, "dnd2024")).toEqual([]);
    expect(narrowRows([row(a)], f, "dnd2024").map((r) => r.shown)).toEqual([a]);
  });

  it("returns the rows as they are while nothing is filtered", () => {
    const rows = [row(null)];
    expect(narrowRows(rows, EMPTY_FILTER, "dnd2024")).toBe(rows);
  });
});

describe("Clear on an emptied slot list (GitHub #21)", () => {
  it("clears the filter bar too", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await freshWizard(api);
    const slot = slotTitled(snap, "Wizard cantrip");
    renderApp(`/spells?slot=${encodeURIComponent(slot.id)}`, { api, openCharacterId: snap.id });
    const table = await catalogue(slot.id, "Wizard spells");
    const all = names(table);
    const box = screen.getByRole("textbox", { name: "Filter spells" }) as HTMLInputElement;
    await user.type(box, "zzzz");
    expect(names(table)).toEqual([]);
    await user.click(within(table).getByRole("button", { name: "Clear" }));
    expect(box.value).toBe("");
    expect(names(table)).toEqual(all);
  });
});
