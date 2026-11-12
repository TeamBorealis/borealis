import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { renderApp } from "./utils";

// IDpnY (WB-27b, the real macOS window): WebKit's macOS tree names a text box from `<label htmlFor>` only through a
// title element, so a creator Name box read as a nameless text box with a placeholder, and a select with no value yet
// (a new spell's School) was left out of the tree. Like the New character Name box, which the tree names, each control
// also points at its visible label with `aria-labelledby`.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };

/** The text of the elements `aria-labelledby` names, or null without it. */
function labelledBy(el: HTMLElement): string | null {
  const ids = el.getAttribute("aria-labelledby");
  if (!ids) return null;
  return ids.split(/\s+/).map((id) => document.getElementById(id)?.textContent?.trim() ?? "").join(" ");
}

describe("IDpnY creator controls name themselves by their label", () => {
  it("New pack's and New element's Name boxes, and a new spell's School select with no value", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    renderApp("/homebrew", { api });
    await user.click(await screen.findByRole("button", { name: "New pack" }));
    const packFlyout = await screen.findByRole("dialog", { name: "New pack" });
    const packName = within(packFlyout).getByRole("textbox", { name: "Name" });
    expect(labelledBy(packName)).toBe("Name");
    for (const name of ["Code", "Author", "Description"]) expect(labelledBy(within(packFlyout).getByRole("textbox", { name }))).toBe(name);
    await user.type(packName, PACK.name);
    await user.click(within(packFlyout).getByRole("button", { name: "Create" }));

    await user.click(await screen.findByRole("button", { name: "New element" }));
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    const elementName = within(flyout).getByRole("textbox", { name: "Name" });
    expect(labelledBy(elementName)).toBe("Name");
    await user.click(within(flyout).getByRole("radio", { name: /^Spell A spell/ }));
    await user.type(elementName, "Ember Ward");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));

    const editor = await screen.findByRole("region", { name: "Element editor" });
    const school = await within(editor).findByRole("combobox", { name: /^School/ }) as HTMLSelectElement;
    expect(school.value).toBe("");
    expect(labelledBy(school)).toBe("School *");
    await waitFor(() => expect(labelledBy(within(editor).getByRole("textbox", { name: /^Name/ }))).toBe("Name *"));
  });
});
