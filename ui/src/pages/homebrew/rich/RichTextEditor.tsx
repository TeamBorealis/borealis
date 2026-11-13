import { useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { baseKeymap } from "prosemirror-commands";
import { keymap } from "prosemirror-keymap";
import type { Node as PMNode } from "prosemirror-model";
import { EditorState, Selection, type Command, type Transaction } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { Icon, type IconName } from "../../../components/Icon";
import { usePhone } from "../../../lib/phone";
import { CellSheet } from "./CellSheet";
import "../../equipment/equipment.css";
import "../homebrew.css";
import "./rich.css";
import {
  ACTION_META, addTableColumn, addTableRow, asAction, cellIndexAt, setCellContent, tableCells, deleteTable, docFromJson, insertHigherLevel, insertTable, isHeadingActive,
  isListActive, isMarkActive, removeKeptAt, replaceDoc, richKeys, tableAt, toggleBold, toggleHeading, toggleItalic, toggleList,
  type Edition,
} from "./commands";
import { fixDocJson, lintDocJson, normalizeDocJson } from "./doc";
import { keptNodeView } from "./keptView";
import { cleanPastedHtml, normalizePastedSlice, parsePastedText } from "./paste";
import { richSchema } from "./schema";
import type { DocJson, EscapeFinding } from "./types";

// The description editor (homebrew creator spec 7.1, 13.13, 16, 17): a ProseMirror view over `richSchema` with the
// Equipment inventory panel's icon+text flat buttons as its toolbar. The value is the JSON tree; the creator's history
// owns undo/redo, so a new `value` from outside replaces the editor's document.

export interface RichTextEditorProps {
  /** Id of the editable (the field's input id). */
  id: string;
  /** Accessible name (the field label). */
  label: string;
  value: DocJson | null | undefined;
  /** `typing` is true for text input (grouped into one undo step), false for toolbar actions, paste, Fix and Remove. */
  onChange: (doc: DocJson, typing: boolean) => void;
  describedBy?: string;
  invalid?: boolean;
  extras?: "spell" | "class" | null;
  edition?: Edition | null;
}

/** Most artefacts named in the escape warning line. */
const MAX_NAMED_FINDINGS = 4;

/** Whether `value` and `doc` are the same description once both are normalized (no re-sync needed). */
export function sameDescription(value: DocJson | null | undefined, doc: PMNode): boolean {
  const a = normalizeDocJson(value ?? { type: "doc" });
  const b = normalizeDocJson(doc.toJSON() as DocJson);
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Whether a transaction is typing: not a toolbar or key action, not paste, drop or cut. */
export function isTyping(tr: Transaction): boolean {
  return !tr.getMeta(ACTION_META) && tr.getMeta("uiEvent") === undefined && !tr.getMeta("paste");
}

/** The escape warning sentence for the findings (plain words, the artefacts as found). */
export function escapeWarning(findings: readonly EscapeFinding[]): string {
  const named = [...new Set(findings.map((f) => f.found))].slice(0, MAX_NAMED_FINDINGS);
  return `Looks like pasted escape characters: ${named.join(", ")}`;
}

function editorAttributes(p: RichTextEditorProps): Record<string, string> {
  const attrs: Record<string, string> = {
    id: p.id,
    class: "rich rich-edit",
    role: "textbox",
    "aria-multiline": "true",
    "aria-label": p.label,
  };
  if (p.describedBy) attrs["aria-describedby"] = p.describedBy;
  if (p.invalid) attrs["aria-invalid"] = "true";
  return attrs;
}

function plugins() {
  return [keymap(richKeys()), keymap(baseKeymap)];
}

interface ToolButton {
  key: string;
  icon: IconName;
  text: string;
  label: string;
  title: string;
  cmd?: Command;
  /** Toggle buttons only: whether the format is on at the selection. */
  pressed?: boolean;
  /** Shown but not usable yet (keeps the tooltip reachable). */
  unavailable?: boolean;
  /** A formatting button: its icon alone on desktop (the name is its label and tooltip), so the toolbar keeps one row
   * in the editor column (U7, WB-26); phones show the text under the icon. */
  compact?: boolean;
}

function toolButtons(state: EditorState, extras: RichTextEditorProps["extras"], edition: RichTextEditorProps["edition"]): ToolButton[] {
  const inTable = tableAt(state) !== null;
  const buttons: ToolButton[] = [
    { key: "bold", compact: true, icon: "bold", text: "Bold", label: "Bold", title: "Bold (Ctrl+B)", cmd: toggleBold, pressed: isMarkActive(state, richSchema.marks.bold) },
    { key: "italic", compact: true, icon: "italic", text: "Italic", label: "Italic", title: "Italic (Ctrl+I)", cmd: toggleItalic, pressed: isMarkActive(state, richSchema.marks.italic) },
    { key: "h3", compact: true, icon: "heading-3", text: "Heading", label: "Heading", title: "Heading", cmd: toggleHeading(3), pressed: isHeadingActive(state, 3) },
    { key: "h4", compact: true, icon: "heading-4", text: "Subheading", label: "Subheading", title: "Subheading", cmd: toggleHeading(4), pressed: isHeadingActive(state, 4) },
    { key: "ul", compact: true, icon: "list-bulleted", text: "List", label: "Bulleted list", title: "Bulleted list", cmd: toggleList("bullet_list"), pressed: isListActive(state, "bullet_list") },
    { key: "ol", compact: true, icon: "list-numbered", text: "Numbered", label: "Numbered list", title: "Numbered list", cmd: toggleList("ordered_list"), pressed: isListActive(state, "ordered_list") },
  ];
  if (inTable) {
    buttons.push(
      { key: "row", compact: true, icon: "table-row-add", text: "Add row", label: "Add table row", title: "Add a row below", cmd: addTableRow },
      { key: "col", compact: true, icon: "table-column-add", text: "Add column", label: "Add table column", title: "Add a column to the right", cmd: addTableColumn },
      { key: "deltable", compact: true, icon: "table-remove", text: "Delete table", label: "Delete table", title: "Delete this table", cmd: deleteTable },
    );
  } else {
    buttons.push({ key: "table", icon: "table", text: "Table", label: "Insert table", title: "Insert a table with a header row", cmd: insertTable });
  }
  if (extras === "spell") {
    buttons.push({
      key: "higher", icon: "level-up", text: "Higher-level slots", label: "Higher-level slots",
      title: `Add the "${edition === "dnd2014" ? "At Higher Levels." : "Using a Higher-Level Spell Slot."}" paragraph at the end`,
      cmd: insertHigherLevel(edition),
    });
  }
  if (extras === "class") {
    buttons.push({
      key: "classtable", icon: "table-large", text: "Class table", label: "Insert class table",
      title: "Insert class table: needs the class levels grid, which is not in this build yet", unavailable: true,
    });
  }
  return buttons;
}

/** Keeps the editor focused when a toolbar button is pressed. */
const keepFocus = (e: MouseEvent) => e.preventDefault();

/** The rich-text description editor. */
export function RichTextEditor(props: RichTextEditorProps) {
  const phone = usePhone();
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const propsRef = useRef(props);
  const [state, setState] = useState<EditorState | null>(null);
  const [focused, setFocused] = useState(false);
  /** Phones: the table cell open in the cell sheet (spec 16), by its index in the table. */
  const [cellSheet, setCellSheet] = useState<number | null>(null);

  useLayoutEffect(() => {
    propsRef.current = props;
  });

  const run = (cmd: Command) => {
    const view = viewRef.current;
    if (!view) return;
    asAction(cmd)(view.state, view.dispatch, view);
    view.focus();
  };

  // Mount once; the latest props are read through propsRef.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return undefined;
    const initial = propsRef.current;
    const view: EditorView = new EditorView(host, {
      state: EditorState.create({ doc: docFromJson(initial.value), plugins: plugins() }),
      attributes: editorAttributes(initial),
      nodeViews: {
        kept: keptNodeView((pos) => {
          asAction(removeKeptAt(pos))(view.state, view.dispatch, view);
        }),
      },
      transformPastedHTML: (html) => cleanPastedHtml(html),
      clipboardTextParser: (text) => parsePastedText(text),
      transformPasted: (slice) => normalizePastedSlice(slice),
      dispatchTransaction(tr) {
        const next = view.state.apply(tr);
        view.updateState(next);
        setState(next);
        if (tr.docChanged) propsRef.current.onChange(next.doc.toJSON() as DocJson, isTyping(tr));
      },
    });
    viewRef.current = view;
    setState(view.state);
    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, []);

  useEffect(() => {
    viewRef.current?.setProps({ attributes: editorAttributes(props) });
  }, [props.id, props.label, props.describedBy, props.invalid]); // eslint-disable-line react-hooks/exhaustive-deps

  // A value from outside (undo, redo, another field's change) replaces the document; the cursor stays near.
  useEffect(() => {
    const view = viewRef.current;
    if (!view || sameDescription(props.value, view.state.doc)) return;
    const doc = docFromJson(props.value);
    const selection = Selection.near(doc.resolve(Math.min(view.state.selection.from, doc.content.size)));
    const next = EditorState.create({ doc, plugins: view.state.plugins, selection });
    view.updateState(next);
    setState(next);
  }, [props.value]);

  const doc = state?.doc;
  const findings = useMemo(() => (doc ? lintDocJson(doc.toJSON() as DocJson) : []), [doc]);
  const buttons = state ? toolButtons(state, props.extras, props.edition) : [];
  const docked = phone && (focused || cellSheet !== null);
  const cells = phone && state ? tableCells(state) : null;

  return (
    <div
      className={docked ? "rich-editor rich-docked" : "rich-editor"}
      onFocus={() => setFocused(true)}
      onBlur={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setFocused(false); }}
    >
      <div className="eq-inv-actions hb-commands rich-toolbar" role="toolbar" aria-label="Description formatting">
        {buttons.map((b) => (
          <button
            key={b.key}
            type="button"
            aria-label={b.label}
            title={b.title}
            aria-pressed={b.pressed === undefined ? undefined : b.pressed}
            aria-disabled={b.unavailable || undefined}
            disabled={!b.unavailable && state !== null && b.cmd !== undefined && !b.cmd(state)}
            onMouseDown={keepFocus}
            onClick={() => { if (b.cmd && !b.unavailable) run(b.cmd); }}
          >
            <Icon name={b.icon} /><span className={b.compact ? "rich-btn-text rich-compact" : "rich-btn-text"}>{b.text}</span>
          </button>
        ))}
        {phone && cells && (
          <button type="button" aria-label="Edit cell" title="Edit this cell in a sheet"
            onMouseDown={keepFocus} onClick={() => setCellSheet(cellIndexAt(state!))}>
            <Icon name="pen" />Edit cell
          </button>
        )}
      </div>
      <div ref={hostRef} className="rich-host" />
      {cellSheet !== null && cells && (
        <CellSheet cells={cells} index={cellSheet} onIndex={setCellSheet} onClose={() => setCellSheet(null)}
          onWrite={(cell, content) => { const view = viewRef.current; if (view) asAction(setCellContent(cell.pos, content))(view.state, view.dispatch, view); }} />
      )}
      {findings.length > 0 && (
        <div className="rich-lint" role="status">
          <Icon name="alert" />
          <span>{escapeWarning(findings)}</span>
          <button type="button" className="hb-tool" title="Turn them into the real characters" onClick={() => {
            const view = viewRef.current;
            if (view) run(replaceDoc(fixDocJson(view.state.doc.toJSON() as DocJson)));
          }}>Fix</button>
        </div>
      )}
    </div>
  );
}
