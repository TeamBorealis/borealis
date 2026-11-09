import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { CharacterSnapshot } from "../api/contract";
import { MockApi } from "../api/mock";
import { openPicksFor } from "../components/shell/tabBadges";
import { setPhoneForTests } from "../lib/phone";
import { BUY_OWN, openPackages } from "../pages/equipment/startingPackages";
import { nextSelectionPrompt } from "../state/status";
import { renderApp, seededId } from "./utils";

// Tester B3 (B-129, TV2): the starting-equipment chooser sat under Gear > "Additional treasure", so the tester thought he had
// to buy every item one by one. On a phone it now opens Gear > Equipment while a package is untaken, and an untaken package
// counts as an open choice (Gear badge, Next selection) until taken or dismissed with "I'll buy my own".
const PKG = "Mock starting equipment";

/** Every pick made, so the starting equipment is the only open choice. */
class AllPickedApi extends MockApi {
  override async loadCharacter(args: { characterId: string }): Promise<CharacterSnapshot> {
    return allPicked(await super.loadCharacter(args));
  }
  override async getSnapshot(args: { characterId: string }): Promise<CharacterSnapshot> {
    return allPicked(await super.getSnapshot(args));
  }
  override async chooseEquipmentPackage(a: { characterId: string; packageId: string; option: string }): Promise<CharacterSnapshot> {
    return allPicked(await super.chooseEquipmentPackage(a));
  }
}
const allPicked = (s: CharacterSnapshot): CharacterSnapshot => ({
  ...s, progress: { ...s.progress, remaining: 0, nextSlotId: null, byTab: Object.fromEntries(Object.keys(s.progress.byTab).map((k) => [k, 0])) as CharacterSnapshot["progress"]["byTab"] },
});

/** jsdom has no matchMedia: answer the narrow-shell query as a phone screen would. */
function narrowScreen(): () => void {
  const original = window.matchMedia;
  window.matchMedia = ((q: string) => ({
    matches: q.includes("max-width: 899px"), media: q, onchange: null, addEventListener: () => undefined,
    removeEventListener: () => undefined, addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false,
  })) as typeof window.matchMedia;
  return () => { window.matchMedia = original; };
}

let restore: () => void = () => undefined;
afterEach(() => { restore(); setPhoneForTests(false); });

async function openPhone(route: string, api = new AllPickedApi()) {
  restore = narrowScreen();
  setPhoneForTests(true);
  // A new character: no items yet, so Gear opens on its Equipment sub-tab.
  const { id } = await api.createCharacter({ request: { name: "Fresh", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
  const utils = renderApp(route, { api, openCharacterId: id });
  return { id, ...utils };
}

const chooser = () => screen.findByRole("region", { name: "Starting equipment" });

describe("starting equipment on Gear > Equipment (B3)", () => {
  it("Gear > Equipment shows the starting-equipment chooser for a new character", async () => {
    const user = userEvent.setup();
    await openPhone("/equipment");
    const box = await chooser();
    expect(screen.getByRole("tab", { name: "Equipment" }).getAttribute("aria-selected")).toBe("true");
    expect(within(box).getByRole("button", { name: "Starting equipment" }).getAttribute("aria-expanded")).toBe("true");
    await user.click(within(box).getByRole("radio", { name: /Option A/ }));
    await user.click(within(box).getByRole("button", { name: "Take option A" }));
    await waitFor(() => expect(within(box).getByRole("button", { name: "Starting equipment" }).getAttribute("aria-expanded")).toBe("false"));
    await user.click(within(box).getByRole("button", { name: "Starting equipment" }));
    expect(within(box).getByText("Option A taken")).toBeTruthy();
  });

  it("\"I'll buy my own\" closes the package without taking an option", async () => {
    const user = userEvent.setup();
    const { api, id } = await openPhone("/equipment");
    const box = await chooser();
    await user.click(within(box).getByRole("button", { name: `I'll buy my own (${PKG})` }));
    await waitFor(async () => expect(openPackages(await api.getSnapshot({ characterId: id }))).toEqual([]));
    const pkg = (await api.getSnapshot({ characterId: id })).play.equipmentPackages[0]!;
    expect(pkg.chosen).toBeNull();
    expect(pkg.declined).toBe(true);
  });

  it("an untaken package badges Gear and is where Next selection goes once every pick is made", async () => {
    const user = userEvent.setup();
    await openPhone("/characters");
    expect(await screen.findByRole("link", { name: "Gear, 1 choices remaining" })).toBeTruthy();
    expect(document.querySelector(".aur-status")?.getAttribute("data-pending")).toBe("true");
    await user.click(screen.getByRole("button", { name: /Next selection/ }));
    expect(await chooser()).toBeTruthy();
  });

  it("Next selection opens the Equipment sub-tab even for a character with items (which otherwise opens on Inventory)", async () => {
    restore = narrowScreen();
    setPhoneForTests(true);
    const user = userEvent.setup();
    const api = new AllPickedApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/characters", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: /Next selection/ }));
    expect(await chooser()).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Equipment" }).getAttribute("aria-selected")).toBe("true");
  });

  it("the open-package helpers: declined and taken packages are done", () => {
    const s = (chosen: string | null, declined?: boolean) => ({
      progress: { remaining: 0, nextSlotId: null, byTab: {} }, slots: [], ruleset: "dnd2024",
      play: { equipmentPackages: [{ package: PKG, name: PKG, owner: "x", chosen, declined, options: [] }] },
    }) as unknown as CharacterSnapshot;
    expect(BUY_OWN).toBe("none");
    expect(openPackages(s(null))).toHaveLength(1);
    expect(openPackages(s(null, true))).toHaveLength(0);
    expect(openPackages(s("A"))).toHaveLength(0);
    expect(openPicksFor("equipment", s(null), true)).toBe(1);
    expect(openPicksFor("equipment", s(null), false)).toBe(0);
    setPhoneForTests(true);
    expect(nextSelectionPrompt(s(null))).toBe("Next: choose your starting equipment (Gear tab)");
    expect(nextSelectionPrompt(s("A"))).toBe("All required selections are made.");
    setPhoneForTests(false);
    expect(nextSelectionPrompt(s(null))).toBe("All required selections are made.");
  });

  it("desktop keeps the chooser under Additional treasure", async () => {
    const user = userEvent.setup();
    const api = new AllPickedApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/equipment", { api, openCharacterId: id });
    await screen.findByRole("tab", { name: "Additional treasure" });
    expect(screen.queryByRole("region", { name: "Starting equipment" })).toBeNull();
    await user.click(screen.getByRole("tab", { name: "Additional treasure" }));
    expect(await chooser()).toBeTruthy();
  });
});
