import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { SearchIndexStatus } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { footerText, groupRows, highlightParts, initialEdition, rowOfHit } from "../pages/compendium/searchModel";
import { renderApp } from "./utils";

async function openSearch(query: string, api?: MockApi) {
  const user = userEvent.setup();
  renderApp("/characters", { api });
  await user.type(await screen.findByLabelText("Compendium search"), `${query}{Enter}`);
  return { user, dlg: await screen.findByRole("dialog", { name: "Compendium" }) };
}

const names = (dlg: HTMLElement) => Array.from(dlg.querySelectorAll(".v4-el .n")).map((e) => e.textContent);

describe("search flyout (S1.5)", () => {
  it("a typo still lists Fireball first, as you type", async () => {
    // Arrange / Act
    const { dlg } = await openSearch("firball");

    // Assert
    await waitFor(() => expect(names(dlg)[0]).toBe("Fireball"));
  });

  it("offers All | Rules | Lore scope tabs (no Sessions without campaigns), All selected", async () => {
    // Arrange / Act
    const { dlg } = await openSearch("fireball");

    // Assert
    const tabs = within(dlg).getByRole("tablist", { name: "Search scope" });
    expect(within(tabs).getAllByRole("tab").map((t) => t.textContent)).toEqual(["All", "Rules", "Lore"]);
    expect(within(tabs).getByRole("tab", { name: "All" }).getAttribute("aria-selected")).toBe("true");
  });

  it("Rules scope finds a section and opens its text with the attribution", async () => {
    // Arrange
    const { user, dlg } = await openSearch("dash");

    // Act
    await user.click(within(dlg).getByRole("tab", { name: "Rules" }));
    const list = await within(dlg).findByRole("list", { name: "Compendium results: Rules" });
    await user.click(within(list).getByRole("button", { name: /Dash/ }));

    // Assert
    const entry = await within(dlg).findByRole("article", { name: "Dash" });
    expect(entry.textContent).toContain("Combat > Actions > Dash");
    expect(entry.textContent).toContain("Mock attribution");
    expect(within(dlg).queryByRole("combobox", { name: "Filter by type" })).toBeNull();
  });

  it("Lore scope opens the element detail as before", async () => {
    // Arrange
    const { user, dlg } = await openSearch("fireball");

    // Act
    await user.click(within(dlg).getByRole("tab", { name: "Lore" }));
    await waitFor(() => expect(within(dlg).queryByRole("list", { name: "Compendium results: Rules" })).toBeNull());
    await user.click((await within(dlg).findAllByRole("button", { name: /Fireball/ }))[0]!);

    // Assert
    expect(await within(dlg).findByRole("article", { name: "Fireball" })).toBeTruthy();
  });

  it("rows show a snippet with the matched words highlighted", async () => {
    // Arrange / Act
    const { dlg } = await openSearch("burst of flame");

    // Assert
    await waitFor(() => expect(dlg.querySelector(".v4-el .snip mark")).not.toBeNull());
    const marks = Array.from(dlg.querySelectorAll(".v4-el .snip mark")).map((m) => m.textContent?.toLowerCase());
    expect(marks).toContain("flame");
  });

  it("the footer says Keyword search and follows the index status event", async () => {
    // Arrange
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const building: SearchIndexStatus = { state: "building", done: 1, total: 3, message: null };
    vi.spyOn(api, "searchIndexStatus").mockResolvedValue(building);
    const { dlg } = await openSearch("fireball", api);
    const foot = await within(dlg).findByRole("note", { name: "Search mode" });
    await waitFor(() => expect(foot.textContent).toBe("Keyword search - indexing 1/3"));

    // Act
    const emit = (api as unknown as { emit: (e: string, p: SearchIndexStatus) => void }).emit.bind(api);
    act(() => emit("search:index-status", { state: "ready", done: 3, total: 3, message: null }));

    // Assert
    await waitFor(() => expect(foot.textContent).toBe("Keyword search"));
  });

  it("a query typed while the index builds is searched again once the index is ready", async () => {
    // Arrange: the first run on a phone; before the index exists the backend answers from the
    // plain name scan, which finds nothing for a typo (seen on the Android emulator: "0 results"
    // that never updated).
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    vi.spyOn(api, "searchIndexStatus").mockResolvedValue({ state: "building", done: 0, total: 3, message: null });
    const real = api.search.bind(api);
    let built = false;
    const search = vi.spyOn(api, "search").mockImplementation(async (args) => {
      const page = await real(args);
      return built ? page : { ...page, items: [], total: 0 };
    });
    const { dlg } = await openSearch("firball", api);
    await waitFor(() => expect(search).toHaveBeenCalled());
    await waitFor(() => expect(within(dlg).getByRole("status").textContent).toBe("0 results"));
    const calls = search.mock.calls.length;

    // Act
    built = true;
    const emit = (api as unknown as { emit: (e: string, p: SearchIndexStatus) => void }).emit.bind(api);
    act(() => emit("search:index-status", { state: "ready", done: 3, total: 3, message: null }));

    // Assert
    await waitFor(() => expect(names(dlg)[0]).toBe("Fireball"));
    expect(search.mock.calls.length).toBe(calls + 1);
  });

  it("an index that is already ready does not search twice", async () => {
    // Arrange
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
    const search = vi.spyOn(api, "search");

    // Act
    const { dlg } = await openSearch("firball", api);
    await waitFor(() => expect(names(dlg)[0]).toBe("Fireball"));
    await act(() => new Promise((r) => setTimeout(r, 300)));

    // Assert
    expect(search).toHaveBeenCalledTimes(1);
  });

  it("an empty query lists the compendium like before (no snippets)", async () => {
    // Arrange
    const user = userEvent.setup();
    renderApp("/characters");

    // Act
    await user.keyboard("{F1}");
    const dlg = await screen.findByRole("dialog", { name: "Compendium" });

    // Assert
    await waitFor(() => expect(names(dlg).length).toBeGreaterThan(5));
    expect(dlg.querySelector(".v4-el .snip")).toBeNull();
  });

  it("phones reach it as More > Search", async () => {
    // Arrange
    const user = userEvent.setup();
    renderApp("/more", { api: new MockApi({ mobile: true }) });

    // Act
    const list = await screen.findByRole("list", { name: "More destinations" });
    await user.click(within(list).getByRole("link", { name: /Search/ }));

    // Assert
    expect(await screen.findByRole("dialog", { name: "Compendium" })).toBeTruthy();
  });
});

describe("search model", () => {
  it("highlight parts cover the text once even for overlapping or unsorted ranges", () => {
    expect(highlightParts("a fire bolt", [[7, 11], [2, 6], [3, 5]])).toEqual([
      { text: "a ", hit: false }, { text: "fire", hit: true }, { text: " ", hit: false }, { text: "bolt", hit: true },
    ]);
    expect(highlightParts("abc", [[5, 9]])).toEqual([{ text: "abc", hit: false }]);
  });

  it("groups keep the order of their best hit", () => {
    const hit = (docKey: string, type: "spell" | "rules") => rowOfHit({ docKey, title: docKey, path: "", type, ruleset: null, source: "", snippet: "", highlights: [], elementId: null });
    expect(groupRows([hit("a", "spell"), hit("b", "rules"), hit("c", "spell")]).map(([t, r]) => `${t}:${r.length}`)).toEqual(["spell:2", "rules:1"]);
  });

  it("the edition toggle starts from settings, the character's edition, else both", () => {
    expect(initialEdition("character", "dnd2014")).toBe("dnd2014");
    expect(initialEdition("character", null)).toBe("any");
    expect(initialEdition("both", "dnd2014")).toBe("any");
    expect(initialEdition("2024", null)).toBe("dnd2024");
    expect(footerText({ state: "failed", done: 0, total: 3, message: "x" })).toBe("Keyword search - index unavailable");
  });
});
