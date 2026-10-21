import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { groupSlots } from "../pages/builder/PickerGroup";
import { renderApp } from "./utils";

describe("Locked slots (E31 SO-3)", () => {
  it("groupSlots drops locked slots, as Aurora does not show them", () => {
    const base = { id: "a", title: "A", groupLabel: null, locked: false };
    const groups = groupSlots([base, { ...base, id: "b", locked: true }] as never);
    expect(groups.map((g) => g.key)).toEqual(["a"]);
  });

  it("does not render a locked slot in the builder", async () => {
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const created = await api.createCharacter({ request: { name: "Locked", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
    const bgSlot = created.slots.find((s) => s.elementType === "background")!;
    const snap = await api.applySelection({ characterId: created.id, slotId: bgSlot.id, elementId: "sage" });
    const lockedSlot = snap.slots.find((s) => s.tab === "feats" && s.locked);
    expect(lockedSlot?.chosen).toBeTruthy();
    renderApp("/builder/feats", { api, openCharacterId: snap.id });
    await waitFor(() => expect(screen.getByText(/doesn't have any selections available/i)).toBeTruthy());
    expect(screen.queryByText("Origin feat")).toBeNull();
    expect(document.querySelector(`[data-slot-id="${lockedSlot!.id}"]`)).toBeNull();
  });
});
