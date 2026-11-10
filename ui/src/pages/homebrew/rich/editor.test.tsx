import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { EditorState, TextSelection, type Command } from "prosemirror-state";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { setPhoneForTests } from "../../../lib/phone";
import {
  ACTION_META, addTableColumn, addTableRow, applyCommand, asAction, breakInCell, deleteTable, docFromJson, exitEmptyListItem,
  higherLevelLead, insertHardBreak, insertTable, isMarkActive, toggleBold, toggleHeading, toggleItalic, toggleList,
} from "./commands";
import { fixDocJson, normalizeDocJson } from "./doc";
import { cleanPastedHtml } from "./paste";
import { RichTextEditor, escapeWarning, isTyping, sameDescription, type RichTextEditorProps } from "./RichTextEditor";
import { richSchema } from "./schema";
import type { BlockJson, DocJson, InlineJson } from "./types";

// Homebrew WB-10: the description editor (spec 7.1, 7.3, 13.13, 16, 17). Commands are tested on plain EditorStates; the
// component is tested through its DOM (toolbar, kept blocks, lint line, paste event, typing).

beforeAll(() => {
  // jsdom has no layout: ProseMirror measures ranges when it scrolls the selection into view.
  const rect = { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, toJSON: () => ({}) } as DOMRect;
  const list = Object.assign([rect], { item: () => rect }) as unknown as DOMRectList;
  Range.prototype.getBoundingClientRect = () => rect;
  Range.prototype.getClientRects = () => list;
  if (!document.elementFromPoint) document.elementFromPoint = () => null;
});

afterEach(() => { act(() => setPhoneForTests(false)); });

const para = (text: string, joined = false): BlockJson => ({ type: "paragraph", attrs: { joined, bare: false }, content: text ? [{ type: "text", text }] : undefined });
const inlinePara = (content: InlineJson[]): BlockJson => ({ type: "paragraph", attrs: { joined: false, bare: false }, content });
const docOf = (...content: BlockJson[]): DocJson => ({ type: "doc", content });
const norm = (d: DocJson) => normalizeDocJson(d);

function stateOf(doc: DocJson, from?: number, to?: number): EditorState {
  const node = docFromJson(doc);
  const state = EditorState.create({ doc: node });
  if (from === undefined) return state;
  return state.apply(state.tr.setSelection(TextSelection.create(node, from, to ?? from)));
}

function run(state: EditorState, cmd: Command): DocJson {
  const result = applyCommand(state, cmd);
  expect(result.applied).toBe(true);
  return result.state.doc.toJSON() as DocJson;
}

function setup(over: Partial<RichTextEditorProps> = {}) {
  const onChange = vi.fn<(doc: DocJson, typing: boolean) => void>();
  const props: RichTextEditorProps = { id: "desc-input", label: "Description", value: docOf(para("Hello world")), onChange, ...over };
  const utils = render(<RichTextEditor {...props} />);
  const box = () => screen.getByRole("textbox", { name: "Description" });
  const toolbar = () => screen.getByRole("toolbar", { name: "Description formatting" });
  const button = (name: string) => within(toolbar()).getByRole("button", { name });
  const last = () => onChange.mock.calls[onChange.mock.calls.length - 1];
  return { ...utils, props, onChange, box, toolbar, button, last };
}

describe("commands", () => {
  it("bold and italic mark the selection and report it", () => {
    const state = stateOf(docOf(para("Hello world")), 1, 6);
    const bold = run(state, toggleBold);
    expect(bold.content?.[0]).toEqual({
      type: "paragraph", attrs: { joined: false, bare: false },
      content: [{ type: "text", text: "Hello", marks: [{ type: "bold", attrs: { tag: "b" } }] }, { type: "text", text: " world" }],
    });
    const after = stateOf(bold, 2, 4);
    expect(isMarkActive(after, richSchema.marks.bold)).toBe(true);
    expect(isMarkActive(after, richSchema.marks.italic)).toBe(false);
    const both = run(after, toggleItalic);
    expect(both.content?.[0]?.type === "paragraph" && both.content[0].content?.[1]).toEqual({
      type: "text", text: "el", marks: [{ type: "bold", attrs: { tag: "b" } }, { type: "italic", attrs: { tag: "i" } }],
    });
  });

  it("heading and subheading toggle the block and back", () => {
    const h3 = run(stateOf(docOf(para("Title")), 2), toggleHeading(3));
    expect(h3.content?.[0]).toEqual({ type: "heading", attrs: { joined: false, level: 3 }, content: [{ type: "text", text: "Title" }] });
    const h4 = run(stateOf(h3, 2), toggleHeading(4));
    expect(h4.content?.[0]?.attrs).toEqual({ joined: false, level: 4 });
    expect(run(stateOf(h4, 2), toggleHeading(4)).content?.[0]?.type).toBe("paragraph");
  });

  it("lists wrap paragraphs, switch kind and unwrap", () => {
    const start = docOf(para("One"), para("Two"), para("Three"));
    // positions: One 1..4, Two 6..9, Three 11..16
    const wrapped = run(stateOf(start, 2, 7), toggleList("bullet_list"));
    expect(wrapped.content?.map((b) => b.type)).toEqual(["bullet_list", "paragraph"]);
    const list = wrapped.content?.[0];
    expect(list?.type === "bullet_list" && list.content?.map((i) => i.content?.[0])).toEqual([{ type: "text", text: "One" }, { type: "text", text: "Two" }]);
    const ordered = run(stateOf(wrapped, 3), toggleList("ordered_list"));
    expect(ordered.content?.[0]?.type).toBe("ordered_list");
    // cursor in the second item: the list splits around it
    const back = run(stateOf(ordered, 8), toggleList("ordered_list"));
    expect(back.content?.map((b) => b.type)).toEqual(["ordered_list", "paragraph", "paragraph"]);
    expect(back.content?.[1]).toEqual(para("Two"));
  });

  it("Enter in an empty list item leaves the list; Enter in a cell is a line break", () => {
    const list: DocJson = docOf({ type: "bullet_list", attrs: { joined: false }, content: [{ type: "list_item", content: [{ type: "text", text: "A" }] }, { type: "list_item" }] });
    const out = run(stateOf(list, 5), exitEmptyListItem);
    expect(out.content?.map((b) => b.type)).toEqual(["bullet_list", "paragraph"]);
    expect(applyCommand(stateOf(list, 3), exitEmptyListItem).applied).toBe(false);

    const table = run(stateOf(docOf(para("")), 1), insertTable);
    const broke = run(stateOf(table, 3), breakInCell);
    const t = broke.content?.[0];
    expect(t?.type === "table" && t.content?.[0]?.content?.[0]).toEqual({ type: "table_header", content: [{ type: "hard_break" }] });
    expect(applyCommand(stateOf(docOf(para("x")), 1), breakInCell).applied).toBe(false);
    const hard = run(stateOf(docOf(para("ab")), 2), insertHardBreak);
    expect(hard.content?.[0]).toEqual({ ...para("a"), content: [{ type: "text", text: "a" }, { type: "hard_break" }, { type: "text", text: "b" }] });
  });

  it("a new table has a header row; rows and columns are added; the table is deleted", () => {
    const table = run(stateOf(docOf(para("Intro")), 2), insertTable);
    expect(table.content?.map((b) => b.type)).toEqual(["paragraph", "table"]);
    const t = table.content?.[1];
    if (t?.type !== "table") throw new Error("no table");
    expect(t.attrs).toEqual({ joined: false, sections: true });
    expect(t.content?.map((r) => [r.attrs.head, r.content?.map((c) => c.type)])).toEqual([
      [true, ["table_header", "table_header"]],
      [false, ["table_cell", "table_cell"]],
      [false, ["table_cell", "table_cell"]],
    ]);
    const inserted = applyCommand(stateOf(docOf(para("Intro")), 2), insertTable).state;
    expect(inserted.selection.$from.parent.type.name).toBe("table_header");

    const rowed = applyCommand(inserted, addTableRow);
    const r = rowed.state.doc.toJSON().content[1];
    expect(r.content.map((row: { attrs: { head: boolean } }) => row.attrs.head)).toEqual([true, false, false, false]);
    expect(rowed.state.selection.$from.parent.type.name).toBe("table_cell");

    const coled = run(inserted, addTableColumn);
    const c = coled.content?.[1];
    expect(c?.type === "table" && c.content?.map((row) => row.content?.map((cell) => cell.type))).toEqual([
      ["table_header", "table_header", "table_header"],
      ["table_cell", "table_cell", "table_cell"],
      ["table_cell", "table_cell", "table_cell"],
    ]);

    expect(run(inserted, deleteTable)).toEqual(docOf(para("Intro")));
    const lone = run(applyCommand(stateOf(docOf(para("")), 1), insertTable).state, deleteTable);
    expect(lone).toEqual(docOf(para("")));
    expect(applyCommand(inserted, insertTable).applied).toBe(false);
  });

  it("toolbar and key actions are marked as actions, typing is not", () => {
    const state = stateOf(docOf(para("Hi")), 1, 3);
    const action = applyCommand(state, asAction(toggleBold)).tr;
    expect(action?.getMeta(ACTION_META)).toBe(true);
    expect(isTyping(action!)).toBe(false);
    expect(isTyping(state.tr.insertText("x"))).toBe(true);
    expect(isTyping(state.tr.insertText("x").setMeta("uiEvent", "paste"))).toBe(false);
  });

  it("names the artefacts in the warning line", () => {
    expect(escapeWarning([
      { path: [0, 0], kind: "backslash_escape", found: "\\n", replacement: "\n" },
      { path: [0, 0], kind: "html_entity", found: "&amp;", replacement: "&" },
      { path: [1, 0], kind: "backslash_escape", found: "\\n", replacement: "\n" },
    ])).toBe("Looks like pasted escape characters: \\n, &amp;");
  });

  it("treats an empty value and one empty paragraph as the same description", () => {
    expect(docFromJson(null).toJSON()).toEqual(docOf(para("")));
    expect(docFromJson({ type: "doc" }).toJSON()).toEqual(docOf(para("")));
    expect(sameDescription(undefined, docFromJson(docOf(para(""))))).toBe(true);
    expect(sameDescription(docOf(para("a")), docFromJson(docOf(para("b"))))).toBe(false);
  });
});

describe("RichTextEditor", () => {
  it("renders the value as an accessible multiline textbox", () => {
    const value = docOf({ type: "heading", attrs: { joined: false, level: 3 }, content: [{ type: "text", text: "Effect" }] }, para("Hello world"));
    const { box } = setup({ value, describedBy: "desc-msg", invalid: true });
    const el = box();
    expect(el.id).toBe("desc-input");
    expect(el.getAttribute("aria-multiline")).toBe("true");
    expect(el.getAttribute("aria-describedby")).toBe("desc-msg");
    expect(el.getAttribute("aria-invalid")).toBe("true");
    expect(el.getAttribute("contenteditable")).toBe("true");
    expect(el.querySelector("h3")?.textContent).toBe("Effect");
    expect(el.querySelector("p")?.textContent).toBe("Hello world");
  });

  it("calls onChange with typing=true when text is typed into the DOM", async () => {
    const { box, onChange, last } = setup();
    const text = box().querySelector("p")?.firstChild as Text;
    await act(async () => {
      text.data = "Hello worlds";
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    await waitFor(() => expect(onChange).toHaveBeenCalled());
    expect(last()?.[1]).toBe(true);
    expect(last()?.[0]).toEqual(docOf(para("Hello worlds")));
  });

  it("toolbar buttons are toggle buttons with tooltips; aria-pressed follows the formatting", async () => {
    const { button, onChange, last } = setup();
    for (const name of ["Bold", "Italic", "Heading", "Subheading", "Bulleted list", "Numbered list"]) {
      expect(button(name).getAttribute("aria-pressed")).toBe("false");
      expect(button(name).title).not.toBe("");
    }
    expect(button("Insert table").hasAttribute("aria-pressed")).toBe(false);
    // the cursor starts at the beginning: Bold sets the stored mark, no document change
    fireEvent.click(button("Bold"));
    await waitFor(() => expect(button("Bold").getAttribute("aria-pressed")).toBe("true"));
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.click(button("Heading"));
    await waitFor(() => expect(button("Heading").getAttribute("aria-pressed")).toBe("true"));
    expect(last()).toEqual([docOf({ type: "heading", attrs: { joined: false, level: 3 }, content: [{ type: "text", text: "Hello world" }] }), false]);
  });

  it("list and table buttons produce the expected tree", async () => {
    const { button, last } = setup();
    fireEvent.click(button("Bulleted list"));
    await waitFor(() => expect(button("Bulleted list").getAttribute("aria-pressed")).toBe("true"));
    expect(last()).toEqual([docOf({ type: "bullet_list", attrs: { joined: false }, content: [{ type: "list_item", content: [{ type: "text", text: "Hello world" }] }] }), false]);
    fireEvent.click(button("Numbered list"));
    await waitFor(() => expect(last()?.[0].content?.[0]?.type).toBe("ordered_list"));

    fireEvent.click(button("Insert table"));
    await waitFor(() => expect(within(screen.getByRole("toolbar")).getByRole("button", { name: "Add table row" })).toBeTruthy());
    const doc = last()?.[0];
    expect(doc?.content?.map((b) => b.type)).toEqual(["ordered_list", "table"]);
    fireEvent.click(button("Add table row"));
    await waitFor(() => {
      const t = last()?.[0].content?.[1];
      expect(t?.type === "table" && t.content?.length).toBe(4);
    });
    fireEvent.click(button("Add table column"));
    await waitFor(() => {
      const t = last()?.[0].content?.[1];
      expect(t?.type === "table" && t.content?.[0]?.content?.length).toBe(3);
    });
    fireEvent.click(button("Delete table"));
    await waitFor(() => expect(last()?.[0].content?.map((b) => b.type)).toEqual(["ordered_list"]));
    expect(button("Insert table")).toBeTruthy();
  });

  it("re-syncs when the value changes from outside, without calling onChange", () => {
    const { box, rerender, props, onChange } = setup();
    rerender(<RichTextEditor {...props} value={docOf(para("Undone text"))} />);
    expect(box().textContent).toBe("Undone text");
    rerender(<RichTextEditor {...props} value={null} />);
    expect(box().querySelectorAll("p")).toHaveLength(1);
    expect(box().textContent).toBe("");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("shows kept blocks read-only with a label, and Remove deletes them", async () => {
    const kept: BlockJson = { type: "kept", attrs: { joined: false, html: '<p class="table-text">Kept words</p>', reason: "outside_palette" } };
    const { box, onChange, last } = setup({ value: docOf(para("Before"), kept) });
    const block = box().querySelector(".rich-kept-block") as HTMLElement;
    expect(block.getAttribute("contenteditable")).toBe("false");
    expect(within(block).getByText("Kept as is")).toBeTruthy();
    expect(within(block).getByText("Kept words").tagName).toBe("P");
    expect(block.querySelector("[class]")?.className).not.toContain("table-text");
    fireEvent.click(within(block).getByRole("button", { name: "Remove kept part" }));
    await waitFor(() => expect(onChange).toHaveBeenCalled());
    expect(last()).toEqual([docOf(para("Before")), false]);
    expect(box().querySelector(".rich-kept-block")).toBeNull();
  });

  it("warns about pasted escape characters and fixes them in one step", async () => {
    const value = docOf(para("Line one\\nLine two &amp;amp; more"));
    const { onChange, last } = setup({ value });
    const line = screen.getByRole("status");
    expect(line.textContent).toContain("Looks like pasted escape characters: \\n, &amp;");
    fireEvent.click(within(line).getByRole("button", { name: "Fix" }));
    await waitFor(() => expect(onChange).toHaveBeenCalledTimes(1));
    const [doc, typing] = last()!;
    expect(typing).toBe(false);
    expect(norm(doc)).toEqual(fixDocJson(value));
    expect(doc.content?.[0]).toEqual({
      ...para(""), content: [{ type: "text", text: "Line one" }, { type: "hard_break" }, { type: "text", text: "Line two & more" }],
    });
    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
  });

  it("pastes Word HTML through the paste rules (list kept, styles dropped)", async () => {
    const word =
      "<p class=MsoListParagraph style='mso-list:l0 level1 lfo1'><span style='mso-list:Ignore'>·<span>  </span></span><span style='font-family:Calibri'>First</span></p>" +
      "<p class=MsoListParagraph style='mso-list:l0 level1 lfo1'><span style='mso-list:Ignore'>·<span>  </span></span><b>Second</b></p>";
    expect(cleanPastedHtml(word)).toBe("<ul><li><span style=\"font-family:Calibri\">First</span></li><li><b>Second</b></li></ul>");
    const { box, onChange, last } = setup({ value: null });
    const event = new Event("paste", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "clipboardData", {
      value: { types: ["text/html", "text/plain"], getData: (t: string) => (t === "text/html" ? word : t === "text/plain" ? "First\nSecond" : "") },
    });
    act(() => { box().dispatchEvent(event); });
    await waitFor(() => expect(onChange).toHaveBeenCalled());
    const [doc, typing] = last()!;
    expect(typing).toBe(false);
    expect(norm(doc)).toEqual(docOf({
      type: "bullet_list", attrs: { joined: false }, content: [
        { type: "list_item", content: [{ type: "text", text: "First" }] },
        { type: "list_item", content: [{ type: "text", text: "Second", marks: [{ type: "bold", attrs: { tag: "b" } }] }] },
      ],
    }));
  });

  it.each([
    ["dnd2024", "Using a Higher-Level Spell Slot."],
    ["dnd2014", "At Higher Levels."],
  ] as const)("the spell extra adds the higher-level paragraph at the end (%s)", async (edition, lead) => {
    expect(higherLevelLead(edition)).toBe(lead);
    const { button, last } = setup({ extras: "spell", edition });
    fireEvent.click(button("Higher-level slots"));
    await waitFor(() => expect(last()).toBeDefined());
    expect(last()).toEqual([docOf(para("Hello world"), inlinePara([
      { type: "text", text: lead, marks: [{ type: "bold", attrs: { tag: "b" } }] }, { type: "text", text: " " },
    ])), false]);
  });

  it("the class extra is shown but waits for the class levels grid", () => {
    const { button, onChange } = setup({ extras: "class" });
    const b = button("Insert class table");
    expect(b.getAttribute("aria-disabled")).toBe("true");
    expect(b.title).toMatch(/class levels grid/);
    fireEvent.click(b);
    expect(onChange).not.toHaveBeenCalled();
    expect(within(screen.getByRole("toolbar")).queryByRole("button", { name: "Higher-level slots" })).toBeNull();
  });

  it("on phones the toolbar docks while the editor has focus", () => {
    act(() => setPhoneForTests(true));
    const { box, container } = setup();
    const root = container.querySelector(".rich-editor") as HTMLElement;
    expect(root.classList.contains("rich-docked")).toBe(false);
    fireEvent.focus(box());
    expect(root.classList.contains("rich-docked")).toBe(true);
    fireEvent.blur(box(), { relatedTarget: null });
    expect(root.classList.contains("rich-docked")).toBe(false);
  });
});
