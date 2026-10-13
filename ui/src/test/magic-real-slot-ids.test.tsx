import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { CharacterSnapshot } from "../api/contract";
import { getStatus } from "../state/status";
import { renderApp } from "./utils";

/** The real backend's slot id shape (src-tauri ids.rs encode_slot): `owner:rule:ordinal[@instance]`. */
const REAL_SLOT_ID = /^[^:@]+:\d+:\d+(@.+)?$/;

async function freshWizard(api: MockApi): Promise<CharacterSnapshot> {
  const created = await api.createCharacter({ request: { name: "Real Ids", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
  const classSlot = created.slots.find((s) => s.elementType === "class")!;
  return api.applySelection({ characterId: created.id, slotId: classSlot.id, elementId: "wizard" });
}

const wizardOf = (s: CharacterSnapshot) => s.spellcasting.classes.find((c) => c.className === "Wizard")!;

describe("Magic page with real-shaped slot ids", () => {
  it("the mock emits every slot id in the real owner:rule:ordinal shape, linked to its class by classSlotId", async () => {
    const snap = await freshWizard(new MockApi());
    const cls = wizardOf(snap);
    for (const s of snap.slots) {
      expect(s.id).toMatch(REAL_SLOT_ID);
      expect(s.id.startsWith(`${cls.classSlotId}:`), s.id).toBe(false);
    }
    const spellSlots = snap.slots.filter((s) => s.tab === "spells");
    expect(spellSlots.length).toBeGreaterThan(0);
    expect(spellSlots.every((s) => s.classSlotId === cls.classSlotId)).toBe(true);
    expect(snap.slots.filter((s) => s.tab !== "spells").every((s) => s.classSlotId === null)).toBe(true);
  });

  it("picks a cantrip and a level-1 spell from the slot catalogue, then prepares the spell on the class tab", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const { id } = await freshWizard(api);
    renderApp("/spells", { api, openCharacterId: id });

    const pickInto = async (slotTitle: string, level: (lvl: number | undefined) => boolean) => {
      const table = await screen.findByRole("table", { name: "Selection slots" });
      const row = within(table).getAllByRole("row").find((r) => r.textContent?.includes(slotTitle) && within(r).queryByText("not selected"))!;
      expect(row, `open ${slotTitle} slot`).toBeTruthy();
      expect(row.textContent).toContain("Wizard");
      await user.click(within(row).getAllByRole("cell")[1]!);
      const catalogue = await screen.findByRole("table", { name: "Wizard spells" });
      const slotId = catalogue.getAttribute("data-slot-id")!;
      expect(slotId).toMatch(REAL_SLOT_ID);
      const options = await api.getSlotOptions({ characterId: id, slotId, query: "" });
      const option = options.items.find((o) => level(o.level))!;
      await user.dblClick(await within(catalogue).findByRole("button", { name: option.name }));
      await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).slots.find((s) => s.id === slotId)!.chosen?.id).toBe(option.id));
      return option;
    };

    const cantrip = await pickInto("cantrip", (l) => l === 0);
    const spell = await pickInto("Spellbook spell", (l) => (l ?? 0) >= 1);
    const known = wizardOf(await api.getSnapshot({ characterId: id })).spells.map((s) => s.id);
    expect(known).toEqual(expect.arrayContaining([cantrip.id, spell.id]));

    await user.click(await screen.findByRole("tab", { name: /wizard/i }));
    const grid = await screen.findByRole("table", { name: /wizard spells/i });
    await user.click(within(grid).getByRole("button", { name: spell.name }));
    await user.click(screen.getByRole("button", { name: "Prepare" }));
    await waitFor(async () => expect(wizardOf(await api.getSnapshot({ characterId: id })).spells.find((s) => s.id === spell.id)!.prepared).toBe(true));
    expect(getStatus()).toBe(`Wizard: ${spell.name} is prepared.`);
  });
});
