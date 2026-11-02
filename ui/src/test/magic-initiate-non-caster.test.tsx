import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { CharacterSnapshot, SelectionSlot } from "../api/contract";
import { setPhoneForTests } from "../lib/phone";
import { renderApp } from "./utils";

/**
 * B1 (tester video 2026-11-02): a 2024 non-caster with the Acolyte background gets Magic Initiate (Cleric). Its spell slots
 * route to Magic > Spellcasting, which must list them and offer their catalogue even with no spellcasting class.
 */
const MI_LABEL = "Magic Initiate (Cleric)";
const EMPTY = /doesn't have any spellcasting available here/;

async function create(api: MockApi, classId: string, backgroundId: string): Promise<CharacterSnapshot> {
  const created = await api.createCharacter({ request: { name: "Initiate", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
  const classSlot = created.slots.find((s) => s.elementType === "class")!;
  const withClass = await api.applySelection({ characterId: created.id, slotId: classSlot.id, elementId: classId });
  const bgSlot = withClass.slots.find((s) => s.elementType === "background")!;
  return api.applySelection({ characterId: created.id, slotId: bgSlot.id, elementId: backgroundId });
}

const miSlots = (s: CharacterSnapshot): SelectionSlot[] => s.slots.filter((x) => x.tab === "spells" && x.title.startsWith("Magic Initiate"));

/** Fills every required open slot outside Magic with its first option, so NEXT SELECTION lands on a spell slot. */
async function fillAllButSpells(api: MockApi, id: string): Promise<CharacterSnapshot> {
  let snap = await api.getSnapshot({ characterId: id });
  for (let i = 0; i < 40; i++) {
    const next = snap.slots.find((s) => s.id === snap.progress.nextSlotId);
    if (!next || next.tab === "spells") return snap;
    const options = await api.getSlotOptions({ characterId: id, slotId: next.id, query: "" });
    const pick = options.items.find((o) => !o.disabledReason)!;
    snap = await api.applySelection({ characterId: id, slotId: next.id, elementId: pick.id });
  }
  return snap;
}

afterEach(() => setPhoneForTests(false));

describe("Magic Initiate on a character with no spellcasting class (B1)", () => {
  it("a non-caster's Magic Initiate slots are listed on Magic > Spellcasting", async () => {
    const api = new MockApi();
    const snap = await create(api, "fighter", "acolyte");
    expect(snap.spellcasting.classes).toEqual([]);
    const slots = miSlots(snap);
    expect(slots).toHaveLength(3);
    for (const s of slots) {
      expect(s.classSlotId).toBeNull();
      expect(s.groupLabel).toBe(MI_LABEL);
    }

    renderApp("/spells", { api, openCharacterId: snap.id });
    const table = await screen.findByRole("table", { name: "Selection slots" });
    expect(screen.queryByText(EMPTY)).toBeNull();
    const open = within(table).getAllByRole("row").filter((r) => within(r).queryByText("not selected"));
    const expected = ["Magic Initiate Cantrips (1 of 2)", "Magic Initiate Cantrips (2 of 2)", "Magic Initiate Level 1 Spell"];
    expect(open).toHaveLength(expected.length);
    open.forEach((r, i) => expect(within(r).getAllByRole("cell")[1]!.textContent?.startsWith(expected[i]!), r.textContent ?? "").toBe(true));
    for (const r of open) expect(within(r).getAllByRole("cell")[5]!.textContent).toBe(MI_LABEL);
  });

  it("picks a Magic Initiate cantrip from the slot catalogue without any spellcasting class", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await create(api, "fighter", "acolyte");
    const slot = miSlots(snap)[0]!;
    renderApp("/spells", { api, openCharacterId: snap.id });
    const table = await screen.findByRole("table", { name: "Selection slots" });
    const row = within(table).getAllByRole("row").find((r) => r.textContent?.includes("Magic Initiate Cantrips (1 of 2)"))!;
    await user.click(within(row).getAllByRole("cell")[1]!);
    expect(row.getAttribute("aria-selected")).toBe("true");

    const block = await screen.findByRole("region", { name: `${MI_LABEL} spellcasting` });
    expect(within(block).getByRole("heading", { name: MI_LABEL })).toBeTruthy();
    const catalogue = within(block).getByRole("table", { name: `${MI_LABEL} spells` });
    expect(catalogue.getAttribute("data-slot-id")).toBe(slot.id);
    const options = await api.getSlotOptions({ characterId: snap.id, slotId: slot.id, query: "" });
    await within(catalogue).findByRole("button", { name: options.items[0]!.name });
    const shown = within(catalogue).getAllByRole("button").map((b) => b.textContent).filter((t) => options.items.some((o) => o.name === t));
    expect(shown).toHaveLength(options.items.length);
    expect(within(catalogue).queryByRole("button", { name: "Fire Bolt" })).toBeNull();

    const pick = options.items.find((o) => o.name === "Sacred Flame")!;
    await user.dblClick(within(catalogue).getByRole("button", { name: pick.name }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: snap.id })).slots.find((s) => s.id === slot.id)!.chosen?.id).toBe(pick.id));
  });

  it("a phone chooses a Magic Initiate spell with the Choose button", async () => {
    setPhoneForTests(true);
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await create(api, "fighter", "acolyte");
    const slot = miSlots(snap).find((s) => s.title === "Magic Initiate Level 1 Spell")!;
    renderApp(`/spells?slot=${encodeURIComponent(slot.id)}`, { api, openCharacterId: snap.id });
    const catalogue = await screen.findByRole("table", { name: `${MI_LABEL} spells` });
    expect(screen.queryByRole("button", { name: /^Choose / })).toBeNull();
    await user.click(await within(catalogue).findByRole("button", { name: "Bless" }));
    await user.click(await screen.findByRole("button", { name: "Choose Bless" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: snap.id })).slots.find((s) => s.id === slot.id)!.chosen?.name).toBe("Bless"));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Choose Bless" })).toBeNull());
  });

  it("Next selection lands on the highlighted Magic Initiate row", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const { id } = await create(api, "fighter", "acolyte");
    const snap = await fillAllButSpells(api, id);
    const next = snap.slots.find((s) => s.id === snap.progress.nextSlotId)!;
    expect(next.title).toMatch(/^Magic Initiate/);
    renderApp("/builder/race", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: /Next selection/i }));
    const table = await screen.findByRole("table", { name: "Selection slots" });
    await waitFor(() => {
      const sel = within(table).getAllByRole("row").filter((r) => r.getAttribute("aria-selected") === "true");
      expect(sel).toHaveLength(1);
      expect(sel[0]!.textContent).toContain(next.title);
    });
    const catalogue = await screen.findByRole("table", { name: `${MI_LABEL} spells` });
    expect(catalogue.getAttribute("data-slot-id")).toBe(next.id);
  });

  it("the empty message shows only with no casting and no spell slots", async () => {
    const api = new MockApi();
    const soldier = await create(api, "fighter", "soldier");
    expect(soldier.slots.some((s) => s.tab === "spells")).toBe(false);
    const view = renderApp("/spells", { api, openCharacterId: soldier.id });
    expect(await screen.findByText(EMPTY)).toBeTruthy();
    view.unmount();

    const acolyte = await create(api, "fighter", "acolyte");
    renderApp("/spells", { api, openCharacterId: acolyte.id });
    await screen.findByRole("table", { name: "Selection slots" });
    expect(screen.queryByText(EMPTY)).toBeNull();
  });

  it("a Wizard with Acolyte sees the Magic Initiate catalogue in its own block, not the Wizard's", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await create(api, "wizard", "acolyte");
    const slot = miSlots(snap)[0]!;
    renderApp(`/spells?slot=${encodeURIComponent(slot.id)}`, { api, openCharacterId: snap.id });
    const catalogue = await screen.findByRole("table", { name: `${MI_LABEL} spells` });
    expect(catalogue.getAttribute("data-slot-id")).toBe(slot.id);
    const wizard = screen.getByRole("region", { name: "Wizard spellcasting" });
    expect(within(wizard).queryByRole("table", { name: `${MI_LABEL} spells` })).toBeNull();
    expect(within(wizard).queryByRole("table", { name: "Wizard spells" })).toBeTruthy();
    // A Wizard slot highlighted again puts the catalogue back in the Wizard block.
    const table = screen.getByRole("table", { name: "Selection slots" });
    const wizRow = within(table).getAllByRole("row").find((r) => r.textContent?.includes("Wizard cantrip") && within(r).queryByText("not selected"))!;
    await user.click(within(wizRow).getAllByRole("cell")[1]!);
    await waitFor(() => expect(screen.queryByRole("region", { name: `${MI_LABEL} spellcasting` })).toBeNull());
  });
});
