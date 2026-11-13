import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { Fragment } from "prosemirror-model";
import userEvent from "@testing-library/user-event";
import { EditorState } from "prosemirror-state";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { HomebrewRef } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { AppProviders } from "../AppProviders";
import { type HapticsBackend, setHapticsForTests } from "../lib/haptics";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { RefField } from "../pages/homebrew/pickers/RefField";
import { CellSheet } from "../pages/homebrew/rich/CellSheet";
import { cellIndexAt, insertTable, setCellText, tableCells } from "../pages/homebrew/rich/commands";
import { richSchema } from "../pages/homebrew/rich/schema";
import { mockApi, renderControl, Stateful } from "./hc-wb10-harness";
import { renderApp } from "./utils";

// Homebrew WB-25, the phone pass (spec 16): the editor's sticky head (the element's name and the sub-tab strip) and the
// desktop's fixed head, the multi-select picker's check rows, light haptics on save and delete, the LEVELS rows with a
// sheet per level, and table cells edited in a sheet.

afterEach(() => act(() => { setPhoneForTests(false); setPhoneOsForTests(null); setHapticsForTests(null); }));

const editor = () => screen.getByRole("region", { name: "Element editor" });

/** A recording haptics backend (the platform's system setting on). */
function recorder() {
  const calls: string[] = [];
  const backend: HapticsBackend = {
    selection: async () => { calls.push("selection"); },
    impact: async (style) => { calls.push(`impact:${style}`); },
    enabled: async () => true,
  };
  return { calls, backend };
}

async function newElement(user: ReturnType<typeof userEvent.setup>, kind: RegExp, name: string, api: MockApi = mockApi()) {
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  renderApp(`/homebrew/${pack.id}`, { api });
  await user.click((await screen.findAllByRole("button", { name: /^New( element)?$/ }))[0]!);
  const flyout = await screen.findByRole("dialog", { name: "New element" });
  await user.click(within(flyout).getByRole("radio", { name: kind }));
  await user.type(within(flyout).getByLabelText("Name"), name);
  await user.click(within(flyout).getByRole("button", { name: "Create" }));
  await waitFor(() => expect((screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement).value).toBe(name));
  return api;
}

describe("WB-25 the editor's head", () => {
  it("phones: the element's name and the sub-tab strip share one sticky head; PROBLEMS follows it; the commands close the form", async () => {
    // Arrange
    act(() => setPhoneForTests(true));
    const user = userEvent.setup();
    // Act
    await newElement(user, /^Spell A spell/, "Ember Lance");
    // Assert
    const head = editor().querySelector<HTMLElement>(".hb-editor-head")!;
    expect(head).not.toBeNull();
    expect(within(head).getByRole("heading", { name: "Ember Lance" })).toBeTruthy();
    expect(within(head).getByRole("tablist", { name: "Editor sections" })).toBeTruthy();
    expect(head.nextElementSibling?.classList.contains("hb-problems")).toBe(true);
    const commands = within(editor()).getByRole("toolbar", { name: "Element commands" });
    expect(commands.classList.contains("eq-pin")).toBe(true);
    expect(editor().lastElementChild).toBe(commands);
    expect(within(commands).getAllByRole("button").map((b) => b.getAttribute("aria-label"))).toEqual(["Undo", "Redo", "Try it", "Save"]);
  });

  it("desktop: the title band, the commands, PROBLEMS and the sub-tab band come before the form, which scrolls on its own", async () => {
    const user = userEvent.setup();
    await newElement(user, /^Spell A spell/, "Ember Lance");
    const kids = [...editor().children];
    const at = (pred: (e: Element) => boolean) => kids.findIndex(pred);
    const band = at((e) => e.classList.contains("ap-band"));
    // the command row: the commands toolbar, then the Expert switch at its right end (WB-26: the switch covered tabs)
    const commands = at((e) => e.classList.contains("hb-cmdrow") && e.firstElementChild?.getAttribute("aria-label") === "Element commands");
    const problems = at((e) => e.classList.contains("hb-problems"));
    const tabs = at((e) => e.classList.contains("hb-band"));
    const panel = at((e) => e.getAttribute("role") === "tabpanel");
    expect([band, commands, problems, tabs, panel]).toEqual([0, 1, 2, 3, 4]);
    expect(editor().querySelector(".hb-editor-head")).toBeNull();
    expect(kids[commands]!.firstElementChild!.classList.contains("eq-pin")).toBe(false);
    expect(within(kids[commands] as HTMLElement).getByRole("switch", { name: "Expert fields" })).toBeTruthy();
    expect(within(kids[tabs] as HTMLElement).queryByRole("switch")).toBeNull();
  });
});

describe("WB-25 multi-select pickers on phones", () => {
  async function multiField(onValue = vi.fn()) {
    await renderControl(
      <Stateful initial={[] as HomebrewRef[]} onValue={onValue}>
        {(v, set) => <RefField label="Languages" target={{ types: ["Language"], multiple: true }} value={v} onChange={set} />}
      </Stateful>,
    );
    return onValue;
  }

  it("a tap on a row's name checks it and a second tap clears it; the mark is a check box, not a ring", async () => {
    // Arrange
    act(() => setPhoneForTests(true));
    const user = userEvent.setup();
    const onValue = await multiField();
    await user.click(screen.getByRole("button", { name: "Pick: Languages" }));
    const list = await screen.findByRole("list", { name: "Languages options" });
    const row = (name: RegExp) => within(list).getByRole("button", { name }).closest("li")!;
    const mark = (name: RegExp) => row(name).querySelector<HTMLButtonElement>(".pick-mark")!;
    // Act: the name of Dwarvish, the name of Elvish, then Dwarvish's name again
    await user.click(await within(list).findByRole("button", { name: /^Dwarvish/ }));
    await user.click(within(list).getByRole("button", { name: /^Elvish/ }));
    await user.click(within(list).getByRole("button", { name: /^Dwarvish/ }));
    // Assert
    expect(mark(/^Dwarvish/).classList.contains("is-check")).toBe(true);
    // the multi-select mark is a check box to screen readers (WB-26)
    expect(mark(/^Dwarvish/).getAttribute("role")).toBe("checkbox");
    expect(mark(/^Dwarvish/).getAttribute("aria-checked")).toBe("false");
    expect(mark(/^Elvish/).getAttribute("aria-checked")).toBe("true");
    // the check box itself toggles both ways too
    await user.click(mark(/^Elvish/));
    expect(mark(/^Elvish/).getAttribute("aria-checked")).toBe("false");
    await user.click(mark(/^Giant/));
    await user.click(screen.getByRole("button", { name: "OK" }));
    expect(onValue.mock.lastCall![0].map((r: HomebrewRef) => r.name)).toEqual(["Giant"]);
  });

  it("desktop keeps a click for describing: only a double-click (or Enter) checks, and it unchecks too", async () => {
    const user = userEvent.setup();
    const onValue = await multiField();
    await user.click(screen.getByRole("button", { name: "Pick: Languages" }));
    const list = await screen.findByRole("list", { name: "Languages options" });
    const dwarvish = await within(list).findByRole("button", { name: /^Dwarvish/ });
    await user.click(dwarvish);
    expect(dwarvish.getAttribute("aria-pressed")).toBe("false");
    await user.dblClick(dwarvish);
    expect(dwarvish.getAttribute("aria-pressed")).toBe("true");
    await user.dblClick(dwarvish);
    expect(dwarvish.getAttribute("aria-pressed")).toBe("false");
    expect(list.querySelector(".pick-mark")).toBeNull();
    await user.dblClick(within(list).getByRole("button", { name: /^Elvish/ }));
    await user.click(screen.getByRole("button", { name: "OK" }));
    expect(onValue.mock.lastCall![0].map((r: HomebrewRef) => r.name)).toEqual(["Elvish"]);
  });
});

describe("WB-25 haptics (spec 16, O13 B)", () => {
  it("a light haptic on save and on removing a rule", async () => {
    // Arrange
    act(() => { setPhoneForTests(true); setPhoneOsForTests("android"); });
    const { calls, backend } = recorder();
    setHapticsForTests(backend);
    const user = userEvent.setup();
    await newElement(user, /^Spell A spell/, "Ember Lance");
    await user.click(document.querySelector<HTMLButtonElement>("#hb-f-school-input")!);
    await user.click(within(await screen.findByRole("listbox")).getByRole("option", { name: "Evocation" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \(0\)/ })).toBeTruthy());
    // Act: save
    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    // Assert
    await waitFor(() => expect(calls).toEqual(["impact:light"]));
    // Act: a rule, then remove it
    await user.click(within(editor()).getByRole("tab", { name: "RULES" }));
    await user.click(screen.getByRole("button", { name: "Add rule" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Add a rule" })).getByRole("button", { name: "Advantage on a roll" }));
    const dialog = await screen.findByRole("dialog", { name: "New rule: Advantage on a roll" });
    await user.click(within(dialog).getByRole("button", { name: "OK" }));
    await user.click(await screen.findByRole("button", { name: /^Remove: Advantage on a roll/ }));
    await waitFor(() => expect(calls).toEqual(["impact:light", "impact:light"]));
  });
});

describe("WB-25 class LEVELS on phones", () => {
  it("one row per level with its features in the detail line; a tap opens that level's sheet with Add feature", async () => {
    // Arrange
    act(() => setPhoneForTests(true));
    const user = userEvent.setup();
    await newElement(user, /^Class A class/, "Gloom Knight", new MockApi({ creatorEnabled: true }));
    // Act
    await user.click(within(editor()).getByRole("tab", { name: "LEVELS" }));
    const rows = await screen.findByRole("list", { name: "Features by level" });
    // Assert
    expect(screen.queryByRole("table", { name: "Features by level" })).toBeNull();
    const buttons = within(rows).getAllByRole("button");
    expect(buttons).toHaveLength(20);
    expect(buttons[3]!.textContent).toBe("Level 4+2 · Ability Score Improvement");
    expect(buttons[4]!.textContent).toBe("Level 5+3 · No features");
    await user.click(buttons[3]!);
    const sheet = await screen.findByRole("dialog", { name: "Level 4" });
    expect(within(sheet).getByText("Proficiency bonus +2.")).toBeTruthy();
    expect(within(sheet).getByRole("button", { name: "Ability Score Improvement" })).toBeTruthy();
    const add = within(sheet).getByRole("button", { name: "Add a feature at level 4" });
    expect(add.textContent).toBe("Add feature");
    await user.click(add);
    // the feature editor replaces the level's sheet
    expect(await screen.findByRole("dialog", { name: /^Class feature:/ })).toBeTruthy();
    expect(screen.queryByRole("dialog", { name: "Level 4" })).toBeNull();
  });
});

describe("WB-25 table cells in a sheet (spec 16)", () => {
  const withTable = (): EditorState => {
    const state = EditorState.create({ doc: richSchema.node("doc", null, [richSchema.node("paragraph")]) });
    let next = state;
    insertTable(state, (tr) => { next = state.apply(tr); });
    return next;
  };

  it("lists the table's cells in reading order and writes one as text with line breaks", () => {
    // Arrange
    const state = withTable();
    const cells = tableCells(state)!;
    // Assert: a header row and two body rows of two columns; the cursor is in the first cell
    expect(cells.map((c) => [c.row, c.col, c.head])).toEqual([[0, 0, true], [0, 1, true], [1, 0, false], [1, 1, false], [2, 0, false], [2, 1, false]]);
    expect(cellIndexAt(state)).toBe(0);
    // Act
    let next = state;
    setCellText(cells[2]!.pos, "1-3\nlow")(state, (tr) => { next = state.apply(tr); });
    // Assert
    const after = tableCells(next)!;
    expect(after[2]).toMatchObject({ text: "1-3\nlow" });
    expect(cellIndexAt(next)).toBe(2);
    const cell = next.doc.nodeAt(after[2]!.pos)!;
    expect(cell.content.childCount).toBe(3);
    expect(cell.child(1).type.name).toBe("hard_break");
  });

  it("the sheet keeps what was typed on Next and on OK, and nothing on Cancel", async () => {
    // Arrange: a new table's cells ("Column 1", "Column 2" headings)
    act(() => setPhoneForTests(true));
    const cells = tableCells(withTable())!;
    const onWrite = vi.fn();
    const onClose = vi.fn();
    let index = 0;
    const view = () => (
      <AppProviders api={mockApi()} restore={false}>
        <CellSheet cells={cells} index={index} onWrite={onWrite} onIndex={(i) => { index = i; utils.rerender(view()); }} onClose={onClose} />
      </AppProviders>
    );
    const utils = render(view());
    // Act: typing replaces the heading's text (the editor reads its DOM)
    const sheet = await screen.findByRole("dialog", { name: "Table cell: Header, column 1" });
    const box = within(sheet).getByRole("textbox", { name: "Cell text" });
    expect(box.textContent).toBe("Column 1");
    await act(async () => { (box.firstChild as Text).data = "d8"; await new Promise((r) => setTimeout(r, 0)); });
    fireEvent.click(within(sheet).getByRole("button", { name: "Next cell" }));
    // Assert
    expect(onWrite).toHaveBeenCalledTimes(1);
    expect(onWrite.mock.calls[0]![0]).toBe(cells[0]);
    expect((onWrite.mock.calls[0]![1] as Fragment).toJSON()).toEqual([{ type: "text", text: "d8" }]);
    const second = await screen.findByRole("dialog", { name: "Table cell: Header, column 2" });
    const box2 = within(second).getByRole("textbox", { name: "Cell text" });
    expect(box2.textContent).toBe("Column 2");
    await act(async () => { (box2.firstChild as Text).data = "Result"; await new Promise((r) => setTimeout(r, 0)); });
    fireEvent.click(within(second).getByRole("button", { name: "Cancel" }));
    expect(onWrite).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe("WB-25 export on a phone (spec 16: the share sheet)", () => {
  afterEach(() => { Reflect.deleteProperty(navigator, "share"); Reflect.deleteProperty(navigator, "canShare"); });

  it("the second Share hands the share sheet the pack file only (a title became a stray text.txt on iOS)", async () => {
    // Arrange
    const share = vi.fn(async (_data: { files: File[]; title?: string }) => undefined);
    Object.defineProperty(navigator, "share", { value: share, configurable: true });
    Object.defineProperty(navigator, "canShare", { value: () => true, configurable: true });
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, creatorEnabled: true, mobile: true });
    await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
    renderApp("/homebrew", { api });
    await user.click(await screen.findByRole("row", { name: /Ember Isles/ }));
    await user.click(screen.getByRole("button", { name: "Export pack" }));
    const dialog = await screen.findByRole("dialog", { name: "Export pack" });
    // Act: the first Share makes the file, the second shares it
    await user.click(within(dialog).getByRole("button", { name: "Share" }));
    await within(dialog).findByText("Ember_Isles.borealis-pack.zip is ready to share.");
    await user.click(within(dialog).getByRole("button", { name: "Share" }));
    // Assert
    await waitFor(() => expect(share).toHaveBeenCalledTimes(1));
    const data = share.mock.calls[0]![0];
    expect(Object.keys(data)).toEqual(["files"]);
    expect(data.files.map((f) => f.name)).toEqual(["Ember_Isles.borealis-pack.zip"]);
  });
});
