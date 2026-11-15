import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { EditorState } from "prosemirror-state";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { AppProviders } from "../AppProviders";
import { setPhoneForTests } from "../lib/phone";
import { cellContent, cellDoc, cellPastedText } from "../pages/homebrew/rich/cell";
import { docFromJson, tableCells } from "../pages/homebrew/rich/commands";
import { RichTextEditor } from "../pages/homebrew/rich/RichTextEditor";
import type { CellJson, DocJson, InlineJson, TableRowJson } from "../pages/homebrew/rich/types";
import { mockApi } from "./hc-wb10-harness";

// Homebrew P3 (decision HC12, board item ...H5wuc): the phone cell sheet (spec 16) edits a table cell's bold and italic
// text too. The cell's marks come back exactly (b/strong, i/em kept), Bold and Italic toggle in the sheet, OK writes the
// cell as one step of the description (the shared fixture's tree, whose HTML the Rust writer pins), Cancel writes nothing.

beforeAll(() => {
  // jsdom has no layout: ProseMirror measures ranges when it scrolls the selection into view.
  const rect = { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, toJSON: () => ({}) } as DOMRect;
  const list = Object.assign([rect], { item: () => rect }) as unknown as DOMRectList;
  Range.prototype.getBoundingClientRect = () => rect;
  Range.prototype.getClientRects = () => list;
  if (!document.elementFromPoint) document.elementFromPoint = () => null;
});

afterEach(() => act(() => { setPhoneForTests(false); }));

interface RoundTrip { name: string; html: string; doc: DocJson }
const FIXTURE_NAME = "table cells with bold and italic (phone cell sheet)";
const FIXTURE = (JSON.parse(
  readFileSync(resolve(__dirname, "../../../crates/borealis-content/tests/fixtures/rich_text.json"), "utf8"),
) as { roundtrip: RoundTrip[] }).roundtrip.find((c) => c.name === FIXTURE_NAME)!;

/** The fixture's table rows. */
const rowsOf = (doc: DocJson): TableRowJson[] => {
  const table = doc.content?.[0];
  if (table?.type !== "table") throw new Error("the fixture starts with a table");
  return table.content ?? [];
};

/** The fixture with one cell's content replaced. */
function withCell(row: number, col: number, content: InlineJson[]): DocJson {
  const doc = structuredClone(FIXTURE.doc);
  const cell = rowsOf(doc)[row]!.content![col]! as CellJson;
  cell.content = content;
  return doc;
}

/** The fixture's marked cells: row 2 "Ash" bold + italic, row 3 strong, a line break, em. */
const ASH = rowsOf(FIXTURE.doc)[1]!.content![1]!.content!;
const FIRE = rowsOf(FIXTURE.doc)[2]!.content![1]!.content!;

function setup(value: DocJson) {
  act(() => setPhoneForTests(true));
  const onChange = vi.fn<(doc: DocJson, typing: boolean) => void>();
  render(
    <AppProviders api={mockApi()} restore={false}>
      <RichTextEditor id="desc-input" label="Description" value={value} onChange={onChange} />
    </AppProviders>,
  );
  const toolbar = () => screen.getByRole("toolbar", { name: "Description formatting" });
  const sheet = (name: string) => screen.findByRole("dialog", { name: `Table cell: ${name}` });
  return { onChange, toolbar, sheet };
}

const box = (sheet: HTMLElement) => within(sheet).getByRole("textbox", { name: "Cell text" });
const tools = (sheet: HTMLElement) => within(sheet).getByRole("toolbar", { name: "Cell formatting" });
const selectAll = (sheet: HTMLElement) => fireEvent.keyDown(box(sheet), { key: "a", ctrlKey: true });

describe("HC12 the phone cell sheet edits bold and italic", () => {
  it("a cell's marks come back from the sheet's editor exactly, tags and line breaks kept", () => {
    // Arrange
    const cells = tableCells(EditorState.create({ doc: docFromJson(FIXTURE.doc) }))!;
    for (const cell of cells) {
      // Act
      const back = cellContent(cellDoc(cell.content));
      // Assert
      expect(back.eq(cell.content)).toBe(true);
      expect(back.toJSON()).toEqual(cell.content.toJSON());
    }
    expect(cells[5]!.content.toJSON()).toEqual(FIRE);
  });

  it("pasted plain text in the sheet keeps its lines as line breaks, through the paste rules", () => {
    // Act
    const slice = cellPastedText("1-3\\nlow\r\nhigh  roll");
    // Assert: the literal \\n is fixed into a line break, CRLF is one, space runs are one space
    expect(slice.content.toJSON()).toEqual([
      { type: "text", text: "1-3" }, { type: "hard_break" }, { type: "text", text: "low" }, { type: "hard_break" }, { type: "text", text: "high roll" },
    ]);
    expect(cellPastedText("").size).toBe(0);
  });

  it("Edit cell opens a cell with bold and italic text in the sheet, and OK or Next with no change writes nothing", async () => {
    // Arrange: the cursor starts in the first header cell, which holds strong, a line break and em
    const { onChange, toolbar, sheet } = setup(withCell(0, 0, FIRE));
    const edit = await within(toolbar()).findByRole("button", { name: "Edit cell" });
    expect((edit as HTMLButtonElement).disabled).toBe(false);
    expect(edit.title).toBe("Edit this cell in a sheet");
    // Act
    fireEvent.click(edit);
    const first = await sheet("Header, column 1");
    // Assert: the cell as it is, with Bold and Italic buttons
    expect(box(first).innerHTML).toBe("<strong>Fire</strong><br><em>cold</em> ash");
    expect(box(first).getAttribute("contenteditable")).toBe("true");
    expect(within(tools(first)).getByRole("button", { name: "Bold" }).getAttribute("aria-pressed")).toBe("false");
    expect(within(tools(first)).getByRole("button", { name: "Italic" }).getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(within(first).getByRole("button", { name: "Next cell" }));
    const second = await sheet("Header, column 2");
    expect(box(second).textContent).toBe("Result");
    fireEvent.click(within(second).getByRole("button", { name: "OK" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(onChange).not.toHaveBeenCalled();
  });

  it("Bold and Italic in the sheet write the cell's canonical tree as one step on OK", async () => {
    // Arrange: row 2, column 2 holds "Ash" without marks
    const { onChange, toolbar, sheet } = setup(withCell(1, 1, [{ type: "text", text: "Ash" }]));
    fireEvent.click(await within(toolbar()).findByRole("button", { name: "Edit cell" }));
    for (const name of ["Header, column 1", "Header, column 2", "Row 2, column 1"]) {
      fireEvent.click(within(await sheet(name)).getByRole("button", { name: "Next cell" }));
    }
    const ash = await sheet("Row 2, column 2");
    expect(box(ash).textContent).toBe("Ash");
    // Act
    selectAll(ash);
    fireEvent.click(within(tools(ash)).getByRole("button", { name: "Bold" }));
    await waitFor(() => expect(within(tools(ash)).getByRole("button", { name: "Bold" }).getAttribute("aria-pressed")).toBe("true"));
    selectAll(ash);
    fireEvent.click(within(tools(ash)).getByRole("button", { name: "Italic" }));
    await waitFor(() => expect(within(tools(ash)).getByRole("button", { name: "Italic" }).getAttribute("aria-pressed")).toBe("true"));
    expect(box(ash).innerHTML).toBe("<b><i>Ash</i></b>");
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.click(within(ash).getByRole("button", { name: "OK" }));
    // Assert: one change, an action (one undo step), and exactly the fixture whose HTML the Rust writer pins
    await waitFor(() => expect(onChange).toHaveBeenCalledTimes(1));
    expect(onChange).toHaveBeenLastCalledWith(FIXTURE.doc, false);
    expect(rowsOf(onChange.mock.calls[0]![0])[1]!.content![1]!.content).toEqual(ASH);
  });

  it("Cancel after Bold writes nothing", async () => {
    // Arrange
    const { onChange, toolbar, sheet } = setup(withCell(0, 0, [{ type: "text", text: "d6" }]));
    fireEvent.click(await within(toolbar()).findByRole("button", { name: "Edit cell" }));
    const first = await sheet("Header, column 1");
    // Act
    selectAll(first);
    fireEvent.click(within(tools(first)).getByRole("button", { name: "Bold" }));
    await waitFor(() => expect(box(first).innerHTML).toBe("<b>d6</b>"));
    fireEvent.click(within(first).getByRole("button", { name: "Cancel" }));
    // Assert
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(onChange).not.toHaveBeenCalled();
  });

  it("Enter in the sheet is a line break in the cell, inside the marks", async () => {
    // Arrange
    const { onChange, toolbar, sheet } = setup(withCell(0, 0, [{ type: "text", text: "d6", marks: [{ type: "bold", attrs: { tag: "b" } }] }]));
    fireEvent.click(await within(toolbar()).findByRole("button", { name: "Edit cell" }));
    const first = await sheet("Header, column 1");
    // Act: the cursor opens at the end of the cell
    fireEvent.keyDown(box(first), { key: "Enter" });
    await waitFor(() => expect(box(first).querySelectorAll("br").length).toBeGreaterThan(0));
    fireEvent.click(within(first).getByRole("button", { name: "OK" }));
    // Assert
    await waitFor(() => expect(onChange).toHaveBeenCalledTimes(1));
    expect(rowsOf(onChange.mock.calls[0]![0])[0]!.content![0]!.content).toEqual([
      { type: "text", text: "d6", marks: [{ type: "bold", attrs: { tag: "b" } }] },
      { type: "hard_break", marks: [{ type: "bold", attrs: { tag: "b" } }] },
    ]);
  });
});
