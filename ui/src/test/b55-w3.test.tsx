import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ElementSummary } from "../api/contract";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

/** B-55 W3 (specs/version-rows-and-filters.md sec. 5): add-item list, Weapon Mastery, compendium mark, audit bugs. */

const EXPANSION = "sample-expansion";

async function openEquipment(prepare?: (api: MockApi, id: string) => Promise<unknown>) {
  const user = userEvent.setup();
  const api = new MockApi();
  const id = await seededId(api, "Seraphine");
  if (prepare) await prepare(api, id);
  renderApp("/equipment", { api, openCharacterId: id });
  return { user, api, id };
}

async function category(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(await screen.findByRole("button", { name: new RegExp(`^${name} \\(`) }));
  return screen.findByRole("table", { name: "Item catalog" });
}

const rowOf = (grid: HTMLElement, name: string) => within(grid).getByRole("button", { name }).closest("tr")!;
const sourceText = (row: HTMLElement) => row.querySelector(".eq-src")?.textContent;

describe("B-55 W3 equipment add-item list", () => {
  it("groups same-name items into one row; the shown version is the character's edition, imported first", async () => {
    const { user } = await openEquipment();
    const grid = await category(user, "Adventuring Gear");
    await within(grid).findByRole("button", { name: "Hooded Lantern" });
    expect(within(grid).getAllByRole("button", { name: "Hooded Lantern" })).toHaveLength(1);
    const row = rowOf(grid, "Hooded Lantern");
    expect(sourceText(row)).toBe("Sample Expansion");
    expect(within(row).getByRole("button", { name: "3 versions of Hooded Lantern" })).toBeTruthy();
    // Single-version rows get an empty version cell, so every source ends at the same x.
    expect(rowOf(grid, "Rope").querySelector(".eq-ver")).not.toBeNull();
    expect(rowOf(grid, "Rope").querySelector(".pick-ver-btn")).toBeNull();
  });

  it("the version menu changes the shown version, and Add adds that version", async () => {
    const { user, api, id } = await openEquipment();
    const grid = await category(user, "Adventuring Gear");
    await within(grid).findByRole("button", { name: "Hooded Lantern" });
    await user.click(within(rowOf(grid, "Hooded Lantern")).getByRole("button", { name: "3 versions of Hooded Lantern" }));
    const menu = await screen.findByRole("listbox", { name: "Versions of Hooded Lantern" });
    expect(within(menu).getAllByRole("group").map((g) => g.querySelector(".vm-head")!.textContent)).toEqual(["2024 rules (your character)", "2014 rules"]);
    await user.click(within(menu).getByRole("option", { name: "Sample Reference 5.1" }));
    await waitFor(() => expect(sourceText(rowOf(grid, "Hooded Lantern"))).toBe("Sample Reference 5.1 (2014)"));
    await user.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(async () => {
      const items = (await api.getSnapshot({ characterId: id })).inventory.items;
      expect(items.find((i) => i.itemId === "hooded-lantern")?.quantity).toBe(1);
    });
  });

  it("the Edition filter keeps versions of that edition and edition-neutral items", async () => {
    const { user } = await openEquipment();
    const grid = await category(user, "Adventuring Gear");
    await within(grid).findByRole("button", { name: "Hooded Lantern" });
    const edition = screen.getByRole("group", { name: "Edition" });
    await user.click(within(edition).getByRole("button", { name: "2014" }));
    const row = rowOf(grid, "Hooded Lantern");
    expect(sourceText(row)).toBe("Sample Reference 5.1 (2014)");
    expect(within(row).queryByRole("button", { name: /versions of/ })).toBeNull();
    expect(within(grid).getByRole("button", { name: "Rope" })).toBeTruthy();
    const weapons = await category(user, "Weapons");
    expect(await within(weapons).findByRole("button", { name: "Trident" })).toBeTruthy();
    await user.click(within(screen.getByRole("group", { name: "Edition" })).getByRole("button", { name: "2024" }));
    await waitFor(() => expect(within(weapons).queryByRole("button", { name: "Trident" })).toBeNull());
    expect(within(weapons).getByRole("button", { name: "Dagger" })).toBeTruthy();
    await user.click(within(screen.getByRole("group", { name: "Edition" })).getByRole("button", { name: "All" }));
    expect(await within(weapons).findByRole("button", { name: "Trident" })).toBeTruthy();
  });

  it("the Source checklist keeps several sources", async () => {
    const { user } = await openEquipment();
    const grid = await category(user, "Adventuring Gear");
    await within(grid).findByRole("button", { name: "Hooded Lantern" });
    await user.click(screen.getByRole("button", { name: "Source: All sources" }));
    const list = await screen.findByRole("dialog", { name: "Sources" });
    expect(within(list).getAllByRole("checkbox").map((c) => c.closest("label")!.querySelector(".vm-src")!.textContent))
      .toEqual(["Sample Expansion", "Sample Reference 5.1", "Sample Reference 5.2"]);
    await user.click(within(list).getByRole("checkbox", { name: /Sample Expansion/ }));
    expect(screen.getByRole("button", { name: "Source: 2 of 3 sources" })).toBeTruthy();
    const row = rowOf(grid, "Hooded Lantern");
    expect(sourceText(row)).toBe("Sample Reference 5.2");
    expect(within(row).getByRole("button", { name: "2 versions of Hooded Lantern" })).toBeTruthy();
    await user.click(within(list).getByRole("checkbox", { name: /Sample Reference 5\.1/ }));
    await waitFor(() => expect(within(grid).queryByRole("button", { name: "Rope" })).toBeNull());
    expect(within(rowOf(grid, "Hooded Lantern")).queryByRole("button", { name: /versions of/ })).toBeNull();
  });

  it("with strict mode on only the character's edition is listed", async () => {
    const { user } = await openEquipment((api, id) => api.setAllowCrossEdition({ characterId: id, allow: false }));
    const weapons = await category(user, "Weapons");
    await within(weapons).findByRole("button", { name: "Dagger" });
    expect(within(weapons).queryByRole("button", { name: "Trident" })).toBeNull();
    const gear = await category(user, "Adventuring Gear");
    const row = (await within(gear).findByRole("button", { name: "Hooded Lantern" })).closest("tr")!;
    expect(within(row).getByRole("button", { name: "2 versions of Hooded Lantern" })).toBeTruthy();
  });
});

describe("B-55 W3 mock host item scope (as the Rust host)", () => {
  it("honours the character's edition and source restrictions, marks bundled items and counts one row per name", async () => {
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    const gear = await api.searchItems({ category: "Adventuring Gear", limit: 1000, characterId: id });
    const lanterns = gear.items.filter((i) => i.name === "Hooded Lantern");
    expect(lanterns.map((i) => i.id).sort()).toEqual(["hooded-lantern", "hooded-lantern-2024", "hooded-lantern-expansion"]);
    expect(lanterns.find((i) => i.id === "hooded-lantern-2024")?.bundled).toBe(true);
    expect(lanterns.find((i) => i.id === "hooded-lantern-expansion")?.bundled).toBeUndefined();
    const count = (await api.listItemCategories({ characterId: id })).find((c) => c.name === "Adventuring Gear")!.count;
    expect(count).toBe(new Set(gear.items.map((i) => i.name)).size);
    const enabled = (await api.listSources()).map((s) => s.id).filter((s) => s !== EXPANSION);
    await api.setSourceRestrictions({ characterId: id, enabledSources: enabled });
    const kept = await api.searchItems({ category: "Weapons", limit: 1000, characterId: id });
    expect(kept.items.some((i) => i.source === EXPANSION)).toBe(false);
    const strict = await api.setAllowCrossEdition({ characterId: id, allow: false });
    const own = await api.searchItems({ category: "Weapons", limit: 1000, characterId: strict.id });
    expect(own.items.some((i) => i.id === "trident")).toBe(false);
  });
});

async function openMastery(prepare?: (api: MockApi, id: string) => Promise<unknown>) {
  const user = userEvent.setup();
  const api = new MockApi({ rng: () => 0.5 });
  const { newCharacterId } = await api.applyConvertTo2024({ characterId: await seededId(api, "Brannoc") });
  if (prepare) await prepare(api, newCharacterId);
  renderApp("/equipment", { api, openCharacterId: newCharacterId });
  await user.click(await screen.findByRole("tab", { name: "Additional treasure" }));
  return screen.findByRole("list", { name: "Weapons" });
}

describe("B-55 W3 Weapon Mastery list", () => {
  it("lists the character's edition only and shows each weapon's source", async () => {
    const list = await openMastery();
    const whip = (await within(list).findByRole("button", { name: "Master Whip" })).closest("li")!;
    expect(whip.querySelector(".mastery-src")?.textContent).toBe("Sample Expansion");
    expect(within(list).getByRole("button", { name: "Master Dagger" }).closest("li")!.querySelector(".mastery-src")?.textContent).toBe("Sample Reference 5.1");
    expect(within(list).queryByRole("button", { name: "Master Trident" })).toBeNull();
  });

  it("honours the character's source restrictions", async () => {
    const list = await openMastery(async (api, id) => {
      const enabled = (await api.listSources()).map((s) => s.id).filter((s) => s !== EXPANSION);
      return api.setSourceRestrictions({ characterId: id, enabledSources: enabled });
    });
    await within(list).findByRole("button", { name: "Master Dagger" });
    expect(within(list).queryByRole("button", { name: "Master Whip" })).toBeNull();
  });
});

describe("B-55 W3 compendium rows", () => {
  it("marks other-edition rows with ' (2014)' after the source, never on edition-neutral rows", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.type(await screen.findByLabelText("Compendium search"), "trident{Enter}");
    const dlg = await screen.findByRole("dialog", { name: "Compendium" });
    await waitFor(() => expect(dlg.querySelector(".v4-el .n")?.textContent).toBe("Trident"));
    const row = Array.from(dlg.querySelectorAll(".v4-el li")).find((li) => li.querySelector(".n")?.textContent === "Trident")!;
    expect(row.querySelector(".s")?.textContent).toBe("Sample Reference 5.1 (2014)");
    expect(row.querySelector(".s .ed-mark")?.getAttribute("title")).toBe("Written for the 2014 rules");
  });

  it("leaves edition-neutral rows unmarked", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.type(await screen.findByLabelText("Compendium search"), "dagger{Enter}");
    const dlg = await screen.findByRole("dialog", { name: "Compendium" });
    await waitFor(() => expect(Array.from(dlg.querySelectorAll(".v4-el .n")).some((n) => n.textContent === "Dagger")).toBe(true));
    const row = Array.from(dlg.querySelectorAll(".v4-el li")).find((li) => li.querySelector(".n")?.textContent === "Dagger")!;
    expect(row.querySelector(".ed-mark")).toBeNull();
  });
});

describe("B-55 W3 audit: source restriction confirm", () => {
  it("says the picks stay, and they do", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    const race = (await api.getSnapshot({ characterId: id })).slots.find((s) => s.elementType === "race")!.chosen!.id;
    renderApp("/content/sources", { api, openCharacterId: id });
    const grid = await screen.findByRole("grid", { name: "Sources" });
    await user.click(await within(grid).findByRole("checkbox", { name: "Allow Sample Reference 5.1" }));
    await user.click(screen.getByRole("button", { name: "Apply to character" }));
    const dlg = await screen.findByRole("dialog", { name: "Picks from these sources stay" });
    expect(screen.queryByText("Selections will be removed")).toBeNull();
    expect(within(dlg).getByText(/stay on the character/)).toBeTruthy();
    await user.click(within(dlg).getByRole("button", { name: "Apply" }));
    await waitFor(async () => expect((await api.loadCharacter({ characterId: id })).sourceRestrictions).not.toBeNull());
    expect((await api.getSnapshot({ characterId: id })).slots.find((s) => s.elementType === "race")!.chosen?.id).toBe(race);
  });
});

describe("B-55 W3 audit: multiclass entry of the right class", () => {
  const summary = (id: string, name: string, type: ElementSummary["type"], source: string, prerequisites: string | null = null): ElementSummary => ({
    id, name, type, source, sourceName: source, shortDescription: "", prerequisites, rulesets: ["dnd2024"],
  });

  it("takes each class's prerequisite from the multiclass entry of its own source, not the first of that name", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    const classes = [summary("fighter-a", "Fighter", "class", "book-a"), summary("fighter-b", "Fighter", "class", "book-b")];
    const multis = [summary("multi-b", "Fighter", "multiclass", "book-b", "Charisma 13"), summary("multi-a", "Fighter", "multiclass", "book-a", "Strength 13")];
    const original = api.searchCompendium.bind(api);
    vi.spyOn(api, "searchCompendium").mockImplementation(async (a) => {
      if (a.query.types?.[0] === "class") return { total: classes.length, items: classes };
      if (a.query.types?.[0] === "multiclass") return { total: multis.length, items: multis };
      return original(a);
    });
    renderApp("/sheet", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: "Level Up" }));
    const dlg = await screen.findByRole("dialog", { name: "Level up & multiclassing" });
    await waitFor(() => expect(within(dlg).getAllByLabelText("Fighter multiclass")).toHaveLength(2));
    const texts = within(dlg).getAllByLabelText("Fighter multiclass").map((r) => r.querySelector(".lvl-prereq")?.textContent);
    expect(texts).toEqual(["Prerequisites: Strength 13", "Prerequisites: Charisma 13"]);
  });
});
