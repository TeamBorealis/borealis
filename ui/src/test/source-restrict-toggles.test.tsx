import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ContentSource } from "../api/contract";
import { MockApi } from "../api/mock";
import { applyDefaultSources, loadDefaultSources, startingSources } from "../pages/content/applySources";
import { idsOfKind, isKind, restrictedCount, restrictedNote, withToggles } from "../pages/content/sourceFilters";
import { renderApp, seededId } from "./utils";

const src = (id: string, playtest = false, thirdParty = false): ContentSource => ({
  id, name: id, group: "G", packId: "p", elementCount: 1, description: "", playtest, thirdParty,
  edition: null, editionSetByUser: false, editionEditable: false,
});
const BOOKS = [src("Core Book"), src("Playtest One", true), src("Playtest Two", true), src("Third Party", false, true)];

describe("source classification", () => {
  it("sorts books into playtest and third-party", () => {
    expect(idsOfKind(BOOKS, "playtest")).toEqual(["Playtest One", "Playtest Two"]);
    expect(idsOfKind(BOOKS, "thirdParty")).toEqual(["Third Party"]);
    // An older backend sends no thirdParty: the book is not third-party.
    expect(isKind({ playtest: false } as ContentSource, "thirdParty")).toBe(false);
  });

  it("counts the restricted books of a kind", () => {
    const allowed = new Set(["Core Book", "Playtest Two", "Third Party"]);
    expect(restrictedCount(BOOKS, "playtest", allowed)).toBe(1);
    expect(restrictedCount(BOOKS, "thirdParty", allowed)).toBe(0);
    expect(restrictedNote(23, "playtest")).toBe("23 playtest sources restricted");
    expect(restrictedNote(1, "thirdParty")).toBe("1 third-party source restricted");
  });

  it("leaves out each switched-on kind and keeps null for every source", () => {
    expect(withToggles(null, BOOKS, {})).toBeNull();
    expect(withToggles(null, BOOKS, { restrictPlaytest: true })?.sort()).toEqual(["Core Book", "Third Party"]);
    expect(withToggles(null, BOOKS, { restrictPlaytest: true, restrictThirdParty: true })).toEqual(["Core Book"]);
    expect(withToggles(["Core Book", "Playtest One"], BOOKS, { restrictThirdParty: true })).toEqual(["Core Book", "Playtest One"]);
  });
});

/** The mock's books with the first marked playtest and the second third-party. */
async function flagged(api: MockApi): Promise<ContentSource[]> {
  const list = await MockApi.prototype.listSources.call(api);
  return list.map((s, i) => ({ ...s, playtest: i === 0, thirdParty: i === 1 }));
}
function withFlags(api: MockApi): MockApi {
  api.listSources = () => flagged(api);
  return api;
}

describe("Sources page restrict toggles", () => {
  it("restricts playtest content as a remembered toggle, and allows it again", async () => {
    const user = userEvent.setup();
    const api = withFlags(new MockApi({ rng: () => 0.5 }));
    const books = await flagged(api);
    renderApp("/content/sources", { api });
    const btn = await screen.findByRole("checkbox", { name: "Restrict playtest content" });
    const grid = await screen.findByRole("grid", { name: "Sources" });
    const box = () => within(grid).getByRole("checkbox", { name: `Allow ${books[0]!.name}` }) as HTMLInputElement;
    await waitFor(() => expect(box().checked).toBe(true));
    expect((btn as HTMLInputElement).checked).toBe(false);
    await user.click(btn);
    await waitFor(() => expect((btn as HTMLInputElement).checked).toBe(true));
    expect(box().checked).toBe(false);
    expect(screen.getByRole("status", { name: "Restricted sources" }).textContent).toBe("1 playtest source restricted");
    expect((await api.getSettings()).restrictPlaytest).toBe(true);
    await user.click(btn);
    await waitFor(() => expect((btn as HTMLInputElement).checked).toBe(false));
    expect(box().checked).toBe(true);
    expect((await api.getSettings()).restrictPlaytest).toBe(false);
  });

  it("restricts third-party content on the loaded character", async () => {
    const user = userEvent.setup();
    const api = withFlags(new MockApi({ rng: () => 0.5 }));
    const books = await flagged(api);
    const id = await seededId(api, "Brannoc");
    renderApp("/content/sources", { api, openCharacterId: id });
    await user.click(await screen.findByRole("checkbox", { name: "Restrict third-party content" }));
    const apply = await screen.findByRole("button", { name: "Apply" }, { timeout: 500 }).catch(() => null);
    if (apply) await user.click(apply);
    await waitFor(async () => {
      const r = (await api.getSnapshot({ characterId: id })).sourceRestrictions;
      expect(r).not.toBeNull();
      expect(r).not.toContain(books[1]!.id);
      expect(r).toContain(books[0]!.id);
    });
    expect(screen.getByRole("status", { name: "Restricted sources" }).textContent).toBe("1 third-party source restricted");
  });

  it("disables a toggle when no book of its kind is installed", async () => {
    renderApp("/content/sources");
    const btn = await screen.findByRole("checkbox", { name: "Restrict third-party content" });
    expect((btn as HTMLInputElement).disabled).toBe(true);
    expect(btn.closest("label")!.title).toMatch(/no third-party sources are installed/);
  });
});

describe("new characters", () => {
  it("start without the restricted kinds", async () => {
    const api = withFlags(new MockApi({ rng: () => 0.5 }));
    const books = await flagged(api);
    api.presetSettings({ restrictPlaytest: true });
    const id = await seededId(api, "Brannoc");
    const snap = await applyDefaultSources(api, await api.getSnapshot({ characterId: id }));
    expect(snap.sourceRestrictions).not.toContain(books[0]!.id);
    expect(snap.sourceRestrictions).toContain(books[1]!.id);
  });

  it("keep every source when nothing is saved or restricted", async () => {
    const api = withFlags(new MockApi({ rng: () => 0.5 }));
    const id = await seededId(api, "Brannoc");
    const before = await api.getSnapshot({ characterId: id });
    expect((await applyDefaultSources(api, before)).sourceRestrictions).toEqual(before.sourceRestrictions);
  });
});

/** In-memory Web Storage: newer Node runtimes shadow jsdom's localStorage with an unconfigured one under Vitest. */
function memoryStorage(): Storage {
  const m = new Map<string, string>();
  return {
    get length() { return m.size; },
    clear: () => m.clear(),
    getItem: (k) => m.get(k) ?? null,
    key: (i) => [...m.keys()][i] ?? null,
    removeItem: (k) => { m.delete(k); },
    setItem: (k, v) => { m.set(k, String(v)); },
  };
}

describe("saved default sources with the toggles on", () => {
  beforeEach(() => { vi.stubGlobal("localStorage", memoryStorage()); });
  afterEach(() => { vi.unstubAllGlobals(); });

  it("the toggles never take a book out of a saved default", () => {
    // "Third Party" is the user's own book here: checked again after the toggle, then saved.
    const saved = ["Core Book", "Third Party"];
    expect(startingSources(saved, BOOKS, { restrictPlaytest: true, restrictThirdParty: true })).toEqual(saved);
    expect(startingSources(null, BOOKS, { restrictThirdParty: true })?.sort()).toEqual(["Core Book", "Playtest One", "Playtest Two"]);
  });

  it("save with a search filter and Restrict third-party on, then load, gives back exactly the checked set", async () => {
    const user = userEvent.setup();
    const api = withFlags(new MockApi({ rng: () => 0.5 }));
    const books = await flagged(api);
    const own = books[1]!; // third-party by its Source element; the user checks it again
    renderApp("/content/sources", { api });
    const grid = await screen.findByRole("grid", { name: "Sources" });
    const box = (b: ContentSource) => within(grid).getByRole("checkbox", { name: `Allow ${b.name}` }) as HTMLInputElement;
    await user.click(await screen.findByRole("checkbox", { name: "Restrict third-party content" }));
    await waitFor(() => expect(box(own).checked).toBe(false));
    await user.click(screen.getByRole("checkbox", { name: "Restrict playtest content" }));
    await waitFor(() => expect(box(books[0]!).checked).toBe(false));

    // Filter the list down to the user's book, check it again and save.
    await user.type(screen.getByRole("searchbox", { name: "Search sources" }), own.name.slice(0, 4));
    await user.click(box(own));
    expect(box(own).checked).toBe(true);
    await user.click(screen.getByRole("button", { name: "Save as default" }));
    const expected = books.filter((b) => b.id !== books[0]!.id).map((b) => b.id).sort();
    expect([...(loadDefaultSources() ?? [])].sort()).toEqual(expected);

    // Uncheck it, then load the default: it comes back checked, and the playtest book stays off.
    await user.click(box(own));
    expect(box(own).checked).toBe(false);
    await user.click(screen.getByRole("button", { name: "Load default" }));
    await waitFor(() => expect(box(own).checked).toBe(true));
    await user.clear(screen.getByRole("searchbox", { name: "Search sources" }));
    expect(box(books[0]!).checked).toBe(false);
    expect(screen.getByRole("status", { name: "Restricted sources" }).textContent).toBe("1 playtest source restricted · 0 third-party sources restricted");

    // A new character starts on the same set.
    const id = await seededId(api, "Brannoc");
    const snap = await applyDefaultSources(api, await api.getSnapshot({ characterId: id }));
    expect([...(snap.sourceRestrictions ?? [])].sort()).toEqual(expected);
  });
});

describe("switches and the saved default", () => {
  beforeEach(() => { vi.stubGlobal("localStorage", memoryStorage()); });
  afterEach(() => { vi.unstubAllGlobals(); });

  it("a default saved first loses that kind's books when the switch is turned on, and a book checked again later wins", async () => {
    const user = userEvent.setup();
    const api = withFlags(new MockApi({ rng: () => 0.5 }));
    const books = await flagged(api);
    renderApp("/content/sources", { api });
    const grid = await screen.findByRole("grid", { name: "Sources" });
    const box = (b: ContentSource) => within(grid).getByRole("checkbox", { name: `Allow ${b.name}` }) as HTMLInputElement;
    await waitFor(() => expect(box(books[0]!).checked).toBe(true));
    // Save the default with every book checked, then switch playtest on.
    await user.click(screen.getByRole("button", { name: "Save as default" }));
    expect(loadDefaultSources()).toContain(books[0]!.id);
    await user.click(screen.getByRole("checkbox", { name: "Restrict playtest content" }));
    await waitFor(() => expect(box(books[0]!).checked).toBe(false));
    await waitFor(() => expect(loadDefaultSources()).not.toContain(books[0]!.id));
    expect(loadDefaultSources()).toContain(books[1]!.id);
    // A new character starts without the playtest book, matching the page.
    const id = await seededId(api, "Brannoc");
    const snap = await applyDefaultSources(api, await api.getSnapshot({ characterId: id }));
    expect(snap.sourceRestrictions).not.toContain(books[0]!.id);
  });

  it("the one-time playtest flask leaves the remembered switch alone", async () => {
    const user = userEvent.setup();
    const api = withFlags(new MockApi({ rng: () => 0.5 }));
    const books = await flagged(api);
    renderApp("/content/sources", { api });
    const grid = await screen.findByRole("grid", { name: "Sources" });
    const box = within(grid).getByRole("checkbox", { name: `Allow ${books[0]!.name}` }) as HTMLInputElement;
    await user.click(await screen.findByRole("button", { name: "Restrict all playtest material" }));
    await waitFor(() => expect(box.checked).toBe(false));
    expect((await api.getSettings()).restrictPlaytest).toBe(false);
  });

  it("with a character loaded, Cancel on the confirm leaves the switch and the character unchanged", async () => {
    const user = userEvent.setup();
    const api = withFlags(new MockApi({ rng: () => 0.5 }));
    const books = await flagged(api);
    const id = await seededId(api, "Brannoc");
    api.previewSourceRestrictions = async () => ({ affected: [{ slotId: "s", element: "Some pick" }] } as never);
    renderApp("/content/sources", { api, openCharacterId: id });
    const sw = await screen.findByRole("checkbox", { name: "Restrict playtest content" });
    await user.click(sw);
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    await waitFor(() => expect((sw as HTMLInputElement).checked).toBe(false));
    expect((await api.getSettings()).restrictPlaytest).toBe(false);
    expect((await api.getSnapshot({ characterId: id })).sourceRestrictions).toBeNull();
    expect(books.length).toBeGreaterThan(1);
  });
});