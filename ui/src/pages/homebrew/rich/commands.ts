import { chainCommands, createParagraphNear, liftEmptyBlock, newlineInCode, setBlockType, splitBlock, toggleMark } from "prosemirror-commands";
import { Fragment, type MarkType, type Node as PMNode, type ResolvedPos } from "prosemirror-model";
import { EditorState, NodeSelection, Selection, TextSelection, type Command, type Transaction } from "prosemirror-state";
import { richSchema } from "./schema";
import type { DocJson } from "./types";

// Editing commands of the description editor (homebrew creator spec 7.1), pure over `EditorState` so the toolbar, the
// keymap and the tests share them. Undo and redo are not here: they belong to the creator's own history (spec 10.2),
// which hands the editor a new `value`.

const { nodes, marks } = richSchema;

/** Transaction meta set by toolbar actions, formatting keys, Fix and Remove: the change is not typing (one undo step). */
export const ACTION_META = "richAction";

/** Marks a command's transactions as an action (not typing). */
export function asAction(cmd: Command): Command {
  return (state, dispatch, view) => cmd(state, dispatch && ((tr) => dispatch(tr.setMeta(ACTION_META, true))), view);
}

/** An empty paragraph, the editor's stand-in for an empty description. */
function emptyParagraph(): PMNode {
  return nodes.paragraph.create();
}

/** The editor document for a stored value: an empty or missing description is one empty paragraph. */
export function docFromJson(value: DocJson | null | undefined): PMNode {
  const doc = value && Array.isArray(value.content) && value.content.length > 0 ? richSchema.nodeFromJSON(value) : null;
  return doc ?? nodes.doc.create(null, emptyParagraph());
}

function ensureNotEmpty(tr: Transaction): Transaction {
  return tr.doc.childCount === 0 ? tr.insert(0, emptyParagraph()) : tr;
}

// ---- marks ---------------------------------------------------------------------------------------------------------

/** Whether `type` is on at the cursor (stored marks) or over the whole selection. */
export function isMarkActive(state: EditorState, type: MarkType): boolean {
  const { from, to, empty, $from } = state.selection;
  if (empty) return Boolean(type.isInSet(state.storedMarks ?? $from.marks()));
  let anyText = false;
  let all = true;
  state.doc.nodesBetween(from, to, (node) => {
    if (!node.isText) return true;
    anyText = true;
    if (!type.isInSet(node.marks)) all = false;
    return false;
  });
  return anyText && all;
}

export const toggleBold: Command = toggleMark(marks.bold);
export const toggleItalic: Command = toggleMark(marks.italic);

// ---- headings ------------------------------------------------------------------------------------------------------

/** The textblock holding the selection start. */
function textblockAt(state: EditorState): PMNode | null {
  const parent = state.selection.$from.parent;
  return parent.isTextblock ? parent : null;
}

/** Whether the selection is in a heading of `level` (3 Heading, 4 Subheading). */
export function isHeadingActive(state: EditorState, level: 3 | 4): boolean {
  const block = textblockAt(state);
  return block?.type === nodes.heading && block.attrs.level === level;
}

/** Heading (3) or Subheading (4) on the selected paragraphs; on a heading of that level, back to a paragraph. */
export function toggleHeading(level: 3 | 4): Command {
  return (state, dispatch) => {
    const joined = Boolean(textblockAt(state)?.attrs.joined);
    if (isHeadingActive(state, level)) return setBlockType(nodes.paragraph, { joined })(state, dispatch);
    return setBlockType(nodes.heading, { joined, level })(state, dispatch);
  };
}

// ---- lists ---------------------------------------------------------------------------------------------------------

type ListType = "bullet_list" | "ordered_list";

/** The list holding the selection (its depth), or null. */
function listAt($pos: ResolvedPos): { node: PMNode; depth: number } | null {
  for (let d = $pos.depth; d > 0; d -= 1) {
    const node = $pos.node(d);
    if (node.type === nodes.bullet_list || node.type === nodes.ordered_list) return { node, depth: d };
  }
  return null;
}

/** Whether the selection is in a list of `type`. */
export function isListActive(state: EditorState, type: ListType): boolean {
  return listAt(state.selection.$from)?.node.type.name === type;
}

/** The selected items of the list at `depth` become paragraphs, splitting the list around them. */
function unwrapItems(state: EditorState, depth: number): Transaction {
  const { $from, $to } = state.selection;
  const list = $from.node(depth);
  const first = $from.index(depth);
  const last = $to.depth >= depth && $to.node(depth) === list ? $to.index(depth) : first;
  const before: PMNode[] = [];
  const middle: PMNode[] = [];
  const after: PMNode[] = [];
  list.forEach((item, _, i) => {
    if (i < first) before.push(item);
    else if (i > last) after.push(item);
    else middle.push(nodes.paragraph.create(null, item.content));
  });
  const head = before.length > 0 ? list.copy(Fragment.fromArray(before)) : null;
  const parts: PMNode[] = head ? [head, ...middle] : [...middle];
  if (after.length > 0) parts.push(list.type.create({ joined: false }, Fragment.fromArray(after)));
  const start = $from.before(depth);
  const tr = state.tr.replaceWith(start, start + list.nodeSize, parts);
  const firstPara = start + (head ? head.nodeSize : 0);
  return tr.setSelection(TextSelection.near(tr.doc.resolve(firstPara + 1)));
}

/** Bulleted or numbered list: wraps the selected paragraphs and headings, switches a list of the other kind, or turns
 * the selected items of a list of this kind back into paragraphs. */
export function toggleList(type: ListType): Command {
  const listType = nodes[type];
  return (state, dispatch) => {
    const { $from, $to } = state.selection;
    const list = listAt($from);
    if (list) {
      if (dispatch) {
        dispatch(list.node.type === listType
          ? unwrapItems(state, list.depth)
          : state.tr.setNodeMarkup($from.before(list.depth), listType, list.node.attrs).scrollIntoView());
      }
      return true;
    }
    const range = $from.blockRange($to);
    if (!range || !(range.parent.type === nodes.doc || range.parent.type === nodes.blockquote)) return false;
    const items: PMNode[] = [];
    for (let i = range.startIndex; i < range.endIndex; i += 1) {
      const block = range.parent.child(i);
      if (block.type !== nodes.paragraph && block.type !== nodes.heading) return false;
      items.push(nodes.list_item.create(null, block.content));
    }
    if (dispatch) {
      const joined = Boolean(range.parent.child(range.startIndex).attrs.joined);
      const wrapped = listType.create({ joined }, items);
      const tr = state.tr.replaceWith(range.start, range.end, wrapped);
      const cursor = Math.min($from.pos + 1, range.start + wrapped.nodeSize - 1);
      dispatch(tr.setSelection(TextSelection.near(tr.doc.resolve(cursor))).scrollIntoView());
    }
    return true;
  };
}

/** Enter in an empty list item leaves the list (the item becomes a paragraph). */
export const exitEmptyListItem: Command = (state, dispatch) => {
  const { $from, empty } = state.selection;
  if (!empty || $from.parent.type !== nodes.list_item || $from.parent.content.size > 0) return false;
  const list = listAt($from);
  if (!list) return false;
  if (dispatch) dispatch(unwrapItems(state, list.depth).scrollIntoView());
  return true;
};

// ---- line breaks ---------------------------------------------------------------------------------------------------

/** A hard line break (`<br />`) at the cursor (Shift+Enter, and Enter in a table cell). */
export const insertHardBreak: Command = (state, dispatch) => {
  if (!state.selection.$from.parent.inlineContent) return false;
  if (dispatch) dispatch(state.tr.replaceSelectionWith(nodes.hard_break.create()).scrollIntoView());
  return true;
};

function inCell($pos: ResolvedPos): boolean {
  const t = $pos.parent.type;
  return t === nodes.table_cell || t === nodes.table_header;
}

/** Enter in a table cell is a line break inside the cell (a cell never splits into a new cell). */
export const breakInCell: Command = (state, dispatch) => (inCell(state.selection.$from) ? insertHardBreak(state, dispatch) : false);

// ---- tables --------------------------------------------------------------------------------------------------------

/** The table holding the selection: its node, position, the row index and cell index. */
export function tableAt(state: EditorState): { table: PMNode; pos: number; row: number; col: number } | null {
  const $from = state.selection.$from;
  for (let d = $from.depth; d > 0; d -= 1) {
    if ($from.node(d).type !== nodes.table) continue;
    return {
      table: $from.node(d),
      pos: $from.before(d),
      row: $from.index(d),
      col: d + 1 <= $from.depth ? $from.index(d + 1) : 0,
    };
  }
  return null;
}

/** Rows and columns of a new table: a header row and two rows of two columns (`d6 | Result` shape). */
export const NEW_TABLE_ROWS = 3;
export const NEW_TABLE_COLUMNS = 2;

/** A new heading cell's text (P3: an empty header cell is an empty `th` in the preview and DATA until the author types). */
export const columnHeading = (index: number): string => `Column ${index + 1}`;
const headingCell = (index: number): PMNode => nodes.table_header.create(null, richSchema.text(columnHeading(index)));
/** The first "Column N" a row's cells do not use yet, as an index (a column added between others). */
const freeHeading = (cells: readonly PMNode[]): number => {
  const used = new Set(cells.map((c) => c.textContent));
  let i = 0;
  while (used.has(columnHeading(i))) i += 1;
  return i;
};

function newTable(): PMNode {
  const row = (head: boolean) =>
    nodes.table_row.create({ head }, Array.from({ length: NEW_TABLE_COLUMNS }, (_, i) => (head ? headingCell(i) : nodes.table_cell.create())));
  return nodes.table.create({ joined: false, sections: true }, Array.from({ length: NEW_TABLE_ROWS }, (_, i) => row(i === 0)));
}

/** Where a new top-level block goes: replacing the current empty paragraph, or after the current top-level block. */
function topLevelSlot(state: EditorState): { from: number; to: number } | null {
  const sel = state.selection;
  if (sel instanceof NodeSelection && sel.$from.depth === 0) return { from: sel.to, to: sel.to };
  const $from = sel.$from;
  if ($from.depth < 1) return null;
  const block = $from.node(1);
  if (block.type === nodes.paragraph && block.content.size === 0) return { from: $from.before(1), to: $from.after(1) };
  return { from: $from.after(1), to: $from.after(1) };
}

/** Insert a table (header row + body rows) after the current top-level block; the first heading is selected, so typing
 * replaces it. */
export const insertTable: Command = (state, dispatch) => {
  if (tableAt(state)) return false;
  const slot = topLevelSlot(state);
  if (!slot) return false;
  if (dispatch) {
    const tr = state.tr.replaceWith(slot.from, slot.to, newTable());
    const first = slot.from + 3;
    dispatch(tr.setSelection(TextSelection.create(tr.doc, first, first + columnHeading(0).length)).scrollIntoView());
  }
  return true;
};

/** A body row after the current row (after the header rows when the cursor is in one), as wide as that row. */
export const addTableRow: Command = (state, dispatch) => {
  const at = tableAt(state);
  if (!at) return false;
  if (dispatch) {
    let index = at.row;
    while (index + 1 < at.table.childCount && at.table.child(index).attrs.head && at.table.child(index + 1).attrs.head) index += 1;
    const width = Math.max(1, at.table.child(at.row).childCount);
    const row = nodes.table_row.create({ head: false }, Array.from({ length: width }, () => nodes.table_cell.create()));
    let pos = at.pos + 1;
    for (let i = 0; i <= index; i += 1) pos += at.table.child(i).nodeSize;
    const tr = state.tr.insert(pos, row);
    dispatch(tr.setSelection(TextSelection.near(tr.doc.resolve(pos + 2))).scrollIntoView());
  }
  return true;
};

/** A column after the current cell, in every row (header cells in header rows). */
export const addTableColumn: Command = (state, dispatch) => {
  const at = tableAt(state);
  if (!at) return false;
  if (dispatch) {
    const rows: PMNode[] = [];
    at.table.forEach((row) => {
      const cells: PMNode[] = [];
      row.forEach((cell) => cells.push(cell));
      const index = Math.min(at.col + 1, cells.length);
      const like = cells[Math.min(at.col, cells.length - 1)];
      const type = like ? like.type : row.attrs.head ? nodes.table_header : nodes.table_cell;
      cells.splice(index, 0, type === nodes.table_header ? headingCell(freeHeading(cells)) : type.create());
      rows.push(row.copy(Fragment.fromArray(cells)));
    });
    const tr = state.tr.replaceWith(at.pos, at.pos + at.table.nodeSize, at.table.copy(Fragment.fromArray(rows)));
    dispatch(tr.setSelection(Selection.near(tr.doc.resolve(Math.min(state.selection.from, tr.doc.content.size)))).scrollIntoView());
  }
  return true;
};

/** Remove the table holding the cursor. */
export const deleteTable: Command = (state, dispatch) => {
  const at = tableAt(state);
  if (!at) return false;
  if (dispatch) {
    const tr = ensureNotEmpty(state.tr.delete(at.pos, at.pos + at.table.nodeSize));
    dispatch(tr.setSelection(Selection.near(tr.doc.resolve(Math.min(at.pos, tr.doc.content.size)))).scrollIntoView());
  }
  return true;
};

// ---- extras --------------------------------------------------------------------------------------------------------

export type Edition = "dnd2014" | "dnd2024";

/** The lead-in of a spell's higher-level paragraph, per edition (the SRD wording). */
export function higherLevelLead(edition: Edition | null | undefined): string {
  return edition === "dnd2014" ? "At Higher Levels." : "Using a Higher-Level Spell Slot.";
}

/** Append `<p><b>{lead}</b> </p>` at the end (replacing a lone empty paragraph); the cursor goes after it. */
export function insertHigherLevel(edition: Edition | null | undefined): Command {
  return (state, dispatch) => {
    if (dispatch) {
      const para = nodes.paragraph.create(null, [
        richSchema.text(higherLevelLead(edition), [marks.bold.create()]),
        richSchema.text(" "),
      ]);
      const doc = state.doc;
      const lone = doc.childCount === 1 && doc.firstChild?.type === nodes.paragraph && doc.firstChild.content.size === 0;
      const from = lone ? 0 : doc.content.size;
      const tr = state.tr.replaceWith(from, doc.content.size, para);
      dispatch(tr.setSelection(TextSelection.create(tr.doc, from + para.nodeSize - 1)).setStoredMarks([]).scrollIntoView());
    }
    return true;
  };
}

// ---- kept blocks and the escape fix -------------------------------------------------------------------------------

/** Remove the kept block at `pos`. */
export function removeKeptAt(pos: number): Command {
  return (state, dispatch) => {
    const node = state.doc.nodeAt(pos);
    if (!node || node.type !== nodes.kept) return false;
    if (dispatch) {
      const tr = ensureNotEmpty(state.tr.delete(pos, pos + node.nodeSize));
      dispatch(tr.setSelection(Selection.near(tr.doc.resolve(Math.min(pos, tr.doc.content.size)))));
    }
    return true;
  };
}

/** Replace the whole document with `doc` in one step (the escape Fix). */
export function replaceDoc(next: DocJson): Command {
  return (state, dispatch) => {
    if (dispatch) {
      const doc = docFromJson(next);
      const tr = state.tr.replaceWith(0, state.doc.content.size, doc.content);
      dispatch(tr.setSelection(Selection.near(tr.doc.resolve(Math.min(state.selection.from, tr.doc.content.size)))));
    }
    return true;
  };
}

// ---- keymap --------------------------------------------------------------------------------------------------------

/** The editor's keys: Mod-b, Mod-i, Enter (cells break the line, an empty list item leaves the list), Shift-Enter, then
 * ProseMirror's base keymap (added by the view). Mod-z and Mod-Shift-z are left to the creator's history (keymap.ts, capture phase). */
export function richKeys(): Record<string, Command> {
  return {
    "Mod-b": asAction(toggleBold),
    "Mod-i": asAction(toggleItalic),
    "Shift-Enter": insertHardBreak,
    Enter: chainCommands(breakInCell, exitEmptyListItem, newlineInCode, createParagraphNear, liftEmptyBlock, splitBlock),
  };
}

/** Run `cmd` on `state` and return the resulting state and whether it applied (tests and dry runs). */
export function applyCommand(state: EditorState, cmd: Command): { state: EditorState; applied: boolean; tr: Transaction | null } {
  let tr: Transaction | null = null;
  const applied = cmd(state, (t) => { tr = t; });
  return { state: tr ? state.apply(tr) : state, applied, tr };
}

// ---- phone cell sheet (spec 16: "tables edit cell by cell in a sheet") -----------------------------------------------

/** One cell of a table: where it starts, its content (bold, italic, line breaks as they are) and its plain text (line
 * breaks as "\n"). The sheet edits any cell: a cell holds inline content only, which the sheet's schema holds whole. */
export interface TableCell {
  pos: number;
  row: number;
  col: number;
  head: boolean;
  text: string;
  content: Fragment;
}

function cellText(cell: PMNode): string {
  let text = "";
  cell.forEach((child) => {
    if (child.type === nodes.hard_break) text += "\n";
    else if (child.isText) text += child.text ?? "";
  });
  return text;
}

/** Every cell of the table holding the selection, in reading order (row by row), or null outside a table. */
export function tableCells(state: EditorState): TableCell[] | null {
  const at = tableAt(state);
  if (!at) return null;
  const cells: TableCell[] = [];
  let rowPos = at.pos + 1;
  at.table.forEach((row, _o, r) => {
    let cellPos = rowPos + 1;
    row.forEach((cell, _c, c) => {
      cells.push({ pos: cellPos, row: r, col: c, head: cell.type === nodes.table_header, text: cellText(cell), content: cell.content });
      cellPos += cell.nodeSize;
    });
    rowPos += row.nodeSize;
  });
  return cells;
}

/** The index (in `tableCells`) of the cell holding the selection. */
export function cellIndexAt(state: EditorState): number {
  const at = tableAt(state);
  const cells = tableCells(state);
  if (!at || !cells) return -1;
  return cells.findIndex((c) => c.row === at.row && c.col === at.col);
}

/** Replace the content of the cell starting at `pos` (the cell sheet's OK, Previous, Next); the cursor goes to the
 * cell's end. */
export function setCellContent(pos: number, content: Fragment): Command {
  return (state, dispatch) => {
    const cell = state.doc.nodeAt(pos);
    if (!cell || (cell.type !== nodes.table_cell && cell.type !== nodes.table_header)) return false;
    if (dispatch) {
      const tr = state.tr.replaceWith(pos + 1, pos + cell.nodeSize - 1, content);
      const end = tr.doc.nodeAt(pos);
      dispatch(tr.setSelection(TextSelection.near(tr.doc.resolve(pos + (end ? end.nodeSize - 1 : 1)), -1)));
    }
    return true;
  };
}

/** Replace the cell starting at `pos` with plain text ("\n" becomes a line break). */
export function setCellText(pos: number, text: string): Command {
  const parts: PMNode[] = [];
  text.split("\n").forEach((line, i) => {
    if (i > 0) parts.push(nodes.hard_break.create());
    if (line) parts.push(richSchema.text(line));
  });
  return setCellContent(pos, Fragment.from(parts));
}
