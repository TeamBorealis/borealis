import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ContentSource } from "../api/contract";
import { MockApi } from "../api/mock";
import { applyDefaultSources } from "../pages/content/applySources";
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
    // An older backend sends no thirdParty: the book counts as official.
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
    const btn = await screen.findByRole("button", { name: "Restrict playtest content" });
    const grid = await screen.findByRole("grid", { name: "Sources" });
    const box = () => within(grid).getByRole("checkbox", { name: `Allow ${books[0]!.name}` }) as HTMLInputElement;
    await waitFor(() => expect(box().checked).toBe(true));
    expect(btn.getAttribute("aria-pressed")).toBe("false");
    await user.click(btn);
    await waitFor(() => expect(btn.getAttribute("aria-pressed")).toBe("true"));
    expect(box().checked).toBe(false);
    expect(screen.getByRole("status", { name: "Restricted sources" }).textContent).toBe("1 playtest source restricted");
    expect((await api.getSettings()).restrictPlaytest).toBe(true);
    await user.click(btn);
    await waitFor(() => expect(btn.getAttribute("aria-pressed")).toBe("false"));
    expect(box().checked).toBe(true);
    expect((await api.getSettings()).restrictPlaytest).toBe(false);
  });

  it("restricts third-party content on the loaded character", async () => {
    const user = userEvent.setup();
    const api = withFlags(new MockApi({ rng: () => 0.5 }));
    const books = await flagged(api);
    const id = await seededId(api, "Brannoc");
    renderApp("/content/sources", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: "Restrict third-party content" }));
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
    const btn = await screen.findByRole("button", { name: "Restrict third-party content" });
    expect((btn as HTMLButtonElement).disabled).toBe(true);
    expect(btn.title).toMatch(/no third-party sources are installed/);
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
