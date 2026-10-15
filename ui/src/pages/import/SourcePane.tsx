import { useEffect, useMemo, useRef, useState } from "react";
import type { ImportSourcePassage } from "../../api/contract";
import { Button } from "../../components";

export interface LineRange { start: number; end: number }

const WINDOW = 150;
const STEP = 300;

/** The 1-based line number of the nearest `[data-line]` ancestor of a DOM node. */
function lineOf(node: Node | null): number | null {
  const el = node instanceof Element ? node : node?.parentElement;
  const found = el?.closest("[data-line]");
  const n = found ? Number(found.getAttribute("data-line")) : NaN;
  return Number.isInteger(n) ? n : null;
}

interface Props {
  lines: readonly string[];
  passage: ImportSourcePassage | null;
  /** What the user picked to relocate the element; null = nothing picked. */
  picked: LineRange | null;
  onPick: (range: LineRange | null) => void;
  disabled?: boolean;
}

/** The draft's source text with the element's passage highlighted. Pick lines by clicking line numbers (click, then click again to set the end) or by dragging a text selection. */
export function SourcePane({ lines, passage, picked, onPick, disabled }: Props) {
  const anchor = passage ? passage.startLine : 1;
  const [from, setFrom] = useState(() => Math.max(1, anchor - WINDOW));
  const [to, setTo] = useState(() => Math.min(lines.length, (passage ? passage.endLine : 1) + WINDOW));
  const box = useRef<HTMLDivElement>(null);
  const first = passage?.startLine ?? 0;
  const last = passage?.endLine ?? 0;
  // A new passage re-centres the window and scrolls to it.
  useEffect(() => {
    setFrom(Math.max(1, (first || 1) - WINDOW));
    setTo(Math.min(lines.length, (last || 1) + WINDOW));
  }, [first, last, lines.length]);
  useEffect(() => {
    const target = first ? box.current?.querySelector<HTMLElement>(`[data-line="${first}"]`) : null;
    if (box.current && target) box.current.scrollTop = Math.max(0, target.offsetTop - box.current.offsetTop - 8);
  }, [first, last, from]);

  const rows = useMemo(() => lines.slice(from - 1, to).map((text, i) => ({ n: from + i, text })), [lines, from, to]);
  const [anchorLine, setAnchorLine] = useState<number | null>(null);

  const clickLine = (n: number, shift: boolean) => {
    if (shift && anchorLine !== null) { onPick({ start: Math.min(anchorLine, n), end: Math.max(anchorLine, n) }); setAnchorLine(null); return; }
    if (anchorLine === null) { setAnchorLine(n); onPick({ start: n, end: n }); return; }
    onPick({ start: Math.min(anchorLine, n), end: Math.max(anchorLine, n) });
    setAnchorLine(null);
  };
  const fromSelection = () => {
    const sel = window.getSelection?.();
    if (disabled || !sel || sel.isCollapsed) return;
    const a = lineOf(sel.anchorNode);
    const b = lineOf(sel.focusNode);
    if (a === null || b === null) return;
    onPick({ start: Math.min(a, b), end: Math.max(a, b) });
    setAnchorLine(null);
  };

  return (
    <div className="imp-source">
      {!passage && <p className="imp-hint">No source passage is linked to this element. Pick the lines it comes from, then press "Use this passage".</p>}
      {passage && <p className="imp-hint">Highlighted: lines {passage.startLine} to {passage.endLine}. To change it, pick other lines.</p>}
      {anchorLine !== null && <p className="imp-hint" role="status">Line {anchorLine} picked. Pick the last line (or Shift-click it).</p>}
      <div className="imp-source-lines" ref={box} role="group" aria-label="Source text" tabIndex={0} onMouseUp={fromSelection}>
        {from > 1 && <Button small variant="ghost" onClick={() => setFrom(Math.max(1, from - STEP))}>Show earlier lines</Button>}
        <ol className="imp-lines" start={from}>
          {rows.map(({ n, text }) => {
            const inPassage = passage !== null && n >= passage.startLine && n <= passage.endLine;
            const isPicked = picked !== null && n >= picked.start && n <= picked.end;
            return (
              <li key={n} data-line={n} data-passage={inPassage || undefined} data-picked={isPicked || undefined} className={`imp-line${inPassage ? " is-passage" : ""}${isPicked ? " is-picked" : ""}`}>
                <button type="button" className="imp-line-no" disabled={disabled} aria-pressed={isPicked} aria-label={`Line ${n}${inPassage ? ", in this element's passage" : ""}`}
                  onClick={(e) => clickLine(n, e.shiftKey)}>{n}</button>
                <span className="imp-line-text">{text || " "}</span>
              </li>
            );
          })}
        </ol>
        {to < lines.length && <Button small variant="ghost" onClick={() => setTo(Math.min(lines.length, to + STEP))}>Show later lines</Button>}
      </div>
      {picked && <p className="imp-hint">Picked: {picked.start === picked.end ? `line ${picked.start}` : `lines ${picked.start} to ${picked.end}`}. <Button small variant="ghost" onClick={() => { onPick(null); setAnchorLine(null); }}>Clear</Button></p>}
    </div>
  );
}
