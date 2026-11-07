import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PlayResource, PlayToggle } from "../api/contract";
import { MockApi } from "../api/mock";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { renderApp, seededId } from "./utils";

// B-178 (follows B-140, tester eylonke): a state with a use pool of the same id (Rage) is switched on that pool's row in
// Resources, so spending and switching are one place, and it is not listed again under States. Generic: any state with a
// same-id pool; a state without one stays in States.

const LAYOUTS = [["desktop", false], ["phone", true]] as const;

afterEach(() => { setPhoneForTests(false); setPhoneOsForTests(null); });

async function openPlay(phone: boolean, prep?: (api: MockApi) => void) {
  if (phone) { setPhoneForTests(true); setPhoneOsForTests("ios"); }
  const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
  const characterId = await seededId(api, "Seraphine");
  prep?.(api);
  renderApp("/play", { api, openCharacterId: characterId });
  return { api, user: userEvent.setup() };
}

/** The Resources row (list item) of the pool `name`. */
async function poolRow(name: string): Promise<HTMLElement> {
  const resources = await screen.findByRole("list", { name: "Resources" });
  const status = within(resources).getByRole("status", { name: new RegExp(`^${name}: \\d+ of \\d+ uses left$`) });
  return status.closest("li") as HTMLElement;
}

describe.each(LAYOUTS)("a state on its use pool's row (%s)", (_layout, phone) => {
  it("the Rage switch sits on the Rage pool's row, next to Spend, and is not in States", async () => {
    const { user } = await openPlay(phone);
    const row = await poolRow("Rage");
    const toggle = within(row).getByRole("switch", { name: "Rage" }) as HTMLInputElement;
    expect(within(row).getByRole("button", { name: "Spend Rage" })).toBeTruthy();
    // the only switch named Rage on the page, and no States card left (the mock's only state is Rage)
    expect(screen.getAllByRole("switch", { name: "Rage" })).toHaveLength(1);
    expect(screen.queryByRole("list", { name: "States" })).toBeNull();

    const count = within(row).getByRole("status", { name: /^Rage:/ });
    const max = Number(/of (\d+)/.exec(count.getAttribute("aria-label")!)![1]);
    await user.click(toggle);
    await waitFor(() => expect(toggle.checked).toBe(true));
    // switching on spends a use (B-140) and the row says the state is in effect
    expect(count.textContent).toBe(`${max - 1}/${max}`);
    expect(within(row).getByText("Active")).toBeTruthy();
  });

  it("selecting the pool's row shows the state's effects in the description", async () => {
    const { user } = await openPlay(phone);
    const row = await poolRow("Rage");
    await user.click(within(row).getByRole("button", { name: /^Rage/ }));
    expect(await screen.findByRole("heading", { name: "State" })).toBeTruthy();
    expect(screen.getByText("Resistance to Bludgeoning, Piercing and Slashing damage")).toBeTruthy();
  });
});

describe("any state with a same-id pool (generic)", () => {
  const pool: PlayResource = {
    id: "innate sorcery", name: "Innate Sorcery", max: 2, current: 2, source: "Sorcerer", recharge: ["long_rest"],
    shortRestRegain: null, longRestRegainAll: true,
  };
  const state = (id: string, name: string): PlayToggle => ({
    id, name, on: false, active: false, activation: "Bonus Action", duration: "1 minute", blocks: [], effects: [], source: "Test",
  });

  it("puts every pooled state on its pool's row and keeps a state without a pool in States", async () => {
    await openPlay(false, (api) => {
      const load = api.loadCharacter.bind(api);
      vi.spyOn(api, "loadCharacter").mockImplementation(async (req) => {
        const s = await load(req);
        return { ...s, play: { ...s.play, resources: [...s.play.resources, pool], toggles: [...s.play.toggles, state("innate sorcery", "Innate Sorcery"), state("hover", "Hover")] } };
      });
    });
    const sorcery = await poolRow("Innate Sorcery");
    expect(within(sorcery).getByRole("switch", { name: "Innate Sorcery" })).toBeTruthy();
    expect(within(await poolRow("Rage")).getByRole("switch", { name: "Rage" })).toBeTruthy();
    const states = screen.getByRole("list", { name: "States" });
    expect(within(states).getAllByRole("switch").map((s) => s.getAttribute("aria-label"))).toEqual(["Hover"]);
  });
});
