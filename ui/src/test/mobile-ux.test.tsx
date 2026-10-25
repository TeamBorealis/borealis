import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { buildLine } from "../pages/characters/CharacterTile";
import { renderApp, seededId } from "./utils";

const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });

describe("accessibility and testids (fix-mobile-ux)", () => {
  it("buildLine never repeats Level for classless characters and keeps multiclass names", () => {
    expect(buildLine({ build: "Level 1", level: 1 })).toBe("Level 1");
    expect(buildLine({ build: "Elf Level 1", level: 1 })).toBe("Level 1 Elf");
    expect(buildLine({ build: "Elf Fighter 2 / Wizard 1", level: 3 })).toBe("Level 3 Elf Fighter / Wizard");
  });

  it("a character tile is a named button (not a switch) with a testid and aria-current when selected", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    const tile = await screen.findByRole("button", { name: /^Seraphine Vale, Level 2 / });
    expect(tile.getAttribute("role")).toBeNull();
    expect(tile.getAttribute("aria-pressed")).toBeNull();
    expect(tile.getAttribute("data-testid")).toMatch(/^character-tile-/);
    expect(tile.getAttribute("aria-current")).toBeNull();
    await user.click(tile);
    expect(tile.getAttribute("aria-current")).toBe("true");
    expect(tile.getAttribute("aria-label")!.match(/Level/g)).toHaveLength(1);
  });

  it("the New character name field is labelled by NAME and has testids", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
    const name = await screen.findByTestId("new-character-name");
    expect(name.getAttribute("aria-labelledby")).toBe("v4-name-label");
    expect(screen.getByLabelText("Name")).toBe(name);
    expect(screen.getByTestId("create-character").textContent).toBe("Create character");
  });

  it("Explain buttons carry their value, and the value has a testid", async () => {
    const api = fast();
    renderApp("/play", { api, openCharacterId: await seededId(api, "Seraphine") });
    const btn = await screen.findByRole("button", { name: /^Explain Proficiency, \+\d+$/ });
    expect(within(btn).getByTestId("explain-proficiency-value").textContent).toMatch(/^\+\d+$/);
  });

  it("build steps have testids and picker rows are one labelled node with hidden decoration", async () => {
    const api = fast();
    renderApp("/builder/race", { api, openCharacterId: await seededId(api, "Seraphine") });
    expect(await screen.findByTestId("build-step-race")).toBeTruthy();
    expect(screen.getByTestId("build-step-class")).toBeTruthy();
    const user = userEvent.setup();
    if (!document.querySelector("button.pick-main")) await user.click(document.querySelector<HTMLElement>("button.aur-expander-toggle")!);
    const row = await waitFor(() => {
      const r = document.querySelector<HTMLElement>("button.pick-main");
      if (!r) throw new Error("no rows yet");
      return r;
    });
    expect(row.getAttribute("aria-label")).toBeTruthy();
    expect(row.querySelectorAll("[aria-hidden=true]").length).toBeGreaterThan(0);
  });
});

describe("Save flyout filename", () => {
  it("editing the Filename does not rename the character", async () => {
    const user = userEvent.setup();
    const api = fast();
    const id = await seededId(api, "Brannoc");
    renderApp("/builder", { api, openCharacterId: id });
    await screen.findByRole("menuitem", { name: /Brannoc/i });
    await user.keyboard("{Control>}s{/Control}");
    const dialog = await screen.findByRole("dialog", { name: "Save character" });
    const file = within(dialog).getByLabelText("Filename");
    await user.clear(file);
    await user.type(file, "PARTY");
    await user.click(within(dialog).getByRole("button", { name: "Save character" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Save character" })).toBeNull());
    expect((await api.getSnapshot({ characterId: id })).name).toBe("Brannoc Ironvale");
    expect(screen.getByRole("menuitem", { name: /Brannoc Ironvale/i })).toBeTruthy();
  });
});

describe("mobile CSS guards", () => {
  const css = (f: string) => readFileSync(resolve(__dirname, "../styles", f), "utf8");
  it("fields are 16px on touch devices so iOS never zooms the page", () => {
    expect(css("base.css")).toMatch(/@media \(pointer: coarse\)[\s\S]*font-size: 16px/);
  });
  it("Android gets a bottom safe-area floor; the bottom bar and status bar respect safe areas", () => {
    expect(css("tokens.css")).toMatch(/:root\.android \{[^}]*--safe-b: max\(env\(safe-area-inset-bottom, 0px\), \d+px/);
    expect(css("shell.css")).toMatch(/\.aur-bottombar \{[^}]*var\(--safe-b\)/);
    expect(css("shell.css")).toMatch(/\.aur-window\.narrow \.aur-status \.status-msg \{ flex: 1 1 0; min-width: 0/);
  });
  it("the full-screen Settings window on a phone keeps its title and tabs out of the status bar (B-08)", () => {
    const settings = readFileSync(resolve(__dirname, "../pages/settings/settings.css"), "utf8");
    const phone = settings.slice(settings.indexOf("@media (max-width: 639px)"));
    expect(phone).toMatch(/\.aur-set-win \{[^}]*padding: var\(--safe-t\) var\(--safe-r\) var\(--safe-b\) var\(--safe-l\)/);
    expect(phone).toMatch(/\.aur-set-win \{[^}]*box-sizing: border-box/);
  });
});
