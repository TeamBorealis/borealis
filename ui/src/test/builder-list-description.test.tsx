import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

const MISSING = "That element does not exist.";

/**
 * Regression: picking the 2024 Sage "Ability Scores" entry (a List rule) turned the description pane red,
 * because list entries carry list item ids, not element ids, and the pane looked them up with getElement.
 */
describe("Builder description pane and list-rule picks", () => {
  it("keeps describing the background after a list entry is clicked and chosen", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const created = await api.createCharacter({ request: { name: "Sage", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
    const bgSlot = created.slots.find((s) => s.elementType === "background")!;
    const snap = await api.applySelection({ characterId: created.id, slotId: bgSlot.id, elementId: "sage" });
    const listSlot = snap.slots.find((s) => s.tab === "background" && s.elementType === "list")!;
    const entry = (await api.getSlotOptions({ characterId: snap.id, slotId: listSlot.id, query: "" })).items[0]!;
    renderApp("/builder/background", { api, openCharacterId: snap.id });

    const pane = await screen.findByRole("complementary", { name: "Description" });
    await within(pane).findByRole("heading", { name: "Sage" });
    const options = await screen.findByRole("list", { name: `${listSlot.title} options` });
    const row = within(options).getByRole("button", { name: new RegExp(`^${entry.name}`) });
    await user.click(row);
    expect(row.closest("li")!.className).toContain("hl");
    await user.dblClick(row);
    await waitFor(async () => expect((await api.getSnapshot({ characterId: snap.id })).slots.find((s) => s.id === listSlot.id)!.chosen?.id).toBe(entry.id));

    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByText(MISSING)).toBeNull();
    expect(within(screen.getByRole("complementary", { name: "Description" })).getByRole("heading", { name: "Sage" })).toBeTruthy();
  });
});
