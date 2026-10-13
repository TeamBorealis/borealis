import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { getStatus } from "../state/status";
import { renderApp, seededId } from "./utils";

async function reopened(api: MockApi) {
  const id = await seededId(api, "Seraphine");
  const snap = await api.getSnapshot({ characterId: id });
  const target = snap.slots.find((s) => s.tab === "spells" && s.allowReplace && s.chosen)!;
  expect(target, "seed has a swappable spell").toBeTruthy();
  await api.replaceSpell({ characterId: id, slotId: target.id });
  return { id, target };
}

describe("Magic refills a reopened spell slot", () => {
  it("lists the highlighted slot's options and registers a double-click pick, keeping the source cell", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const { id, target } = await reopened(api);
    const old = target.chosen!;
    renderApp("/spells", { api, openCharacterId: id });
    const slots = await screen.findByRole("table", { name: "Selection slots" });
    const row = within(slots).getAllByRole("row").find((r) => within(r).queryByText("not selected") && r.textContent?.includes(target.title))!;
    expect(row.textContent).toMatch(/Wizard|Sorcerer|Cleric|Bard|Druid|Warlock|Paladin|Ranger/);
    await user.click(within(row).getAllByRole("cell")[1]!);

    const catalogue = await screen.findByRole("table", { name: /spells$/  });
    expect(catalogue.getAttribute("data-slot-id")).toBe(target.id);
    const opt = await within(catalogue).findByRole("button", { name: old.name });
    await user.dblClick(opt);
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).slots.find((s) => s.id === target.id)!.chosen?.id).toBe(old.id));
    expect(getStatus()).not.toBe("");
  });

  it("registers a pick with Enter", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const { id, target } = await reopened(api);
    const old = target.chosen!;
    renderApp(`/spells?slot=${encodeURIComponent(target.id)}`, { api, openCharacterId: id });
    const catalogue = await screen.findByRole("table", { name: /spells$/ });
    expect(catalogue.getAttribute("data-slot-id")).toBe(target.id);
    const opt = await within(catalogue).findByRole("button", { name: old.name });
    opt.focus();
    await user.keyboard("{Enter}");
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).slots.find((s) => s.id === target.id)!.chosen?.id).toBe(old.id));
  });

  it("routes NEXT SELECTION for a spell slot to Magic with the slot highlighted", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const { id } = await reopened(api);
    const snap = await api.getSnapshot({ characterId: id });
    const nextSlot = snap.slots.find((s) => s.id === snap.progress.nextSlotId);
    expect(nextSlot?.tab).toBe("spells");
    renderApp("/builder/race", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: /Next selection/i }));
    const slots = await screen.findByRole("table", { name: "Selection slots" });
    await waitFor(() => {
      const sel = within(slots).getAllByRole("row").filter((r) => r.getAttribute("aria-selected") === "true");
      expect(sel).toHaveLength(1);
      expect(sel[0]!.textContent).toContain(nextSlot!.title);
    });
  });
});
