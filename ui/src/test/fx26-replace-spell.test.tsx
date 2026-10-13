import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { getStatus } from "../state/status";
import { renderApp, seededId } from "./utils";

describe("Magic Replace spell and slot history", () => {
  it("shows Replace spell for a swappable filled slot, reopens it and records the replaced pick", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    const before = await api.getSnapshot({ characterId: id });
    const target = before.slots.find((s) => s.tab === "spells" && s.allowReplace && s.chosen);
    expect(target, "seed has a swappable spell").toBeTruthy();
    const old = target!.chosen!;
    renderApp("/spells", { api, openCharacterId: id });
    const table = await screen.findByRole("table", { name: "Selection slots" });
    expect(within(table).getByRole("columnheader", { name: "Gained on level" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Replace spell" })).toBeNull();

    const row = within(table).getAllByRole("row").find((r) => within(r).queryByRole("button", { name: old.name }))!;
    await user.click(within(row).getAllByRole("cell")[1]!);
    await user.click(await screen.findByRole("button", { name: "Replace spell" }));

    await waitFor(() => expect(getStatus()).toContain(`${old.name} is replaced`));
    const after = await api.getSnapshot({ characterId: id });
    const slot = after.slots.find((s) => s.id === target!.id)!;
    expect(slot.chosen).toBeNull();
    expect(slot.replaced.map((r) => r.id)).toEqual([old.id]);
    const reopened = within(screen.getByRole("table", { name: "Selection slots" })).getAllByRole("row").find((r) => r.textContent?.includes(old.name))!;
    expect(within(reopened).getByText("not selected")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Replace spell" })).toBeNull();
  });

  it("refuses to replace cantrips and empty slots", async () => {
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    const snap = await api.getSnapshot({ characterId: id });
    const cantrip = snap.slots.find((s) => s.tab === "spells" && s.chosen && !s.allowReplace);
    expect(cantrip).toBeTruthy();
    await expect(api.replaceSpell({ characterId: id, slotId: cantrip!.id })).rejects.toMatchObject({ code: "requirement_failed" });
  });
});
