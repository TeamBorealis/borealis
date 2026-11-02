import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { renderApp, seededId } from "./utils";

// GitHub #13 (decision U16(c)): several companions on Manage > Companion. Brannoc (`sampleCompanions`) has a granted Mastiff
// named Grip and two added ones, Hoot (an Owl) and Ash (a Wolf). Every flow runs on the desktop and the phone layout.

const LAYOUTS = [["desktop", false], ["phone", true]] as const;

afterEach(() => { setPhoneForTests(false); setPhoneOsForTests(null); });

async function openTab(phone: boolean, sampleCompanions = true) {
  if (phone) { setPhoneForTests(true); setPhoneOsForTests("ios"); }
  const api = new MockApi({ aiChunkDelayMs: 1, sampleCompanions });
  const characterId = await seededId(api, "Brannoc");
  renderApp("/manage", { api, openCharacterId: characterId });
  const user = userEvent.setup();
  await user.click(await screen.findByRole("tab", { name: "Companion" }));
  return { api, characterId, user };
}

const list = () => screen.getByRole("list", { name: "Companions" });
const names = () => within(list()).getAllByRole("heading", { level: 3 }).map((h) => h.textContent);
const nameField = () => screen.getByLabelText("Companion Name") as HTMLInputElement;
const card = (name: string) => within(list()).getByRole("heading", { level: 3, name }).closest("li")!;

describe.each(LAYOUTS)("several companions (%s)", (_layout, phone) => {
  it("lists the granted companion first, then the added ones, and switches between them", async () => {
    const { user } = await openTab(phone);
    await waitFor(() => expect(names()).toEqual(["Grip", "Hoot", "Ash"]));
    expect(within(card("Grip")).getByText("From a feature")).toBeTruthy();
    expect(within(card("Grip")).queryByRole("button", { name: /Remove/ })).toBeNull();
    expect(nameField().value).toBe("Grip");
    expect(screen.getByText("Chosen on the builder's Companion tab.")).toBeTruthy();
    await user.click(within(card("Hoot")).getByText("Owl · Tiny beast, unaligned"));
    expect(card("Hoot").getAttribute("aria-current")).toBe("true");
    await waitFor(() => expect(nameField().value).toBe("Hoot"));
    const traits = screen.getByRole("list", { name: "Traits" });
    expect(within(traits).getByText("Night Watch")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Choose Creature" })).toBeTruthy();
  });

  it("adds a companion from a creature, selects it and renames it", async () => {
    const { api, characterId, user } = await openTab(phone);
    await user.click(await screen.findByRole("button", { name: "Add Companion" }));
    const dialog = await screen.findByRole("dialog", { name: "Add Companion" });
    await user.type(within(dialog).getByRole("searchbox", { name: "Search creatures" }), "wolf");
    await user.click(await within(dialog).findByRole("button", { name: /^Wolf/ }));
    await waitFor(() => expect(names()).toEqual(["Grip", "Hoot", "Ash", "Wolf"]));
    expect(nameField().value).toBe("Wolf");
    await user.clear(nameField());
    await user.type(nameField(), "Smoke");
    await user.tab();
    await waitFor(() => expect(names()).toEqual(["Grip", "Hoot", "Ash", "Smoke"]));
    const snap = await api.getSnapshot({ characterId });
    expect(snap.companions.map((c) => [c.id, c.creature?.name ?? null])).toEqual([["granted", "Mastiff"], ["companion-1", "Owl"], ["companion-2", "Wolf"], ["companion-3", "Wolf"]]);
  });

  it("adds, edits and removes the selected companion's traits", async () => {
    const { api, characterId, user } = await openTab(phone);
    await user.click(await screen.findByRole("button", { name: "Add Trait" }));
    let dialog = await screen.findByRole("dialog", { name: "New trait" });
    expect((within(dialog).getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);
    await user.type(within(dialog).getByLabelText("Name"), "Guard Dog");
    await user.type(within(dialog).getByLabelText("Text"), "Grip growls at strangers.");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    const traits = await screen.findByRole("list", { name: "Traits" });
    expect(await within(traits).findByText("Guard Dog")).toBeTruthy();
    await user.click(within(traits).getByRole("button", { name: "Edit Guard Dog" }));
    dialog = await screen.findByRole("dialog", { name: "Edit Guard Dog" });
    const name = within(dialog).getByLabelText("Name");
    await user.clear(name);
    await user.type(name, "Watch Dog");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await within(traits).findByText("Watch Dog")).toBeTruthy();
    let snap = await api.getSnapshot({ characterId });
    expect(snap.companions[0]!.customTraits).toEqual([{ name: "Watch Dog", text: "Grip growls at strangers." }]);
    await user.click(within(traits).getByRole("button", { name: "Remove Watch Dog" }));
    await waitFor(() => expect(screen.queryByRole("list", { name: "Traits" })).toBeNull());
    snap = await api.getSnapshot({ characterId });
    expect(snap.companions[0]!.customTraits).toEqual([]);
  });

  it("sets bonuses and notes that the statistics include", async () => {
    const { api, characterId, user } = await openTab(phone);
    await user.click(within(card("Ash")).getByRole("heading", { level: 3, name: "Ash" }));
    await waitFor(() => expect(nameField().value).toBe("Ash"));
    await user.click(screen.getByRole("button", { name: "Increase Armor Class Bonus" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId })).companions[2]!.armorClass).toBe(15));
    const notes = screen.getByLabelText("Notes");
    await user.type(notes, "Limps on cold days.");
    await user.tab();
    await waitFor(async () => expect((await api.getSnapshot({ characterId })).companions[2]!.notes).toBe("Limps on cold days."));
  });

  it("removes an added companion after a confirmation", async () => {
    const { user } = await openTab(phone);
    await user.click(within(card("Hoot")).getByRole("button", { name: "Remove Hoot" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove Hoot?" });
    await user.click(within(dialog).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(names()).toEqual(["Grip", "Ash"]));
  });
});

describe("one companion", () => {
  it("keeps the classic tab without a list, with an Add Companion button", async () => {
    const api = new MockApi({ aiChunkDelayMs: 1 });
    const characterId = await seededId(api, "Brannoc");
    await api.loadCharacter({ characterId });
    await api.updateCompanion({ characterId, patch: { name: "Biscuit" } });
    await api.saveCharacter({ characterId });
    renderApp("/manage", { api, openCharacterId: characterId });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("tab", { name: "Companion" }));
    expect(nameField().value).toBe("Biscuit");
    expect(screen.queryByRole("list", { name: "Companions" })).toBeNull();
    expect(screen.getByRole("button", { name: "Add Companion" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Companions Gallery" })).toBeTruthy();
  });
});
