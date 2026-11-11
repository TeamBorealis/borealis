import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewElementSummary, HomebrewKindDescriptor } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { CREATOR_KEYMAP } from "../pages/homebrew/keymap";
import { keysText } from "../pages/homebrew/KeysDialog";
import { EMPTY_FILTER, filterRows, nextSelection } from "../pages/homebrew/search";
import { renderApp } from "./utils";

// Homebrew WB-23 (spec 12): the list filter and multi-select (12.3, 12.5), SPELL LISTS with appends and where used (12.4,
// 12.3), the DATA view (12.1) and the keyboard map's listing (12.2), on the web mock.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };

/** A saved element of the pack (through the mock's draft commands, as the editor makes them). */
async function saved(api: MockApi, packId: string, kind: string, fields: Record<string, unknown>): Promise<string> {
  const v = await api.homebrewNewDraft({ packId, kind });
  const u = await api.homebrewUpdateDraft({ draftId: v.draftId, form: { ...v.form, fields: { ...v.form.fields, ...fields } } });
  return (await api.homebrewSaveDraft({ draftId: u.draftId })).elementIds[0]!;
}

const row = (over: Partial<HomebrewElementSummary>): HomebrewElementSummary => ({
  id: "ID_A", name: "Gloomspeak", kind: "language", elementType: "Language", parentId: null, edition: null,
  problems: { errors: 0, warnings: 0 }, hasDraft: false, tags: [], text: "", copiedText: false, ...over,
});
const KINDS = [{ id: "language", label: "Language", pluralLabel: "Languages" }, { id: "spell", label: "Spell", pluralLabel: "Spells" }] as HomebrewKindDescriptor[];

describe("WB-23 list filter and selection (spec 12.3, 12.5)", () => {
  const rows = [
    row({ id: "A", name: "Gloomspeak", tags: ["Secret"] }),
    row({ id: "B", name: "Ember Lance", kind: "spell", elementType: "Spell", text: "A lance of fire.", problems: { errors: 1, warnings: 0 } }),
    row({ id: "C", name: "Brightspeak" }),
  ];

  it("without a filter the kind's rows; a filter searches every kind by name, kind, tag and text; has problems narrows", () => {
    expect(filterRows(rows, "language", EMPTY_FILTER, KINDS).map((r) => r.id)).toEqual(["A", "C"]);
    expect(filterRows(rows, "language", { text: "secret", problemsOnly: false }, KINDS).map((r) => r.id)).toEqual(["A"]);
    expect(filterRows(rows, "language", { text: "fire", problemsOnly: false }, KINDS).map((r) => r.id)).toEqual(["B"]);
    expect(filterRows(rows, "language", { text: "spell lance", problemsOnly: false }, KINDS).map((r) => r.id)).toEqual(["B"]);
    expect(filterRows(rows, "language", { text: "speak", problemsOnly: false }, KINDS).map((r) => r.id)).toEqual(["A", "C"]);
    expect(filterRows(rows, "language", { text: "", problemsOnly: true }, KINDS).map((r) => r.id)).toEqual(["B"]);
  });

  it("a click chooses one row, Ctrl adds or removes, Shift adds the run from the last click", () => {
    const shown = ["A", "B", "C", "D"];
    expect([...nextSelection(new Set(["A"]), shown, "C", "A", { toggle: false, range: false })]).toEqual(["C"]);
    expect([...nextSelection(new Set(["A"]), shown, "C", "A", { toggle: true, range: false })]).toEqual(["A", "C"]);
    expect([...nextSelection(new Set(["A", "C"]), shown, "C", "A", { toggle: true, range: false })]).toEqual(["A"]);
    expect([...nextSelection(new Set(["A"]), shown, "C", "A", { toggle: false, range: true })]).toEqual(["A", "B", "C"]);
  });
});

describe("WB-23 workspace (mock)", () => {
  it("Ctrl+click chooses two elements; Add tag is one step that Undo takes back", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    await saved(api, pack.id, "language", { name: "Gloomspeak" });
    await saved(api, pack.id, "language", { name: "Brightspeak", tags: ["Planar"] });
    const bulk = vi.spyOn(api, "homebrewBulkEdit");
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: /^Languages \(2\)/ }));
    await user.click(await screen.findByRole("button", { name: "Gloomspeak" }));
    await user.keyboard("{Control>}");
    await user.click(screen.getByRole("button", { name: "Brightspeak" }));
    await user.keyboard("{/Control}");
    const bar = await screen.findByRole("toolbar", { name: "2 elements chosen" });
    await user.click(within(bar).getByRole("button", { name: "Add tag..." }));
    const dialog = await screen.findByRole("dialog", { name: "Add a tag" });
    await user.type(within(dialog).getByRole("textbox", { name: "Tag" }), "Secret");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(bulk).toHaveBeenCalledWith(expect.objectContaining({ patch: { addTags: ["Secret"] } })));
    expect(bulk.mock.calls[0]![0].elementIds).toHaveLength(2);
    await waitFor(async () => expect((await api.homebrewListElements({ packId: pack.id })).every((r) => r.tags.includes("Secret"))).toBe(true));

    // the toast's Undo (the editor's own Undo button undoes the open form)
    await waitFor(() => expect(document.querySelector(".toast-action")).not.toBeNull());
    await user.click(document.querySelector<HTMLButtonElement>(".toast-action")!);
    await waitFor(async () => expect((await api.homebrewListElements({ packId: pack.id })).map((r) => r.tags).sort()).toEqual([[], ["Planar"]]));

    // the filter finds by tag across kinds
    await user.type(screen.getByRole("textbox", { name: "Filter elements" }), "planar");
    await waitFor(() => expect(screen.queryByRole("button", { name: "Gloomspeak" })).toBeNull());
    expect(screen.getByRole("button", { name: "Brightspeak" })).toBeTruthy();
  });

  it("SPELL LISTS: three checks wait for Save, then the list counts them and WHERE USED names the list", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const set = vi.spyOn(api, "homebrewSetSpellList");
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "Spell Lists" }));
    const view = await screen.findByRole("region", { name: "Spell lists" });
    await user.selectOptions(within(view).getByRole("combobox", { name: "Spell list" }), "Wizard");
    const grid = await within(view).findByRole("table", { name: "Spells" });
    const before = Number(/(\d+) spells on this list/.exec(within(view).getByRole("status").textContent ?? "")![1]);
    expect((within(grid).getByRole("checkbox", { name: "Choose Fireball" }) as HTMLInputElement).disabled).toBe(true);
    for (const name of ["Bless", "Cure Wounds", "Guidance"]) await user.click(within(grid).getByRole("checkbox", { name: `Choose ${name}` }));
    expect(within(view).getByRole("status").textContent).toContain(`${before + 3} spells on this list (3 not saved)`);
    expect(set).not.toHaveBeenCalled();
    await user.click(within(view).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(set).toHaveBeenCalledWith(expect.objectContaining({ listTag: "Wizard", add: ["bless", "cure-wounds", "guidance"], remove: [] })));
    await waitFor(() => expect(within(view).getByRole("status").textContent).toBe(`${before + 3} spells on this list`));
    expect((within(grid).getByRole("checkbox", { name: "Choose Bless" }) as HTMLInputElement).checked).toBe(true);

    await user.click(within(grid).getByRole("button", { name: "Bless" }));
    const where = await screen.findByRole("region", { name: "Where used" });
    expect(await within(where).findByRole("button", { name: "Wizard" })).toBeTruthy();
    expect(within(where).getAllByText("Lists it · Class")).toHaveLength(2); // Cleric, its own list, and Wizard by the append
  });

  it("DATA shows the stored element and copies it as JSON; Keys lists every binding of the map", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    await saved(api, pack.id, "language", { name: "Gloomspeak", tags: ["Secret"] });
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: /^Languages \(1\)/ }));
    await user.click(await screen.findByRole("button", { name: "Gloomspeak" }));
    await user.click(await screen.findByRole("tab", { name: "DATA" }));
    const tree = await screen.findByRole("tree", { name: "Stored element" });
    expect(within(tree).getByRole("treeitem", { name: "name: Gloomspeak" })).toBeTruthy();
    expect(within(tree).getByRole("treeitem", { name: "1: Secret" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Copy as JSON" }));
    expect(JSON.parse(writeText.mock.calls[0]![0] as string)).toMatchObject({ name: "Gloomspeak", supports: ["Secret"] });

    await user.click(screen.getByRole("button", { name: "Keys" }));
    const keys = await screen.findByRole("dialog", { name: "Keyboard shortcuts" });
    for (const b of CREATOR_KEYMAP) expect(within(keys).getAllByText(keysText(b.keys)).length).toBeGreaterThan(0);
  });
});
