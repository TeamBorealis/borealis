import { toggleMark } from "prosemirror-commands";
import { Fragment, Schema, Slice, type MarkType, type NodeSpec, type NodeType, type Node as PMNode } from "prosemirror-model";
import type { Command } from "prosemirror-state";
import { normalizePastedSlice } from "./paste";
import { richSchema } from "./schema";

// The phone cell sheet's editor (spec 16, decision HC12): one table cell's inline content in a ProseMirror document of
// its own. Its schema is the description schema's inline part (every inline node and every mark, specs shared), so any
// cell the description holds loads into it and comes back exactly: bold and italic with their tags (b/strong, i/em),
// line breaks, marks on line breaks. Only the UI tree crosses; the Rust writer still makes the HTML.

function inlineNodes(): Record<string, NodeSpec> {
  const specs: Record<string, NodeSpec> = {};
  richSchema.spec.nodes.forEach((name, spec) => {
    if (spec.group === "inline") specs[name] = spec;
  });
  return specs;
}

export const cellSchema = new Schema({
  nodes: { doc: { content: "inline*" }, ...inlineNodes() },
  marks: richSchema.spec.marks,
});

/** A mark of the sheet's schema (the description schema's, so it is there). */
function markType(name: string): MarkType {
  const type = cellSchema.marks[name];
  if (!type) throw new Error(`the cell schema has no ${name} mark`);
  return type;
}

const hardBreak: NodeType = (() => {
  const type = cellSchema.nodes.hard_break;
  if (!type) throw new Error("the cell schema has no hard_break");
  return type;
})();

export const cellBold = markType("bold");
export const cellItalic = markType("italic");

/** The sheet's document for a cell's content (a description-schema fragment). */
export function cellDoc(content: Fragment): PMNode {
  return cellSchema.node("doc", null, Fragment.fromJSON(cellSchema, content.toJSON()));
}

/** The cell content (description schema) of the sheet's document. */
export function cellContent(doc: PMNode): Fragment {
  return Fragment.fromJSON(richSchema, doc.content.toJSON());
}

export const toggleCellBold: Command = toggleMark(cellBold);
export const toggleCellItalic: Command = toggleMark(cellItalic);

/** A line break at the cursor, carrying the marks there (Enter and Shift+Enter, as in a description cell). */
export const cellHardBreak: Command = (state, dispatch) => {
  if (dispatch) dispatch(state.tr.replaceSelectionWith(hardBreak.create()).scrollIntoView());
  return true;
};

/** Swallowed: the sheet keeps no history of its own (Cancel leaves the cell as it was); the browser's own undo would
 * rewrite the editor's DOM behind ProseMirror. */
const noUndo: Command = () => true;

/** The sheet's keys: Mod-b, Mod-i, Enter and Shift-Enter break the line, undo keys do nothing (ProseMirror's base keymap
 * is added by the view). The creator's own keys wait while the sheet is open (`useCreatorKeys`, modal). */
export function cellKeys(): Record<string, Command> {
  return {
    "Mod-b": toggleCellBold,
    "Mod-i": toggleCellItalic,
    Enter: cellHardBreak,
    "Shift-Enter": cellHardBreak,
    "Mod-z": noUndo,
    "Mod-y": noUndo,
    "Shift-Mod-z": noUndo,
  };
}

/** Pasted plain text in the sheet: one line per line break, through the description's paste rules (escape fix, text
 * normalization as in a cell). */
export function cellPastedText(text: string): Slice {
  const lines = text.replace(/\r\n?/g, "\n");
  if (lines === "") return Slice.empty;
  return normalizePastedSlice(new Slice(Fragment.from(cellSchema.text(lines)), 0, 0), true);
}
