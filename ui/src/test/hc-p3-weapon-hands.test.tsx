import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewForm } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { weaponSlotFor } from "../pages/homebrew/weaponHands";
import { renderApp } from "./utils";

// Homebrew P3 bug (1.5.0, HC11): a new weapon starts held in one hand (P2b) and the engine's two-handed fact reads only
// the slot, so a weapon given the Two-Handed property but left on One hand was wielded one-handed. Picking Two-Handed
// now moves the weapon to Two hands, and taking it off moves it back, in the same undo step.

const TWO = { id: "ID_INTERNAL_WEAPON_PROPERTY_TWOHANDED", name: "Two-Handed", type: "Weapon Property" };
const TWO_2024 = { id: "ID_2024_WEAPON_PROPERTY_TWO_HANDED", name: "Two-Handed", type: "Weapon Property" };
const FINESSE = { id: "ID_INTERNAL_WEAPON_PROPERTY_FINESSE", name: "Finesse", type: "Weapon Property" };
const editor = () => screen.getByRole("region", { name: "Element editor" });

describe("P3: a Two-Handed weapon is held in two hands", () => {
  it("the slot follows the Two-Handed property, and only from the hand slots", () => {
    expect(weaponSlotFor([], [TWO], "onehand")).toBe("twohand");
    expect(weaponSlotFor([FINESSE], [FINESSE, TWO_2024], "onehand,secondary")).toBe("twohand");
    expect(weaponSlotFor([], [TWO], "")).toBe("twohand");
    expect(weaponSlotFor([TWO], [], "twohand")).toBe("onehand");
    // other changes leave the slot as the author chose it
    expect(weaponSlotFor([], [FINESSE], "onehand")).toBe("onehand");
    expect(weaponSlotFor([TWO], [TWO, FINESSE], "onehand")).toBe("onehand");
    expect(weaponSlotFor([], [TWO], "misc")).toBe("misc");
    expect(weaponSlotFor([TWO], [], "secondary")).toBe("secondary");
  });

  it("a new weapon given Two-Handed is saved held in two hands; Undo takes both back", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
    const update = vi.spyOn(api, "homebrewUpdateDraft");
    const last = (): HomebrewForm => update.mock.lastCall![0].form;
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "New element" }));
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    await user.click(within(flyout).getByRole("radio", { name: /^Weapon A weapon/ }));
    await user.type(within(flyout).getByLabelText("Name"), "Ember Maul");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Ember Maul"));

    // Act
    await user.click(within(editor()).getByRole("button", { name: "Pick: Properties" }));
    const picker = await screen.findByRole("dialog", { name: /Choose/ });
    await user.dblClick(await within(picker).findByText("Two-Handed"));
    const ok = within(picker).queryByRole("button", { name: "OK" });
    if (ok) await user.click(ok);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /Choose/ })).toBeNull());

    // Assert
    await waitFor(() => expect(last().fields.slot).toBe("twohand"));
    expect(last().fields.properties).toEqual([expect.objectContaining({ name: "Two-Handed" })]);
    expect((within(editor()).getByRole("combobox", { name: /^Worn or held on/ }) as HTMLSelectElement).value).toBe("twohand");
    await user.keyboard("{Control>}z{/Control}");
    await waitFor(() => expect(last().fields.slot).toBe("onehand"));
    expect(last().fields.properties ?? []).toEqual([]);
  });
});
