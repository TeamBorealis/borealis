import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { MENUS, START_SUBNAV, startSubnav, visibleMenus } from "../components/shell/sections";
import { ROUTES } from "../routes";
import { creatorOn } from "../state/platform";
import { renderApp } from "./utils";

// Homebrew WB-09, HC3 (spec section 2): the creator renders only while `creatorEnabled` is on. Off (the 1.4.x default),
// nothing creator-related renders anywhere: no START > HOMEBREW sub-tab, no File menu entry, no More page row, no
// "Edit in Homebrew" pack action, and the creator's routes go back to the characters.

const mock = (creatorEnabled: boolean, mobile = false) => new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, creatorEnabled, mobile });

async function withHomebrewPack(api: MockApi): Promise<void> {
  await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
}

async function fileMenuItems(user: ReturnType<typeof userEvent.setup>): Promise<string[]> {
  await user.click(await screen.findByRole("menuitem", { name: "File" }));
  return within(await screen.findByRole("menu")).getAllByRole("menuitem").map((i) => i.textContent ?? "");
}

describe("WB-09 feature switch: off", () => {
  it("START has no Homebrew sub-tab and the File menu no creator entry", async () => {
    // Arrange
    const user = userEvent.setup();
    renderApp("/characters", { api: mock(false) });
    const subnav = await screen.findByRole("navigation", { name: "Start sections" });

    // Act
    const items = await fileMenuItems(user);

    // Assert
    expect(within(subnav).queryByRole("link", { name: "Homebrew" })).toBeNull();
    expect(within(subnav).getAllByRole("link").map((l) => l.textContent)).toEqual(["Character Collection", "Sources", "Additional Content", "Notes"]);
    expect(items.some((t) => /homebrew/i.test(t))).toBe(false);
    expect(document.body.textContent).not.toMatch(/homebrew/i);
  });

  it.each(["/homebrew", "/homebrew/homebrew-emb", "/homebrew?new=1"])("%s goes back to the characters", async (route) => {
    // Arrange
    const api = mock(false);
    await withHomebrewPack(api);

    // Act
    renderApp(route, { api });

    // Assert
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeTruthy();
    expect(screen.queryByRole("grid", { name: "Homebrew packs" })).toBeNull();
    expect(screen.queryByRole("region", { name: "Element editor" })).toBeNull();
  });

  it("Additional Content shows no Edit in Homebrew on a homebrew pack", async () => {
    // Arrange
    const api = mock(false);
    await withHomebrewPack(api);

    // Act
    renderApp("/content", { api });
    const packs = await screen.findByRole("list", { name: "Content packs" });
    await within(packs).findByText("Ember Isles");

    // Assert
    expect(within(packs).queryByRole("button", { name: "Edit in Homebrew" })).toBeNull();
  });

  it("the phone More page lists no Homebrew row", async () => {
    // Arrange
    renderApp("/more", { api: mock(false, true) });

    // Act
    const list = await screen.findByRole("list", { name: "More destinations" });

    // Assert
    expect(within(list).queryByRole("link", { name: "Homebrew" })).toBeNull();
    expect(within(list).getByRole("link", { name: "Notes" })).toBeTruthy();
  });

  it("the shell's tables gate every creator entry", () => {
    // Arrange
    const creatorSubtabs = START_SUBNAV.filter((s) => s.creator).map((s) => s.to);
    const creatorMenu = MENUS.flatMap((m) => m.entries).filter((e) => e.creator).map((e) => e.id);

    // Act
    const offTabs = startSubnav(false, false).map((s) => s.to);
    const offMenu = visibleMenus(false, false, false, false).flatMap((m) => m.entries).map((e) => e.id);

    // Assert
    expect(creatorSubtabs).toEqual(["/homebrew"]);
    expect(creatorMenu).toEqual(["new-homebrew"]);
    expect(offTabs).not.toContain("/homebrew");
    expect(offMenu).not.toContain("new-homebrew");
    expect(ROUTES.filter((r) => r.creator).map((r) => r.path)).toEqual(["/homebrew"]);
  });

  it("the switch: the setting, or a development build of the app", () => {
    expect(creatorOn(false, false)).toBe(false);
    expect(creatorOn(true, false)).toBe(true);
    expect(creatorOn(false, true)).toBe(true);
    // vitest is not a Tauri development build: the default follows the setting.
    expect(creatorOn(false)).toBe(false);
  });
});

describe("WB-09 feature switch: on (the same places, as the control)", () => {
  it("START shows HOMEBREW after Additional Content, and File > New homebrew element...", async () => {
    // Arrange
    const user = userEvent.setup();
    renderApp("/characters", { api: mock(true) });
    const subnav = await screen.findByRole("navigation", { name: "Start sections" });

    // Act
    await within(subnav).findByRole("link", { name: "Homebrew" });
    const items = await fileMenuItems(user);

    // Assert
    expect(within(subnav).getAllByRole("link").map((l) => l.textContent)).toEqual(["Character Collection", "Sources", "Additional Content", "Homebrew", "Notes"]);
    expect(items).toContain("New homebrew element...");
  });

  it("Additional Content offers Edit in Homebrew on a homebrew pack, and it opens the workspace", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock(true);
    await withHomebrewPack(api);
    renderApp("/content", { api });
    const packs = await screen.findByRole("list", { name: "Content packs" });

    // Act
    await user.click(await within(packs).findByRole("button", { name: "Edit in Homebrew" }));

    // Assert
    expect(await screen.findByRole("heading", { name: "Homebrew: Ember Isles" })).toBeTruthy();
  });

  it("the phone More page lists Homebrew", async () => {
    renderApp("/more", { api: mock(true, true) });
    const list = await screen.findByRole("list", { name: "More destinations" });
    await waitFor(() => expect(within(list).getByRole("link", { name: "Homebrew" })).toBeTruthy());
  });
});
