import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { AbilityMethod, AbilityRecord } from "../api/contract";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { applySuggested } from "../pages/builder/SuggestedScores";
import { renderApp, seededId } from "./utils";

// Tester B11 (B-135, TV2): "it would be optimal if it recommended the stats". On a phone Ability Scores offers
// "Suggested for <class>" (that class's standard array from the ruleset's tables) with Apply; it never applies by itself.
const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });

async function wizardWith(api: MockApi, method: AbilityMethod): Promise<string> {
  const id = await seededId(api, "Seraphine");
  await api.setAbilityMethod({ characterId: id, method });
  return id;
}

afterEach(() => setPhoneForTests(false));

describe("ability scores suggest the class's standard array and apply it (B11)", () => {
  it("shows the suggestion for the main class and Apply sets the base scores", async () => {
    setPhoneForTests(true);
    const user = userEvent.setup();
    const api = fast();
    const id = await wizardWith(api, "manual");
    const before = await api.getSnapshot({ characterId: id });
    const target = before.abilities.suggested!;
    expect(target.className).toBe("Wizard");
    renderApp("/builder/abilities", { api, openCharacterId: id });
    const card = await screen.findByRole("region", { name: "Suggested for Wizard" });
    expect(within(card).getByText(`INT ${target.scores.int}`)).toBeTruthy();
    expect((await api.getSnapshot({ characterId: id })).abilities.base).toEqual(before.abilities.base);
    await user.click(within(card).getByRole("button", { name: "Apply" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).abilities.base).toEqual(target.scores));
    expect(await within(card).findByRole("button", { name: "Applied" })).toHaveProperty("disabled", true);
  });

  it("under point buy, lowering scores comes first, so the budget is never overspent on the way", async () => {
    const api = fast();
    const id = await wizardWith(api, "point_buy");
    for (const [k, v] of [["str", 15], ["dex", 15], ["con", 15]] as const) await api.setBaseScore({ characterId: id, ability: k, value: v });
    const s = await api.getSnapshot({ characterId: id });
    const target = s.abilities.suggested!.scores;
    const after = await applySuggested(api, id, s.abilities, target);
    expect(after.abilities.base).toEqual(target);
  });

  it("under the standard array each value is assigned to its ability", async () => {
    const api = fast();
    const id = await wizardWith(api, "standard_array");
    const s = await api.getSnapshot({ characterId: id });
    const target: AbilityRecord<number> = s.abilities.suggested!.scores;
    const after = await applySuggested(api, id, s.abilities, target);
    expect(after.abilities.base).toEqual(target);
  });

  it("desktop shows no suggestion (Aurora has none)", async () => {
    const api = fast();
    const id = await wizardWith(api, "manual");
    renderApp("/builder/abilities", { api, openCharacterId: id });
    await screen.findByRole("tab", { name: "Ability Scores" });
    expect(screen.queryByRole("region", { name: /Suggested for/ })).toBeNull();
  });
});
