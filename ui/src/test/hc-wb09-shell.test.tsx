import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { matchCreatorKey, CREATOR_KEYMAP, type KeyLike } from "../pages/homebrew/keymap";
import { suggestCode } from "../pages/homebrew/NewPackFlyout";
import { renderApp, seededId } from "./utils";

// Homebrew WB-09: START > HOMEBREW on the mock (`creatorEnabled` on): pack list, NEW PACK, the workspace (CATEGORIES,
// element grid, editor, live description), the sub-tab band, PROBLEMS, undo/redo and the creator's keyboard map (spec
// 12.2), scoped to the creator route.

const mock = () => new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, creatorEnabled: true });
type User = ReturnType<typeof userEvent.setup>;

async function packWithSpell(api: MockApi, name = "Ember Bolt"): Promise<string> {
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  const d = await api.homebrewNewDraft({ packId: pack.id, kind: "spell" });
  await api.homebrewUpdateDraft({ draftId: d.draftId, form: { ...d.form, fields: { ...d.form.fields, name, school: "Evocation" } } });
  await api.homebrewSaveDraft({ draftId: d.draftId });
  return pack.id;
}

const nameBox = () => screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement;
const editor = () => screen.getByRole("region", { name: "Element editor" });

async function openSpell(user: User, name = "Ember Bolt"): Promise<void> {
  await user.click(await screen.findByRole("button", { name }));
  await waitFor(() => expect(nameBox().value).toBe(name));
}

const key = (k: string, mods: Partial<KeyLike> = {}): KeyLike => ({ key: k, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...mods });

describe("WB-09 keyboard map (spec 12.2)", () => {
  it.each([
    [key("s", { ctrlKey: true }), "save"],
    [key("s", { metaKey: true }), "save"],
    [key("z", { ctrlKey: true }), "undo"],
    [key("Z", { ctrlKey: true, shiftKey: true }), "redo"],
    [key("z", { metaKey: true, shiftKey: true }), "redo"],
    [key("N", { ctrlKey: true, shiftKey: true }), "newElement"],
    [key("d", { ctrlKey: true }), "duplicate"],
    [key("f", { ctrlKey: true }), "focusFilter"],
    [key("t", { ctrlKey: true }), "tryIt"],
    [key("Insert"), "addBlock"],
    [key("Enter", { ctrlKey: true }), "addBlock"],
    [key("ArrowUp", { altKey: true }), "moveUp"],
    [key("ArrowDown", { altKey: true }), "moveDown"],
    [key("F2"), "rename"],
  ])("%o -> %s", (e, id) => {
    expect(matchCreatorKey(e)?.id).toBe(id);
  });

  it("Ctrl/Cmd+1..9 pick the editor sub-tabs", () => {
    expect(matchCreatorKey(key("1", { ctrlKey: true }))).toEqual({ id: "tab", index: 0 });
    expect(matchCreatorKey(key("9", { metaKey: true }))).toEqual({ id: "tab", index: 8 });
  });

  it("leaves the shell's keys alone: Ctrl+N (new character), Ctrl+P, plain letters, Alt+Q", () => {
    for (const e of [key("n", { ctrlKey: true }), key("p", { ctrlKey: true }), key("s"), key("q", { altKey: true }), key("0", { ctrlKey: true }), key("s", { ctrlKey: true, altKey: true })]) {
      expect(matchCreatorKey(e)).toBeNull();
    }
  });

  it("lists every action of spec 12.2 once", () => {
    const ids = CREATOR_KEYMAP.map((b) => b.action);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.sort()).toEqual(["addBlock", "deleteBlock", "duplicate", "editBlock", "focusFilter", "moveDown", "moveUp", "newElement", "redo", "rename", "save", "tab", "tryIt", "undo"]);
  });

  it("Ctrl+S on the creator route saves the element, never the open character", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const packId = await packWithSpell(api);
    const characterId = await seededId(api, "Seraphine");
    const saveDraft = vi.spyOn(api, "homebrewSaveDraft");
    renderApp(`/homebrew/${packId}`, { api, openCharacterId: characterId });
    await openSpell(user);
    await user.clear(nameBox());
    await user.type(nameBox(), "Ember Lance");

    // Act
    await user.keyboard("{Control>}s{/Control}");

    // Assert
    await waitFor(() => expect(saveDraft).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole("button", { name: "Ember Lance" })).toBeTruthy();
    expect(screen.queryByRole("dialog", { name: "Save character" })).toBeNull();
  });

  it("Ctrl+S elsewhere still saves the character (its Save flyout)", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const characterId = await seededId(api, "Seraphine");
    renderApp("/characters", { api, openCharacterId: characterId });
    await screen.findByRole("button", { name: /^Save/ });

    // Act
    await user.keyboard("{Control>}s{/Control}");

    // Assert
    expect(await screen.findByRole("dialog", { name: "Save character" })).toBeTruthy();
  });

  it("Ctrl+Z / Ctrl+Shift+Z undo and redo, Ctrl+2 changes sub-tab, F2 focuses the name, Ctrl+F the filter", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const packId = await packWithSpell(api);
    renderApp(`/homebrew/${packId}`, { api });
    await openSpell(user);
    await user.type(nameBox(), "s");

    // Act + Assert
    await user.keyboard("{Control>}z{/Control}");
    expect(nameBox().value).toBe("Ember Bolt");
    await user.keyboard("{Control>}{Shift>}z{/Shift}{/Control}");
    expect(nameBox().value).toBe("Ember Bolts");
    await user.keyboard("{Control>}2{/Control}");
    expect(within(editor()).getByRole("tab", { name: "RULES" }).getAttribute("aria-selected")).toBe("true");
    await user.keyboard("{Control>}1{/Control}");
    await user.keyboard("{F2}");
    expect(document.activeElement).toBe(nameBox());
    await user.keyboard("{Control>}f{/Control}");
    expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Filter elements" }));
  });

  it("Ctrl+Shift+N makes a new element of the selected kind; Ctrl+D duplicates; Ctrl+N is not taken", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const packId = await packWithSpell(api);
    const newDraft = vi.spyOn(api, "homebrewNewDraft");
    const duplicate = vi.spyOn(api, "homebrewDuplicateElement");
    renderApp(`/homebrew/${packId}`, { api });
    await openSpell(user);

    // Act
    await user.keyboard("{Control>}d{/Control}");
    await waitFor(() => expect(nameBox().value).toBe("Ember Bolt (copy)"));
    await user.keyboard("{Control>}{Shift>}n{/Shift}{/Control}");
    await waitFor(() => expect(newDraft).toHaveBeenCalledWith({ packId, kind: "spell" }));
    await user.keyboard("{Control>}n{/Control}");

    // Assert: Ctrl+N is the shell's "new character" (it leaves for the characters), not a new element
    expect(duplicate).toHaveBeenCalledWith({ packId, elementId: "ID_EMB_SPELL_EMBER_BOLT" });
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeTruthy();
    expect(newDraft).toHaveBeenCalledTimes(1);
  });
});

describe("WB-09 HOMEBREW pack list and workspace", () => {
  it("suggests a pack code from the name", () => {
    expect(suggestCode("Ember Isles Expanded")).toBe("EIE");
    expect(suggestCode("Frostmarch")).toBe("FROS");
    expect(suggestCode("Café of Ages")).toBe("COA");
    expect(suggestCode("")).toBe("");
  });

  it("NEW PACK makes a pack and opens its workspace; the pack list shows it", async () => {
    // Arrange
    const user = userEvent.setup();
    renderApp("/homebrew", { api: mock() });
    expect(await screen.findByText("No homebrew packs yet. Make one with New pack.")).toBeTruthy();

    // Act
    await user.click(screen.getByRole("button", { name: "New pack" }));
    const flyout = await screen.findByRole("dialog", { name: "New pack" });
    await user.type(within(flyout).getByLabelText("Name"), "Ember Isles");
    expect((within(flyout).getByLabelText("Code") as HTMLInputElement).value).toBe("EI");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));

    // Assert
    expect(await screen.findByRole("heading", { name: "Homebrew: Ember Isles" })).toBeTruthy();
    expect(screen.getByRole("navigation", { name: "Element kinds" }).textContent).toContain("Spells (0)");
    await user.click(screen.getByRole("button", { name: "All packs" }));
    const grid = await screen.findByRole("grid", { name: "Homebrew packs" });
    expect(await within(grid).findByText("Ember Isles")).toBeTruthy();
    expect(within(grid).getByText("2024")).toBeTruthy();
  });

  it("a refused code is explained in the flyout", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    await api.homebrewCreatePack({ request: { name: "First", code: "EI", edition: "dnd2024" } });
    renderApp("/homebrew", { api });
    await user.click(await screen.findByRole("button", { name: "New pack" }));
    const flyout = await screen.findByRole("dialog", { name: "New pack" });

    // Act
    await user.type(within(flyout).getByLabelText("Name"), "Ember Isles");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));

    // Assert
    expect((await within(flyout).findByRole("alert")).textContent).toBe("Another pack already uses the code EI.");
  });

  it("new element -> edit -> PROBLEMS jumps to the field -> undo -> save; the list and the live description follow", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
    renderApp(`/homebrew/${pack.id}`, { api });

    // Act: New element (spell) named Ember Bolt
    await user.click(await screen.findByRole("button", { name: "New element" }));
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    expect(within(flyout).getByRole("radio", { name: /^Spell A spell/ }).getAttribute("aria-checked")).toBe("true");
    await user.type(within(flyout).getByLabelText("Name"), "Ember Bolt");
    await user.click(within(flyout).getByRole("button", { name: "Create" }));
    await waitFor(() => expect(nameBox().value).toBe("Ember Bolt"));
    // A new spell asks for its school (WB-11: the other required fields start filled in).
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(1\)/ })).toBeTruthy());
    await user.selectOptions(screen.getByRole("combobox", { name: /^School/ }), "Evocation");
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(0\)/ })).toBeTruthy());

    // The sub-tab band shows the kind's tabs; the description pane shows the element live.
    expect(within(editor()).getAllByRole("tab").map((t) => t.textContent)).toEqual(["GENERAL", "RULES", "DESCRIPTION"]);
    const pane = screen.getAllByRole("complementary", { name: "Description" }).at(-1)!;
    expect(within(pane).getByRole("heading", { name: "Ember Bolt" })).toBeTruthy();

    // Empty the name: the autosave answers with the problem, on the field and in PROBLEMS.
    await user.clear(nameBox());
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(1\)/ })).toBeTruthy());
    expect(nameBox().getAttribute("aria-describedby")).toBeTruthy();
    expect(document.getElementById(nameBox().getAttribute("aria-describedby")!)!.textContent).toBe("Give it a name.");
    expect(screen.getByRole("contentinfo", { name: "Status" }).textContent).toContain("1 problem");
    // PROBLEMS opened by itself for the school; open it if it isn't
    const problemsButton = screen.getByRole("button", { name: /Problems \(1\)/ });
    if (problemsButton.getAttribute("aria-expanded") !== "true") await user.click(problemsButton);
    await user.click(within(screen.getByRole("list", { name: "Problems" })).getByRole("button", { name: "Give it a name." }));
    expect(document.activeElement).toBe(nameBox());

    // Undo brings the name back and the problem goes after the autosave.
    await user.click(within(editor()).getByRole("button", { name: "Undo" }));
    expect(nameBox().value).toBe("Ember Bolt");
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(0\)/ })).toBeTruthy());

    // Save: the element is in the list, counted in CATEGORIES, and the status line says so.
    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Ember Bolt" })).toBeTruthy();
    expect(screen.getByRole("navigation", { name: "Element kinds" }).textContent).toContain("Spells (1)");
    expect(screen.getByRole("contentinfo", { name: "Status" }).textContent).toContain("Saved 'Ember Bolt' to Ember Isles.");
    // The history survives the save: the undone edit can still be redone.
    expect((within(editor()).getByRole("button", { name: "Redo" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("reopening an element with unsaved changes asks Keep editing / Discard", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const packId = await packWithSpell(api);
    const opened = await api.homebrewOpenDraft({ packId, elementId: "ID_EMB_SPELL_EMBER_BOLT" });
    await api.homebrewUpdateDraft({ draftId: opened.draftId, form: { ...opened.form, fields: { ...opened.form.fields, name: "Ember Bolt Plus" } } });
    renderApp(`/homebrew/${packId}`, { api });

    // Act
    await user.click(await screen.findByRole("button", { name: "Ember Bolt" }));
    const box = await screen.findByRole("dialog", { name: "Unsaved changes" });
    await user.click(within(box).getByRole("button", { name: "Keep editing" }));

    // Assert
    expect(box.textContent).toMatch(/Unsaved changes from /);
    await waitFor(() => expect(nameBox().value).toBe("Ember Bolt Plus"));
  });

  it("Discard drops the draft and opens the saved element", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const packId = await packWithSpell(api);
    const opened = await api.homebrewOpenDraft({ packId, elementId: "ID_EMB_SPELL_EMBER_BOLT" });
    await api.homebrewUpdateDraft({ draftId: opened.draftId, form: { ...opened.form, fields: { ...opened.form.fields, name: "Ember Bolt Plus" } } });
    renderApp(`/homebrew/${packId}`, { api });

    // Act
    await user.click(await screen.findByRole("button", { name: "Ember Bolt" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Unsaved changes" })).getByRole("button", { name: "Discard" }));

    // Assert
    await waitFor(() => expect(nameBox().value).toBe("Ember Bolt"));
    expect(await api.homebrewListDrafts()).toEqual([]);
  });

  it("an element changed on another device asks Keep my draft / Use the saved version", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mock();
    const packId = await packWithSpell(api);
    const real = api.homebrewOpenDraft.bind(api);
    let first = true;
    vi.spyOn(api, "homebrewOpenDraft").mockImplementation(async (args) => {
      const v = await real(args);
      if (!first) return v;
      first = false;
      return { ...v, baseChanged: true, form: { ...v.form, fields: { ...v.form.fields, name: "My draft" } } };
    });
    renderApp(`/homebrew/${packId}`, { api });

    // Act
    await user.click(await screen.findByRole("button", { name: "Ember Bolt" }));
    const box = await screen.findByRole("dialog", { name: "Element changed" });
    await user.click(within(box).getByRole("button", { name: "Use the saved version" }));

    // Assert
    await waitFor(() => expect(nameBox().value).toBe("Ember Bolt"));
  });
});
