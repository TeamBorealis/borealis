import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { defaultRestriction, saveDefaultSources } from "../pages/content/applySources";
import { renderApp, seededId } from "./utils";

/** GitHub #22 (B-169): the saved default sources apply at start and to new characters, not only on "Load default". */

const freshApi = () => new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });

/** Saves a default that switches off the last installed source; returns [kept ids, the switched-off source]. */
async function saveDefaultWithoutLast(api: MockApi) {
  const all = await api.listSources();
  expect(all.length).toBeGreaterThan(1);
  const off = all[all.length - 1]!;
  const kept = all.slice(0, -1).map((s) => s.id);
  saveDefaultSources(kept);
  return { kept, off };
}

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

beforeEach(() => { vi.stubGlobal("localStorage", memoryStorage()); });
afterEach(() => { vi.unstubAllGlobals(); });

describe("defaultRestriction", () => {
  it("is null (every source) when nothing is saved", () => {
    expect(defaultRestriction(null, ["a", "b"])).toBeNull();
  });

  it("keeps the saved sources that are installed and drops unknown ids", () => {
    expect(defaultRestriction(["a", "gone"], ["a", "b"])).toEqual(["a"]);
  });

  it("is null when the saved default allows every installed source", () => {
    expect(defaultRestriction(["b", "a"], ["a", "b"])).toBeNull();
  });

  it("is null when none of the saved sources is installed any more", () => {
    expect(defaultRestriction(["gone"], ["a", "b"])).toBeNull();
  });

  it("does not change its input", () => {
    const saved = ["a", "a", "gone"];
    defaultRestriction(saved, ["a", "b"]);
    expect(saved).toEqual(["a", "a", "gone"]);
  });
});

describe("saved default sources", () => {
  it("show on the sources page at start, with no character loaded", async () => {
    const api = freshApi();
    const { kept, off } = await saveDefaultWithoutLast(api);
    renderApp("/content/sources", { api });
    const grid = await screen.findByRole("grid", { name: "Sources" });
    const offBox = await within(grid).findByRole("checkbox", { name: `Allow ${off.name}` });
    await waitFor(() => expect((offBox as HTMLInputElement).checked).toBe(false));
    const all = await api.listSources();
    for (const id of kept) {
      const name = all.find((s) => s.id === id)!.name;
      expect((within(grid).getByRole("checkbox", { name: `Allow ${name}` }) as HTMLInputElement).checked).toBe(true);
    }
  });

  it("apply to a new character", async () => {
    const user = userEvent.setup();
    const api = freshApi();
    const { kept } = await saveDefaultWithoutLast(api);
    renderApp("/characters", { api });
    await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
    const dialog = await screen.findByRole("dialog", { name: "New character" });
    await user.type(within(dialog).getByLabelText("Name"), "Defaulted");
    await user.click(within(dialog).getByRole("button", { name: "Create character" }));
    await waitFor(async () => expect((await api.listCharacters()).some((c) => c.name === "Defaulted")).toBe(true));
    const id = (await api.listCharacters()).find((c) => c.name === "Defaulted")!.id;
    await waitFor(async () => {
      const snap = await api.getSnapshot({ characterId: id });
      expect([...(snap.sourceRestrictions ?? [])].sort()).toEqual([...kept].sort());
    });
  });

  it("leave a new character unrestricted when none are saved", async () => {
    const user = userEvent.setup();
    const api = freshApi();
    renderApp("/characters", { api });
    await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
    const dialog = await screen.findByRole("dialog", { name: "New character" });
    await user.type(within(dialog).getByLabelText("Name"), "Unrestricted");
    await user.click(within(dialog).getByRole("button", { name: "Create character" }));
    await waitFor(async () => expect((await api.listCharacters()).some((c) => c.name === "Unrestricted")).toBe(true));
    const id = (await api.listCharacters()).find((c) => c.name === "Unrestricted")!.id;
    const snap = await api.getSnapshot({ characterId: id });
    expect(snap.sourceRestrictions).toBeNull();
  });

  it("do not change an existing character's sources", async () => {
    const api = freshApi();
    const { off } = await saveDefaultWithoutLast(api);
    const id = await seededId(api, "Brannoc");
    renderApp("/content/sources", { api, openCharacterId: id });
    const grid = await screen.findByRole("grid", { name: "Sources" });
    const offBox = await within(grid).findByRole("checkbox", { name: `Allow ${off.name}` });
    await screen.findByRole("button", { name: "Apply to character" });
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).sourceRestrictions).toBeNull());
    expect((offBox as HTMLInputElement).checked).toBe(true);
  });
});
