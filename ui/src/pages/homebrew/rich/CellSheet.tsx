import { useEffect, useId, useRef, useState, type MouseEvent } from "react";
import { baseKeymap } from "prosemirror-commands";
import { keymap } from "prosemirror-keymap";
import type { Fragment } from "prosemirror-model";
import { EditorState, Selection, type Command } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { Button, Dialog } from "../../../components";
import { Icon } from "../../../components/Icon";
import { cellBold, cellContent, cellDoc, cellItalic, cellKeys, cellPastedText, toggleCellBold, toggleCellItalic } from "./cell";
import { isMarkActive, type TableCell } from "./commands";
import { cleanPastedHtml, normalizePastedSlice } from "./paste";

export interface CellSheetProps {
  /** The table's cells in reading order, read again after every write. */
  cells: readonly TableCell[];
  /** The cell on screen. */
  index: number;
  /** Writes the cell's content (OK, Previous, Next), only when it changed. */
  onWrite: (cell: TableCell, content: Fragment) => void;
  onIndex: (index: number) => void;
  onClose: () => void;
}

const cellName = (c: TableCell): string => `${c.head ? "Header" : `Row ${c.row + 1}`}, column ${c.col + 1}`;

/** The sheet editor's state for a cell, the cursor at its end (where a tap in a short cell puts it). */
function cellState(cell: TableCell): EditorState {
  const doc = cellDoc(cell.content);
  return EditorState.create({ doc, selection: Selection.atEnd(doc), plugins: [keymap(cellKeys()), keymap(baseKeymap)] });
}

/** Keeps the cell's editor focused when a formatting button is pressed (the main toolbar does the same). */
const keepFocus = (e: MouseEvent) => e.preventDefault();

/**
 * Phones (spec 16, HC12): a table cell edited in a bottom sheet, one cell at a time, with Bold and Italic. Previous and
 * Next keep what was typed and move along the table row by row; OK keeps it and closes; Cancel (or a swipe down) keeps
 * the cell as it was.
 */
export function CellSheet(props: CellSheetProps) {
  const cell = props.cells[props.index];
  return cell ? <CellSheetBody {...props} cell={cell} /> : null;
}

function CellSheetBody({ cell, cells, index, onWrite, onIndex, onClose }: CellSheetProps & { cell: TableCell }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const [state, setState] = useState<EditorState | null>(null);
  const labelId = useId();
  const helpId = useId();
  const shownPos = cell.pos;
  const shownContent = JSON.stringify(cell.content.toJSON());

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return undefined;
    const view: EditorView = new EditorView(host, {
      state: cellState(cell),
      attributes: {
        class: "rich rich-edit rich-cell-edit",
        role: "textbox",
        "aria-multiline": "true",
        "aria-labelledby": labelId,
        "aria-describedby": helpId,
      },
      transformPastedHTML: (html) => cleanPastedHtml(html),
      clipboardTextParser: (text) => cellPastedText(text),
      transformPasted: (slice) => normalizePastedSlice(slice, true),
      handleDOMEvents: {
        // the browser's own undo (iOS shake, a hardware keyboard) would rewrite the DOM behind ProseMirror
        beforeinput: (_v, e) => {
          const type = (e as InputEvent).inputType;
          if (type !== "historyUndo" && type !== "historyRedo") return false;
          e.preventDefault();
          return true;
        },
      },
      dispatchTransaction(tr) {
        const next = view.state.apply(tr);
        view.updateState(next);
        setState(next);
      },
    });
    viewRef.current = view;
    setState(view.state);
    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // a new cell (Previous, Next) or a write shows that cell; a re-render of the editor alone changes nothing
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const next = cellState(cell);
    view.updateState(next);
    setState(next);
  }, [index, shownPos, shownContent]); // eslint-disable-line react-hooks/exhaustive-deps

  const keep = () => {
    const view = viewRef.current;
    if (!view) return;
    const content = cellContent(view.state.doc);
    if (!content.eq(cell.content)) onWrite(cell, content);
  };
  const go = (to: number) => { keep(); onIndex(to); };
  const run = (cmd: Command) => {
    const view = viewRef.current;
    if (!view) return;
    cmd(view.state, view.dispatch, view);
    view.focus();
  };
  const marks = [
    { key: "bold", icon: "bold", text: "Bold", title: "Bold (Ctrl+B)", cmd: toggleCellBold, mark: cellBold },
    { key: "italic", icon: "italic", text: "Italic", title: "Italic (Ctrl+I)", cmd: toggleCellItalic, mark: cellItalic },
  ] as const;

  return (
    <Dialog title={`Table cell: ${cellName(cell)}`} onClose={onClose} className="rich-cell-sheet"
      footer={<><Button onClick={onClose}>Cancel</Button><Button variant="primary" onClick={() => { keep(); onClose(); }}>OK</Button></>}>
      <div className="mg-f hb-field">
        <span className="mg-label" id={labelId}>Cell text</span>
        <div className="eq-inv-actions hb-commands rich-toolbar rich-cell-tools" role="toolbar" aria-label="Cell formatting">
          {marks.map((b) => (
            <button key={b.key} type="button" aria-label={b.text} title={b.title}
              aria-pressed={state ? isMarkActive(state, b.mark) : false} onMouseDown={keepFocus} onClick={() => run(b.cmd)}>
              <Icon name={b.icon} /><span className="rich-btn-text">{b.text}</span>
            </button>
          ))}
        </div>
        <div ref={hostRef} className="rich-host" />
        <p className="hb-field-help" id={helpId}>A new line in the text is a line break in the cell.</p>
      </div>
      <div className="rich-cell-nav">
        <Button disabled={index <= 0} onClick={() => go(index - 1)}>Previous cell</Button>
        <Button disabled={index >= cells.length - 1} onClick={() => go(index + 1)}>Next cell</Button>
      </div>
    </Dialog>
  );
}
