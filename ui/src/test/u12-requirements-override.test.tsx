import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { renderApp } from "./utils";

// U12 (GitHub #3): an equipped "Additional Feat" line whose feat's own requirements fail (an epic boon before level 19)
// offers "Override requirements"; on, the line says "Requirements overridden" and the flag is stored on the line; off
// withholds the feat again. The mock's Boon of Fate has the "Level 19+" prerequisite.
async function boonCharacter() {
  const api = new MockApi({ rng: () => 0.5 });
  const snap = await api.createCharacter({ request: { name: "Fate", ruleset: "dnd2024", startingLevel: 4, gender: "", abilityMethod: "manual", options: {} } });
  const proxy = (await api.searchItems({ category: "Additional Feat", query: "Boon of Fate", limit: 5, characterId: snap.id })).items[0]!;
  await api.addItem({ characterId: snap.id, itemId: proxy.id, quantity: 1, buy: false });
  return { api, id: snap.id, name: proxy.name };
}

function narrowScreen() {
  const original = window.matchMedia;
  window.matchMedia = ((q: string) => ({
    matches: q.includes("max-width: 899px"), media: q, onchange: null, addEventListener: () => undefined,
    removeEventListener: () => undefined, addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false,
  })) as typeof window.matchMedia;
  return () => { window.matchMedia = original; };
}

let restore: () => void = () => undefined;
afterEach(() => { restore(); restore = () => undefined; setPhoneForTests(false); });

describe.each([["desktop", false], ["phone", true]] as const)("Override requirements on an item line (%s)", (_layout, phone) => {
  it("is offered once the equipped boon is withheld, applies on, and withholds again off", async () => {
    if (phone) { restore = narrowScreen(); setPhoneForTests(true); }
    const user = userEvent.setup();
    const { api, id, name } = await boonCharacter();
    renderApp("/equipment", { api, openCharacterId: id });
    const inv = await screen.findByRole("region", { name: "Inventory" });
    expect(within(inv).queryByRole("checkbox", { name: `Override requirements for ${name}` })).toBeNull();
    await user.click(await within(inv).findByRole("button", { name: `Equip ${name}` }));
    expect(await within(inv).findByText("Not applied: Boon of Fate (requirements not met)")).toBeTruthy();
    const toggle = within(inv).getByRole("checkbox", { name: `Override requirements for ${name}` });
    await user.click(toggle);
    expect(await within(inv).findByText("Requirements overridden")).toBeTruthy();
    const line = async () => (await api.getSnapshot({ characterId: id })).inventory.items.find((i) => i.name === name)!;
    expect((await line()).overrideRequirements).toBe(true);
    expect((await line()).withheldGrants).toEqual([]);
    await user.click(within(inv).getByRole("checkbox", { name: `Override requirements for ${name}` }));
    await waitFor(async () => expect((await line()).overrideRequirements).toBe(false));
    expect(await within(inv).findByText("Not applied: Boon of Fate (requirements not met)")).toBeTruthy();
  });
});
