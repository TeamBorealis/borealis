import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { CharacterSnapshot } from "../api/contract";
import { MockApi } from "../api/mock";
import { describeChange, getStatus } from "../state/status";
import { renderApp } from "./utils";

/**
 * A10 (tester video 2026-11-02): the gold prompt names the slot Next selection goes to. It follows the next slot when that
 * changes without a pick (a clear), and Next selection ranks the open species sub-choice ahead of Class (Aurora's groups).
 */
const PROMPT = (title: string) => `Choose one from the '${title}' selection option.`;

async function dragonborn(api: MockApi): Promise<CharacterSnapshot> {
  const created = await api.createCharacter({ request: { name: "Torrin", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
  const race = created.slots.find((s) => s.elementType === "race")!;
  return api.applySelection({ characterId: created.id, slotId: race.id, elementId: "dragonborn-2024" });
}

const nextOf = (s: CharacterSnapshot) => s.slots.find((x) => x.id === s.progress.nextSlotId);

describe("Next selection and the status prompt (A10)", () => {
  it("ranks the open species sub-choice ahead of Class", async () => {
    const snap = await dragonborn(new MockApi());
    expect(snap.slots.find((s) => s.elementType === "class")?.chosen).toBeNull();
    expect(nextOf(snap)).toMatchObject({ tab: "race", title: "Draconic Ancestry" });
  });

  it("re-prompts when the next selection changes without a pick (a clear)", () => {
    const slot = (id: string, title: string, chosen: boolean) => ({ id, title, locked: false, chosen: chosen ? { id: `${id}-pick`, name: "X" } : null });
    const base = { name: "T", level: { total: 1 }, inventory: { items: [] } };
    const prev = { ...base, slots: [slot("race", "Race", true), slot("class", "Class", false)], progress: { remaining: 1, nextSlotId: "class" } } as unknown as CharacterSnapshot;
    const cleared = { ...base, slots: [slot("race", "Race", false), slot("class", "Class", false)], progress: { remaining: 2, nextSlotId: "race" } } as unknown as CharacterSnapshot;
    expect(describeChange(prev, cleared)).toBe(PROMPT("Race"));
  });

  it("clearing the species in the builder prompts for the species again", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const snap = await dragonborn(api);
    renderApp("/builder/race", { api, openCharacterId: snap.id });
    const clear = await screen.findByRole("button", { name: /^Clear Dragonborn/ });
    await user.click(clear);
    await waitFor(() => expect(getStatus()).toBe(PROMPT(snap.slots.find((s) => s.elementType === "race")!.title)));
  });
});
