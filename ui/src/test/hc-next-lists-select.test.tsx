import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { renderApp } from "./utils";

// H0eao (spec 12.4, 16): SPELL LISTS has a Select mode, like the element list's, so rows are chosen for Add selected /
// Remove selected with a tap (phones have no Ctrl/Shift+click). In it a tap on a row adds it to the chosen rows or takes
// it out, and describes nothing; leaving the mode clears the choice.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };

describe("H0eao SPELL LISTS Select mode", () => {
  it("taps choose rows in Select mode; Add selected checks them; leaving the mode clears the choice", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "Spell Lists" }));
    const view = await screen.findByRole("region", { name: "Spell lists" });
    await user.selectOptions(within(view).getByRole("combobox", { name: "Spell list" }), "Wizard");
    const grid = await within(view).findByRole("table", { name: "Spells" });
    const cmds = within(view).getByRole("toolbar", { name: "Spell list commands" });
    const select = within(cmds).getByRole("button", { name: "Select" });
    const rowOf = (name: string) => within(grid).getByRole("button", { name }).closest("tr")!;

    // Act: Select mode, then a tap on each of two rows and a second tap that takes one out again
    await user.click(select);
    for (const name of ["Bless", "Guidance", "Cure Wounds", "Cure Wounds"]) await user.click(within(grid).getByRole("button", { name }));

    // Assert
    expect(select.getAttribute("aria-pressed")).toBe("true");
    expect(rowOf("Bless").getAttribute("aria-selected")).toBe("true");
    expect(rowOf("Guidance").getAttribute("aria-selected")).toBe("true");
    expect(rowOf("Cure Wounds").getAttribute("aria-selected")).toBe("false");
    expect(within(view).getByText("2 spells chosen")).toBeTruthy();
    expect(screen.queryByRole("region", { name: "Where used" })).toBeNull(); // a tap chooses, it describes nothing
    await user.click(within(cmds).getByRole("button", { name: "Add selected" }));
    await waitFor(() => expect(within(view).getByRole("status").textContent).toContain("(2 not saved)"));
    expect((within(grid).getByRole("checkbox", { name: "Choose Bless" }) as HTMLInputElement).checked).toBe(true);

    await user.click(select);
    expect(select.getAttribute("aria-pressed")).toBe("false");
    expect(rowOf("Bless").getAttribute("aria-selected")).toBe("false");
    expect(within(cmds).getByRole("button", { name: "Add selected" }).hasAttribute("disabled")).toBe(true);
    await user.click(within(grid).getByRole("button", { name: "Bless" }));
    expect(await screen.findByRole("region", { name: "Where used" })).toBeTruthy(); // out of the mode a tap describes again
  });
});
