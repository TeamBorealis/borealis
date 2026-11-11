import { useEffect, useId, useState } from "react";
import { Button, Dialog } from "../../../components";
import type { TableCell } from "./commands";

export interface CellSheetProps {
  /** The table's cells in reading order, read again after every write. */
  cells: readonly TableCell[];
  /** The cell on screen. */
  index: number;
  /** Writes the cell's text (OK, Previous, Next). */
  onWrite: (cell: TableCell, text: string) => void;
  onIndex: (index: number) => void;
  onClose: () => void;
}

const cellName = (c: TableCell): string => `${c.head ? "Header" : `Row ${c.row + 1}`}, column ${c.col + 1}`;

/**
 * Phones (spec 16): a table cell edited in a bottom sheet, one cell at a time. Previous and Next keep what was typed and
 * move along the table row by row; OK keeps it and closes; Cancel (or a swipe down) keeps the cell as it was.
 */
export function CellSheet({ cells, index, onWrite, onIndex, onClose }: CellSheetProps) {
  const cell = cells[index];
  const [text, setText] = useState(cell?.text ?? "");
  const inputId = useId();
  const shownPos = cell?.pos;
  const shownText = cell?.text;
  // a new cell (Previous, Next) or a write shows that cell's text; a re-render of the editor alone changes nothing
  useEffect(() => { setText(shownText ?? ""); }, [index, shownPos, shownText]);
  if (!cell) return null;
  const keep = () => { if (text !== cell.text) onWrite(cell, text); };
  const go = (to: number) => { keep(); onIndex(to); };
  return (
    <Dialog title={`Table cell: ${cellName(cell)}`} onClose={onClose} className="rich-cell-sheet"
      footer={<><Button onClick={onClose}>Cancel</Button><Button variant="primary" onClick={() => { keep(); onClose(); }}>OK</Button></>}>
      <div className="mg-f hb-field">
        <label htmlFor={inputId}>Text</label>
        <textarea id={inputId} className="hb-multiline" value={text} rows={4} onChange={(e) => setText(e.target.value)} data-autofocus="" />
        <p className="hb-field-help">A new line in the text is a line break in the cell.</p>
      </div>
      <div className="rich-cell-nav">
        <Button disabled={index <= 0} onClick={() => go(index - 1)}>Previous cell</Button>
        <Button disabled={index >= cells.length - 1} onClick={() => go(index + 1)}>Next cell</Button>
      </div>
    </Dialog>
  );
}
