import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { formatPrerequisite, meetsPrerequisite } from "../pages/builder/MulticlassList";
import { swapTiles } from "../pages/characters/NewCharacterDialog";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

describe("new character flyout", () => {
  it("swaps standard-array tiles and applies the arrangement to the new character", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
    const dialog = await screen.findByRole("dialog", { name: "New character" });
    expect(within(dialog).getByRole("button", { name: "Random name" }).closest(".v4-namefield")).toBeTruthy(); // dice sits inside the field
    await user.selectOptions(within(dialog).getByLabelText("Abilities generation option"), "standard_array");
    await user.click(within(dialog).getByRole("button", { name: /^STR 15/ }));
    await user.click(within(dialog).getByRole("button", { name: /^CON 13/ }));
    expect(within(dialog).getByRole("button", { name: /^STR 13/ })).toBeTruthy();
    await user.type(within(dialog).getByLabelText("Name"), "Arranger");
    await user.click(within(dialog).getByRole("button", { name: "Create character" }));
    await waitFor(async () => expect((await api.listCharacters()).some((c) => c.name === "Arranger")).toBe(true));
    const id = (await api.listCharacters()).find((c) => c.name === "Arranger")!.id;
    await waitFor(async () => {
      const snap = await api.getSnapshot({ characterId: id });
      expect(snap.abilities.base.str).toBe(13);
      expect(snap.abilities.base.con).toBe(15);
    });
  });

  it("swapTiles returns a new array", () => {
    const a = [15, 14, 13, 12, 10, 8];
    expect(swapTiles(a, 0, 2)).toEqual([13, 14, 15, 12, 10, 8]);
    expect(a[0]).toBe(15);
  });
});

describe("multiclass prerequisites", () => {
  it("formats and evaluates ability prerequisites", () => {
    expect(formatPrerequisite("Strength or Dexterity 13")).toBe("Strength 13 or Dexterity 13");
    expect(formatPrerequisite("Wisdom 13")).toBe("Wisdom 13");
    const f = { str: 8, dex: 14, con: 10, int: 10, wis: 10, cha: 10 };
    expect(meetsPrerequisite("Strength or Dexterity 13", f)).toBe(true);
    expect(meetsPrerequisite("Wisdom 13", f)).toBe(false);
    expect(meetsPrerequisite("Strength and Dexterity 13", f)).toBe(false);
  });
});

describe("level up & multiclassing flyout", () => {
  it("shows per-class prerequisites, no XP input, and adds a multiclass through the existing API", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    renderApp("/sheet", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: "Level Up" }));
    const dlg = await screen.findByRole("dialog", { name: "Level up & multiclassing" });
    expect(within(dlg).queryByLabelText("Experience points")).toBeNull();
    const fighter = await within(dlg).findByLabelText("Fighter multiclass");
    expect(within(fighter).getByText("Prerequisites: Strength 13 or Dexterity 13")).toBeTruthy();
    const wisdom = within(dlg).getByLabelText("Cleric multiclass");
    expect((within(wisdom).getByRole("button", { name: /New multiclass/ }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(within(fighter).getByRole("button", { name: /New multiclass/ }));
    // An empty slot at the next level, the flyout closes and Build > Class shows the Multiclass picker.
    await waitFor(async () => {
      const snap = await api.getSnapshot({ characterId: id });
      expect(snap.level.total).toBe(3);
      expect(snap.level.classes.map((c) => c.name)).toEqual(["Wizard", null]);
    });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Level up & multiclassing" })).toBeNull());
    expect(await screen.findByText("MULTICLASS")).toBeTruthy();
    // Reopen: the slot reads EMPTY (n) with the picker hint, and removing the level names it.
    await user.click(await screen.findByRole("button", { name: "Level Up" }));
    const again = await screen.findByRole("dialog", { name: "Level up & multiclassing" });
    expect(within(again).getByText("EMPTY (1)")).toBeTruthy();
    expect(within(again).getByText(/Multiclass \(Level N\) selection picker/)).toBeTruthy();
    await user.click(within(again).getByRole("button", { name: /Remove the last level/ }));
    expect(await screen.findByText(/\(the empty multiclass slot\)/)).toBeTruthy();
    await user.click(await screen.findByRole("button", { name: "Remove level" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).level.total).toBe(2));
  });
});

describe("profile flyouts", () => {
  it("opens the quick view from the profile name and closes it with Escape", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    renderApp("/sheet", { api, openCharacterId: id });
    const name = await waitFor(() => { const el = document.querySelector<HTMLButtonElement>(".aur-profile .name"); if (!el) throw new Error("no name"); return el; });
    await user.click(name);
    const dlg = await screen.findByRole("dialog", { name: /Seraphine/ });
    expect(within(dlg).getByText(/^Level 2 /)).toBeTruthy();
    expect(within(dlg).getByRole("button", { name: "Skills" })).toBeTruthy();
    expect(within(dlg).getByRole("button", { name: "Saving throws" })).toBeTruthy();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("opens the portraits gallery with one click on the profile portrait", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    renderApp("/sheet", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: "Open portraits" }));
    const dlg = await screen.findByRole("dialog", { name: "Portraits gallery" });
    expect(within(dlg).getByLabelText("Show all")).toBeTruthy();
    expect(within(dlg).getByLabelText("Male")).toBeTruthy();
    expect(within(dlg).getByLabelText("Female")).toBeTruthy();
    expect(within(dlg).getAllByRole("button", { name: /Select portrait|Browse|Cancel/ })).toHaveLength(3);
    expect(screen.queryByRole("heading", { name: "Manage" })).toBeNull(); // no page navigation: one click opens the gallery in place
  });

  it("closes the portraits gallery with Escape", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    renderApp("/sheet", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: "Open portraits" }));
    await screen.findByRole("dialog", { name: "Portraits gallery" });
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Portraits gallery" })).toBeNull());
  });
});

describe("compendium shortcut", () => {
  it("F1 opens the compendium and a second F1 closes it", async () => {
    renderApp("/characters", { api: new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 }) });
    await screen.findAllByRole("button");
    fireEvent.keyDown(window, { key: "F1" });
    expect(await screen.findByRole("dialog", { name: "Compendium" })).toBeTruthy();
    fireEvent.keyDown(window, { key: "F1" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Compendium" })).toBeNull());
  });
});
