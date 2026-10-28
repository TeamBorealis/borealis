import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { SlotOption } from "../api/contract";
import { buildRows } from "../lib/versions";
import { sourceLine } from "../pages/builder/AbilityTile";
import { renderApp } from "./utils";

async function fresh(api: MockApi) {
  return api.createCharacter({ request: { name: "Picker", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
}

describe("Pickers", () => {
  it("collapses the chosen picker and opens the next pending one after a double-click", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const snap = await fresh(api);
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    const list = await screen.findByRole("list", { name: "Class options" });
    await user.dblClick(await within(list).findByRole("button", { name: /Fighter/ }));
    await waitFor(() => expect(screen.queryByRole("list", { name: "Class options" })).toBeNull());
    // the chosen class is a footer line with a clear button; the new pick opened as an expanded list
    expect(screen.getAllByRole("button", { name: "Clear Fighter" }).length).toBeGreaterThan(0);
    const style = await screen.findByRole("list", { name: /Fighting style options/i });
    expect(within(style).getAllByRole("button").length).toBeGreaterThan(0);
  });

  it("chooses with Enter and never renders a Select button or in-row check/clear", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const snap = await fresh(api);
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    const list = await screen.findByRole("list", { name: "Class options" });
    const row = await within(list).findByRole("button", { name: /Wizard/ });
    await user.click(row);
    expect(screen.queryByRole("button", { name: "Select" })).toBeNull();
    row.focus();
    await user.keyboard("{Enter}");
    await waitFor(async () => expect((await api.getSnapshot({ characterId: snap.id })).slots.some((s) => s.chosen?.id === "wizard")).toBe(true));
  });

  it("has no Levels & HP tab or multiclass link, and /builder/levels opens the Level Up flyout", async () => {
    const api = new MockApi();
    const snap = await fresh(api);
    renderApp("/builder/levels", { api, openCharacterId: snap.id });
    expect(await screen.findByRole("dialog", { name: /Level up/i })).toBeTruthy();
    expect(screen.queryByRole("tab", { name: /Levels/i })).toBeNull();
    expect(screen.queryByText(/add a multiclass/i)).toBeNull();
  });

  it("lists the class description column and keeps the tab strip free of the old badges", async () => {
    const api = new MockApi();
    const snap = await fresh(api);
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    const list = await screen.findByRole("list", { name: "Class options" });
    expect(await within(list).findByText(/master of martial combat/i)).toBeTruthy();
  });
});

describe("Picker helpers", () => {
  it("formats ability bonus sources as '<source> (n)'", () => {
    expect(sourceLine("Mountain Dwarf +2")).toBe("Mountain Dwarf (2)");
    expect(sourceLine("Background -1")).toBe("Background (-1)");
    expect(sourceLine("Feat")).toBe("Feat");
  });

  it("keeps a chosen element at its alphabetical place instead of pinning it on top", () => {
    const opt = (id: string, name: string) => ({ id, name } as unknown as SlotOption);
    const rows = buildRows([opt("a", "Acrobatics"), opt("c", "Insight"), opt("d", "Survival")], { id: "b", name: "Athletics" });
    expect(rows.map((r) => r.name)).toEqual(["Acrobatics", "Athletics", "Insight", "Survival"]);
  });
});
