import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { CompanionState } from "../api/contract";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

const ABILITIES = { str: 10, dex: 12, con: 10, int: 2, wis: 10, cha: 6 };
const base: CompanionState = {
  creature: null, name: "Pip", portrait: null, armorClass: 12, hp: 9, armorClassNote: null, hpNote: null, challenge: "1/8", speed: 40, abilities: ABILITIES,
};

async function openCompanion(companion: CompanionState) {
  const api = new MockApi();
  const id = await seededId(api, "Seraphine");
  const load = api.loadCharacter.bind(api);
  vi.spyOn(api, "loadCharacter").mockImplementation(async (a) => ({ ...(await load(a)), companion }));
  const user = userEvent.setup();
  renderApp("/manage", { api, openCharacterId: id });
  await user.click(await screen.findByRole("tab", { name: "Companion" }));
  return within(await screen.findByLabelText("Companion statistics"));
}

describe("the Companion tab statistics", () => {
  it("shows the challenge rating beside armor class, hit points and speed", async () => {
    const stats = await openCompanion(base);
    expect(stats.getByText("CR").nextSibling?.textContent).toBe("1/8");
    expect(stats.getByText("AC").nextSibling?.textContent).toBe("12");
    expect(stats.getByText("HP").nextSibling?.textContent).toBe("9");
  });

  it("shows the note, or a dash, for a creature without a challenge rating (IM12)", async () => {
    const stats = await openCompanion({ ...base, challenge: "Scales with spell level" });
    expect(stats.getByText("CR").nextSibling?.textContent).toBe("Scales with spell level");
  });

  it("shows armor class and hit points printed as text (IM12a), never an invented 0", async () => {
    const stats = await openCompanion({ ...base, challenge: "\u2014", armorClass: 0, hp: 0, armorClassNote: "11 + the level of the spell", hpNote: "40 + 10 for each spell level above 4th" });
    expect(stats.getByText("AC").nextSibling?.textContent).toBe("11 + the level of the spell");
    expect(stats.getByText("HP").nextSibling?.textContent).toBe("40 + 10 for each spell level above 4th");
    expect(stats.getByText("CR").nextSibling?.textContent).toBe("\u2014");
  });
});
