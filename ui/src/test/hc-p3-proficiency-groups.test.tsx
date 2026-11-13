import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewForm, HomebrewPickerRow } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { proficiencyGroupOf, withClassProficiencies } from "../pages/homebrew/proficiencyGroups";
import { renderApp } from "./utils";

// Homebrew P3 Hpbig (1.5.0, HC11): a class lists its proficiencies by kind (the backend reads armor and weapon
// proficiencies by their ids), so a tool picked under Armor training came back under Tool and other proficiencies on
// reopen. It now moves there at once, in the same undo step (the field's help and the status line say why), so reopening
// reshuffles nothing.

const TOOLS = { id: "ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS", name: "Thieves' Tools", type: "Proficiency" };
const SHIELD = { id: "ID_PROFICIENCY_ARMOR_PROFICIENCY_SHIELD", name: "Shield", type: "Proficiency" };
const DAGGER = { id: "ID_PROFICIENCY_WEAPON_PROFICIENCY_DAGGER", name: "Dagger", type: "Proficiency" };
const STR_SAVE = { id: "ID_PROFICIENCY_SAVINGTHROW_STRENGTH", name: "Saving Throw (Strength)", type: "Proficiency" };
const editor = () => screen.getByRole("region", { name: "Element editor" });

const row = (r: typeof TOOLS): HomebrewPickerRow => ({
  id: r.id, name: r.name, type: r.type, summary: "", prerequisites: null, sourceName: "Core", edition: null, draft: false,
});

describe("P3 Hpbig: class proficiency groups", () => {
  it("groups a proficiency as the backend reads it", () => {
    expect(proficiencyGroupOf(SHIELD.id)).toBe("armor");
    expect(proficiencyGroupOf(DAGGER.id)).toBe("weapons");
    expect(proficiencyGroupOf(TOOLS.id)).toBe("tools");
    expect(proficiencyGroupOf("ID_EMB_PROFICIENCY_EMBER_PLATE")).toBe("tools");
    expect(proficiencyGroupOf(STR_SAVE.id)).toBe("savingThrows");
  });

  it("moves what a group does not hold to its own group", () => {
    const { fields, moved } = withClassProficiencies({ armor: [SHIELD], tools: [], savingThrows: ["Dexterity"] }, "armor", [SHIELD, TOOLS, DAGGER, STR_SAVE]);
    expect(fields.armor).toEqual([SHIELD]);
    expect(fields.weapons).toEqual([DAGGER]);
    expect(fields.tools).toEqual([TOOLS]);
    expect(fields.savingThrows).toEqual(["Dexterity", "Strength"]);
    expect(moved).toEqual([
      { name: "Thieves' Tools", to: "tools" }, { name: "Dagger", to: "weapons" }, { name: "Saving Throw (Strength)", to: "savingThrows" },
    ]);
    // nothing out of place: the fields as given
    expect(withClassProficiencies({ armor: [] }, "armor", [SHIELD])).toEqual({ fields: { armor: [SHIELD] }, moved: [] });
  });

  it("a tool picked under Armor training goes to Tool and other proficiencies at once, and its help says why", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
    const options = api.homebrewReferenceOptions.bind(api);
    vi.spyOn(api, "homebrewReferenceOptions").mockImplementation(async (args) => (args.target.types.includes("Proficiency")
      ? { total: 1, items: [row(TOOLS)] } : options(args)));
    const update = vi.spyOn(api, "homebrewUpdateDraft");
    const last = (): HomebrewForm => update.mock.lastCall![0].form;
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "New element" }));
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    await user.click(within(flyout).getByRole("radio", { name: /^Class A class/ }));
    await user.type(within(flyout).getByLabelText("Name"), "Gloom Knight");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Gloom Knight"));
    await user.click(within(editor()).getByRole("tab", { name: "PROFICIENCIES" }));

    // Act
    await user.click(within(editor()).getByRole("button", { name: "Pick: Armor training" }));
    const picker = await screen.findByRole("dialog", { name: /Choose/ });
    await user.dblClick(await within(picker).findByText("Thieves' Tools"));
    const ok = within(picker).queryByRole("button", { name: "OK" });
    if (ok) await user.click(ok);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /Choose/ })).toBeNull());

    // Assert
    await waitFor(() => expect(last().fields.tools).toEqual([expect.objectContaining({ id: TOOLS.id })]));
    expect(last().fields.armor ?? []).toEqual([]);
    expect(within(editor()).getAllByText("Armor and shield training. A proficiency of another kind picked here moves to its own list.")).toHaveLength(1);
    await user.keyboard("{Control>}z{/Control}");
    await waitFor(() => expect(last().fields.tools ?? []).toEqual([]));
  });
});
