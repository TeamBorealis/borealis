import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { renderApp } from "./utils";

// H10MQ (found making a weapon on the web mock): New element offers 37 kind cards, so once a kind far down (Weapon) is
// picked the Name box is scrolled away; and until its first save a new element was missing from its kind's count
// ("Weapons (0)") and its Edition column read "New". The Mastery help named one property though several are allowed.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };
const kinds = () => screen.getByRole("navigation", { name: "Element kinds" });
const elementRows = () => within(screen.getByRole("table", { name: "Elements" })).getAllByRole("row").slice(1);

async function openNewElement(user: ReturnType<typeof userEvent.setup>) {
  const buttons = await screen.findAllByRole("button", { name: /^New( element)?$/ });
  await user.click(buttons[0]!);
  return screen.findByRole("dialog", { name: "New element" });
}

describe("H10MQ New element", () => {
  it("picking a kind with no name yet brings the Name box back: it takes the focus", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    renderApp(`/homebrew/${pack.id}`, { api });
    const flyout = await openNewElement(user);
    const name = within(flyout).getByRole("textbox", { name: "Name" });
    const weapon = within(flyout).getByRole("radio", { name: /^Weapon A weapon/ });
    await user.click(weapon);
    expect(weapon.getAttribute("aria-checked")).toBe("true");
    expect(document.activeElement).toBe(name);
    // a name typed first stays where it is: the pick keeps the focus on the card
    await user.type(name, "Twinblade");
    const armor = within(flyout).getByRole("radio", { name: /^Armor Armor/ });
    await user.click(armor);
    expect(document.activeElement).toBe(armor);
    // arrow keys move through the cards as before
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement?.getAttribute("role")).toBe("radio");
  });

  it("an unsaved new element counts in its kind and shows its edition", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    renderApp(`/homebrew/${pack.id}`, { api });
    let flyout = await openNewElement(user);
    await user.click(within(flyout).getByRole("radio", { name: /^Weapon A weapon/ }));
    await user.type(within(flyout).getByRole("textbox", { name: "Name" }), "Twinblade");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    await waitFor(() => expect(within(kinds()).getByRole("button", { name: "Weapons (1)" })).toBeTruthy());
    expect(elementRows().map((r) => r.textContent)).toEqual([expect.stringMatching(/^Twinblade.*Pack/)]);
    expect(elementRows()[0]!.textContent).not.toMatch(/New/);

    // a second new weapon: the first is now a draft left in the list, both count, each with its edition
    await waitFor(() => expect(screen.getByText(/All changes kept/i)).toBeTruthy());
    flyout = await openNewElement(user);
    await user.click(within(flyout).getByRole("radio", { name: /^Weapon A weapon/ }));
    await user.type(within(flyout).getByRole("textbox", { name: "Name" }), "Hookblade");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe("Hookblade"));
    await user.selectOptions(screen.getByRole("combobox", { name: /^Edition/ }), "dnd2014");
    await waitFor(() => expect(within(kinds()).getByRole("button", { name: "Weapons (2)" })).toBeTruthy());
    await waitFor(() => expect(elementRows().map((r) => r.textContent?.replace(/\s+/g, " ").trim())).toEqual([
      expect.stringMatching(/^Hookblade.*2014/), expect.stringMatching(/^Twinblade.*Pack/),
    ]));
  });

  it("the weapon's Mastery help speaks of the properties it can hold", () => {
    const weapon = MOCK_KIND_DESCRIPTORS.find((d) => d.id === "weapon")!;
    const mastery = weapon.fields.find((f) => f.key === "mastery")!;
    expect(mastery.control).toBe("references");
    expect(mastery.help).toBe("The mastery properties a character with Weapon Mastery can use (2024).");
  });
});
