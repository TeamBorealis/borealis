import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { FeaturesCard } from "../pages/play/TraitsPanel";
import { renderApp, seededId } from "./utils";

/**
 * B3 (GitHub #35): Feats, Languages, Proficiencies (Build) and each class's spells (Magic) get an Add action for the
 * extras a DM grants. Only options the character qualifies for are offered unless "Ignore feat prerequisites" is on;
 * every added pick carries the "Added" tag and can be removed.
 */

const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
const MARK = /prerequisites not met/i;

afterEach(() => setPhoneForTests(false));

async function openTab(route: string, name = "Seraphine") {
  const api = fast();
  const id = await seededId(api, name);
  renderApp(route, { api, openCharacterId: id });
  return { api, id };
}

describe.each([["desktop", false], ["phone", true]] as const)("added feats (%s)", (_layout, phone) => {
  it("offers qualifying feats, adds one with the Added tag and removes it again", async () => {
    setPhoneForTests(phone);
    const user = userEvent.setup();
    const { api, id } = await openTab("/builder/feats");
    const section = await screen.findByRole("region", { name: "Added feats" });
    expect(within(section).getByText(/Nothing added yet/)).toBeTruthy();
    await user.click(within(section).getByRole("button", { name: "Add Feat" }));
    const dialog = await screen.findByRole("dialog", { name: "Add Feat" });
    const list = await within(dialog).findByRole("list", { name: "Feat options" });
    await within(list).findByRole("button", { name: /Alert/ });
    expect(within(list).queryByRole("button", { name: /Boon of Fate/ })).toBeNull();
    expect(within(list).queryByText(MARK)).toBeNull();
    await user.click(within(list).getByRole("button", { name: /Alert/ }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Add Feat" })).toBeNull());
    const added = await within(section).findByRole("list", { name: "Added feats list" });
    const line = within(added).getByRole("listitem");
    expect(within(line).getByText("Alert")).toBeTruthy();
    expect(within(line).getByText("Added")).toBeTruthy();
    const snap = await api.getSnapshot({ characterId: id });
    expect(snap.addedPicks.map((p) => [p.kind, p.name])).toEqual([["feat", "Alert"]]);
    expect(snap.inventory.items.some((i) => i.name.includes("Alert"))).toBe(false);
    await user.click(within(line).getByRole("button", { name: "Remove Alert" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).addedPicks).toEqual([]));
    expect(await within(section).findByText(/Nothing added yet/)).toBeTruthy();
  });
});

describe("Ignore feat prerequisites", () => {
  it("offers every feat, marks the ones whose prerequisites fail, and marks the added pick", async () => {
    const user = userEvent.setup();
    const api = fast();
    const id = await seededId(api, "Seraphine");
    await api.setCharacterOptions({ characterId: id, options: { ignoreFeatPrerequisites: true } });
    renderApp("/builder/feats", { api, openCharacterId: id });
    const section = await screen.findByRole("region", { name: "Added feats" });
    await user.click(within(section).getByRole("button", { name: "Add Feat" }));
    const dialog = await screen.findByRole("dialog", { name: "Add Feat" });
    const boon = await within(dialog).findByRole("button", { name: /Boon of Fate/ });
    expect(within(boon).getByText(MARK)).toBeTruthy();
    expect(within(boon).getByText(/Prerequisite: Level 19\+/)).toBeTruthy();
    expect(within(within(dialog).getByRole("button", { name: /Alert/ })).queryByText(MARK)).toBeNull();
    await user.click(boon);
    const line = within(await within(section).findByRole("list", { name: "Added feats list" })).getByRole("listitem");
    expect(within(line).getByText("Boon of Fate")).toBeTruthy();
    expect(within(line).getByText("Added")).toBeTruthy();
    expect(within(line).getByText(MARK)).toBeTruthy();
    // the house rule off: kept, not applied, and the line says so
    await api.setCharacterOptions({ characterId: id, options: { ignoreFeatPrerequisites: false } });
    const snap = await api.getSnapshot({ characterId: id });
    expect(snap.addedPicks[0]).toMatchObject({ name: "Boon of Fate", applied: false, prerequisitesUnmet: false });
  });
});

describe("added languages and proficiencies", () => {
  it.each([
    ["/builder/languages", "Added languages", "Add Language", "Language options", /Elvish/, "Elvish"],
    ["/builder/proficiencies", "Added proficiencies", "Add Proficiency", "Proficiency options", /Stealth/, "Stealth"],
  ])("%s has an Add action", async (route, region, button, listName, row, name) => {
    const user = userEvent.setup();
    const { api, id } = await openTab(route);
    const section = await screen.findByRole("region", { name: region });
    await user.click(within(section).getByRole("button", { name: button }));
    const dialog = await screen.findByRole("dialog", { name: button });
    const list = await within(dialog).findByRole("list", { name: listName });
    await user.type(within(dialog).getByRole("searchbox"), name);
    await user.click(await within(list).findByRole("button", { name: row }));
    const line = within(await within(section).findByRole("list", { name: `${region} list` })).getByRole("listitem");
    expect(within(line).getByText(name)).toBeTruthy();
    expect(within(line).getByText("Added")).toBeTruthy();
    expect((await api.getSnapshot({ characterId: id })).addedPicks).toHaveLength(1);
  });

  it("does not offer what the character already has", async () => {
    const user = userEvent.setup();
    await openTab("/builder/languages");
    const section = await screen.findByRole("region", { name: "Added languages" });
    await user.click(within(section).getByRole("button", { name: "Add Language" }));
    const list = await within(await screen.findByRole("dialog", { name: "Add Language" })).findByRole("list", { name: "Language options" });
    await within(list).findByRole("button", { name: /Elvish/ });
    expect(within(list).queryByRole("button", { name: /Giant/ })).toBeNull();
  });
});

describe("added spells", () => {
  it("adds a spell of the class's list on its Magic tab and tags it in the class's spells", async () => {
    const user = userEvent.setup();
    const { api, id } = await openTab("/spells");
    await user.click(await screen.findByRole("tab", { name: /Wizard/i }));
    const section = await screen.findByRole("region", { name: "Added spells" });
    await user.click(within(section).getByRole("button", { name: "Add Wizard Spell" }));
    const dialog = await screen.findByRole("dialog", { name: "Add Wizard Spell" });
    const list = await within(dialog).findByRole("list", { name: "Spell options" });
    await within(list).findByRole("button", { name: /Burning Hands/ });
    expect(within(list).queryByRole("button", { name: /Cure Wounds/ })).toBeNull();
    expect(within(list).queryByRole("button", { name: /Magic Missile/ })).toBeNull();
    await user.click(within(list).getByRole("button", { name: /Burning Hands/ }));
    const line = within(await within(section).findByRole("list", { name: "Added spells list" })).getByRole("listitem");
    expect(within(line).getByText("Burning Hands")).toBeTruthy();
    const grid = screen.getByRole("table", { name: "Wizard spells" });
    const row = within(grid).getByRole("button", { name: "Burning Hands" }).closest("tr")!;
    expect(within(row).getByText("Added")).toBeTruthy();
    const snap = await api.getSnapshot({ characterId: id });
    expect(snap.addedPicks).toMatchObject([{ kind: "spell", name: "Burning Hands", spellcasting: "Wizard" }]);
  });
});

describe("the Added tag on the Play tab", () => {
  it("marks an added feat in the features list", () => {
    const f = { id: "alert", name: "Alert", ty: "Feat", source: "SRD", level: 1, description: "", displayed: true };
    render(<FeaturesCard features={[{ ...f, added: true }, { ...f, id: "tough", name: "Tough" }]} selectedId={null} onSelect={() => {}} />);
    expect(within(screen.getByRole("button", { name: /Alert/ })).getByText("Added")).toBeTruthy();
    expect(within(screen.getByRole("button", { name: /Tough/ })).queryByText("Added")).toBeNull();
  });
});
