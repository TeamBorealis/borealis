import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { mockApi } from "./hc-wb10-harness";
import { renderApp } from "./utils";

// WB-28 in-app help text. New element (board item H-aK8, decision HC10 Q5): a one-line description under KIND and
// START FROM, in the creator's field help style, read by screen readers with the radio group it explains.

async function openNewElement() {
  const user = userEvent.setup();
  const api = mockApi();
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  renderApp(`/homebrew/${pack.id}`, { api });
  await user.click((await screen.findAllByRole("button", { name: /^New( element)?$/ }))[0]!);
  return screen.findByRole("dialog", { name: "New element" });
}

describe("New element: KIND and START FROM explain themselves (H-aK8)", () => {
  it.each([
    ["Kind", /^What to make\./],
    ["Start from", /^Blank is empty;/],
  ])("%s has a help line that describes its choices", async (group, text) => {
    // Arrange
    const flyout = await openNewElement();

    // Act
    const radios = within(flyout).getByRole("radiogroup", { name: group });
    const helpId = radios.getAttribute("aria-describedby") ?? "";
    const help = helpId ? document.getElementById(helpId) : null;

    // Assert: one plain line in the field help style, above the cards
    expect(help?.textContent).toMatch(text);
    expect(help?.className).toBe("hb-field-help");
    expect(help!.compareDocumentPosition(radios) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});

describe("field help is read with its field (WB-28)", () => {
  it("a field's help line describes its control, until a problem takes its place", async () => {
    // Arrange: a new spell in a pack
    const user = userEvent.setup();
    const flyout = await openNewElement();
    await user.type(within(flyout).getByLabelText("Name"), "Ember Bolt");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    const shown = await screen.findByRole("textbox", { name: /^Prerequisite as shown/ });

    // Act
    const ids = (shown.getAttribute("aria-describedby") ?? "").split(" ").filter(Boolean);

    // Assert
    expect(ids.map((id) => document.getElementById(id)?.textContent ?? "")).toEqual([
      "Written from the prerequisite while left empty or as written; type to reword it.",
    ]);
  });
});
