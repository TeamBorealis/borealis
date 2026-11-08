import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { GUIDES } from "../components/GuideLink";
import { EXTERNAL_LINKS } from "../lib/links";
import { renderApp, seededId } from "./utils";

// B-234 (decision U18): each section the website guides explain has a small help button. Its tooltip names the guide,
// and a click or tap opens the guide in the system browser through open_link (allowlisted, U15).

const URL = {
  create: "https://borealisbuilder.com/guides/create-a-character",
  aurora: "https://borealisbuilder.com/guides/import-aurora-content",
  fiveTools: "https://borealisbuilder.com/guides/import-5etools-data",
  move: "https://borealisbuilder.com/guides/move-characters-between-devices",
  print: "https://borealisbuilder.com/guides/print-character-sheet",
} as const;

afterEach(() => { vi.restoreAllMocks(); });

/** The guide button inside `scope`: named by its aria-label, with the same text in its tooltip. */
function guideButton(scope: HTMLElement, label: string): HTMLElement {
  const button = within(scope).getByRole("button", { name: label });
  const tip = button.closest(".aur-tip")?.querySelector('[role="tooltip"]');
  expect(tip?.textContent).toBe(label);
  return button;
}

async function expectOpens(user: ReturnType<typeof userEvent.setup>, api: MockApi, button: HTMLElement, url: string) {
  const open = vi.spyOn(api, "openLink");
  await user.click(button);
  await waitFor(() => expect(open).toHaveBeenCalledWith({ url }));
  expect(open).toHaveBeenCalledTimes(1);
  expect(api.openedLinks).toEqual([url]);
}

async function openFile(user: ReturnType<typeof userEvent.setup>, entry: string) {
  await user.click(await screen.findByRole("menuitem", { name: "File" }));
  await user.click(await screen.findByRole("menuitem", { name: entry }));
}

describe("U18 guide buttons", () => {
  it("the five guides are on the open_link list with their exact addresses", () => {
    expect(Object.values(GUIDES).map((g) => g.url)).toEqual([URL.create, URL.aurora, URL.fiveTools, URL.move, URL.print]);
    for (const url of Object.values(URL)) expect(EXTERNAL_LINKS).toContain(url);
  });

  it("New Character: the flyout title box has the create-a-character guide, and the name field still gets first focus", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters?new=1");
    const dialog = await screen.findByRole("dialog", { name: "New character" });
    const button = guideButton(dialog, "Guide: Create and level up a character");
    expect(dialog.querySelector(".flyout-head")?.contains(button)).toBe(true);
    await waitFor(() => expect(document.activeElement).toBe(within(dialog).getByTestId("new-character-name")));
    await expectOpens(user, api, button, URL.create);
  });

  it("Additional Content: Custom content has the Aurora import guide", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/content");
    const section = await screen.findByRole("region", { name: "Custom content" });
    expect(within(section).getByRole("heading", { name: "Custom content" })).toBeTruthy();
    await expectOpens(user, api, guideButton(section, "Guide: Import Aurora content"), URL.aurora);
  });

  it("Additional Content: 5etools-format import has the 5etools guide", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/content");
    const section = await screen.findByRole("region", { name: "5etools-format import" });
    await expectOpens(user, api, guideButton(section, "Guide: Import 5etools-format data"), URL.fiveTools);
  });

  it.each([["Send to device...", "Send to device"], ["Receive from device...", "Receive from device"], ["Export to bundle...", "Export to bundle"]])(
    "%s: the dialog title bar has the move-characters guide, and it does not take first focus",
    async (entry, title) => {
      const user = userEvent.setup();
      const { api } = renderApp("/characters");
      await openFile(user, entry);
      const dialog = await screen.findByRole("dialog", { name: title });
      const button = guideButton(dialog, "Guide: Move characters between devices");
      expect(dialog.querySelector(".dialog-head")?.contains(button)).toBe(true);
      expect(document.activeElement).not.toBe(button);
      await expectOpens(user, api, button, URL.move);
    },
  );

  it("Character sheet: Generate preview has the print guide beside Save Character Sheet", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    renderApp("/sheet", { api, openCharacterId: id });
    expect(await screen.findByRole("heading", { name: "Generate preview" })).toBeTruthy();
    await screen.findByRole("button", { name: "Save Character Sheet" });
    await expectOpens(user, api, guideButton(document.body, "Guide: Print or save a character sheet"), URL.print);
  });

  it.each([["a store build", { storeBuild: true }], ["the Mac App Store build", { macAppStore: true }], ["a phone", { mobile: true, storeBuild: true }]])(
    "the guides show and open in %s, like Privacy Policy", async (_, opts) => {
      const user = userEvent.setup();
      const api = new MockApi({ aiChunkDelayMs: 2, ...opts });
      renderApp("/content", { api });
      const section = await screen.findByRole("region", { name: "Custom content" });
      await expectOpens(user, api, guideButton(section, "Guide: Import Aurora content"), URL.aurora);
    },
  );
});
