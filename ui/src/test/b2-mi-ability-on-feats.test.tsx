import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { CharacterSnapshot, SelectionSlot } from "../api/contract";
import { setPhoneForTests } from "../lib/phone";
import { renderApp } from "./utils";

/** The phone's Choose button under the list (B-48 O2); each row's ring (A1) is also named "Choose <name>". */
const chooseButton = (name: string) => waitFor(() => {
  const found = screen.getAllByRole("button", { name: `Choose ${name}` }).find((b) => b.classList.contains("bp-choose-btn"));
  expect(found).toBeTruthy();
  return found!;
});

/**
 * B2 (tester video 2026-11-02): Magic Initiate's "Spellcasting Ability" is the feat's own choice. It sits on Build > Feats
 * under the feat's name (not on Class), and Next selection asks it before the Magic Initiate spells that depend on it.
 */
const MI_LABEL = "Magic Initiate (Cleric)";
const ABILITY = "Spellcasting Ability";

async function acolyte(api: MockApi): Promise<CharacterSnapshot> {
  const created = await api.createCharacter({ request: { name: "Torrin", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
  const classSlot = created.slots.find((s) => s.elementType === "class")!;
  const withClass = await api.applySelection({ characterId: created.id, slotId: classSlot.id, elementId: "fighter" });
  const bgSlot = withClass.slots.find((s) => s.elementType === "background")!;
  return api.applySelection({ characterId: created.id, slotId: bgSlot.id, elementId: "acolyte" });
}

const abilitySlot = (s: CharacterSnapshot): SelectionSlot => s.slots.find((x) => x.title === ABILITY)!;

afterEach(() => setPhoneForTests(false));

describe("Magic Initiate's Spellcasting Ability (B2)", () => {
  it("is a feats-tab list slot named after the feat, asked before the Magic Initiate spells", async () => {
    const api = new MockApi();
    let snap = await acolyte(api);
    const slot = abilitySlot(snap);
    expect(slot).toMatchObject({ tab: "feats", elementType: "list", groupLabel: MI_LABEL, required: true, locked: false, chosen: null });
    for (let i = 0; i < 40; i++) {
      const next = snap.slots.find((s) => s.id === snap.progress.nextSlotId);
      if (!next || next.tab === "feats" || next.tab === "spells") break;
      const options = await api.getSlotOptions({ characterId: snap.id, slotId: next.id, query: "" });
      snap = await api.applySelection({ characterId: snap.id, slotId: next.id, elementId: options.items.find((o) => !o.disabledReason)!.id });
    }
    expect(snap.progress.nextSlotId).toBe(slot.id);
  });

  it.each([["desktop", false], ["phone", true]])("Build > Feats lists it under Magic Initiate and chooses it (%s)", async (_name, phone) => {
    setPhoneForTests(phone);
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await acolyte(api);
    const slot = abilitySlot(snap);
    renderApp("/builder/feats", { api, openCharacterId: snap.id });

    const section = await screen.findByRole("region", { name: MI_LABEL });
    expect(within(section).getByRole("heading", { name: MI_LABEL })).toBeTruthy();
    const options = await within(section).findByRole("list", { name: `${ABILITY} options` });
    const wisdom = await within(options).findByRole("button", { name: /^Wisdom/ });
    if (phone) {
      await user.click(wisdom);
      await user.click(await chooseButton("Wisdom"));
    } else {
      await user.dblClick(wisdom);
    }
    await waitFor(async () => expect((await api.getSnapshot({ characterId: snap.id })).slots.find((s) => s.id === slot.id)!.chosen?.name).toBe("Wisdom"));
  });
});
