import { useState } from "react";
import type { ImportElement } from "../../api/contract";
import { Button } from "../../components";
import { DescHead, Tag, type TagTone } from "../ai/Panes";
import { ElementActions, type ActionHandlers } from "./ElementActions";
import { type LineRange, SourcePane } from "./SourcePane";
import { STATE_LABEL, summaryText } from "./reviewModel";
import { RawJsonEditor } from "./RawJsonEditor";

const TONE: Record<ImportElement["review"]["state"], TagTone> = { ok: "ok", check: "warn", not_imported: "bad" };
const typeLabel = (e: ImportElement) => e.type.replace(/_/g, " ");

export const TEXT_ONLY_NOTE = "The sheet won't calculate this; it's shown as text";

export interface ElementRowProps extends ActionHandlers {
  element: ImportElement;
  selected: boolean;
  /** Being rebuilt or rewritten right now. */
  busy: boolean;
  sourceLines: readonly string[];
  /** The optional AI assistant is on: only then is "Fix with AI" offered. */
  aiEnabled: boolean;
  /** The last failed action on this element, in plain words. */
  error: string | null;
  onSelect: () => void;
  onSaveJson: (json: Record<string, unknown>) => Promise<boolean>;
  /** Opens the bug report dialog with this element marked as wrong. */
  onReport?: () => void;
}

function DetailList({ element }: { element: ImportElement }) {
  const items = [...new Set([...element.review.details, ...element.balanceFlags.map((f) => `Balance: ${f}`), ...element.validationIssues])];
  if (!items.length) return null;
  return (
    <details className="imp-details">
      <summary>Details ({items.length})</summary>
      <ul>{items.map((d, i) => <li key={`${i}-${d}`}>{d}</li>)}</ul>
    </details>
  );
}

function ElementSide({ element, onSaveJson }: Pick<ElementRowProps, "element" | "onSaveJson">) {
  const r = element.review;
  return (
    <div className="imp-side">
      <h3 className="imp-side-title">Imported element</h3>
      <p className="imp-summary">{summaryText(element) || "No description."}</p>
      {r.textOnly && (
        <p className="imp-textonly" role="note"><strong>Text only.</strong> {TEXT_ONLY_NOTE}.{r.textOnlyReason ? ` ${r.textOnlyReason}` : ""}</p>
      )}
      <DetailList element={element} />
      <RawJsonEditor element={element} onSave={onSaveJson} />
    </div>
  );
}

/** One imported element as a list row. The selected row opens its badge, reason, element-beside-source view and point-and-fix actions. */
export function ElementRow(props: ElementRowProps) {
  const { element, selected, busy, sourceLines, error, onSelect, onSaveJson, onReport } = props;
  const [picked, setPicked] = useState<LineRange | null>(null);
  const [seen, setSeen] = useState(element.id);
  if (seen !== element.id) { setSeen(element.id); setPicked(null); }
  const r = element.review;

  return (
    <li className={selected ? "is-selected" : ""} data-status={element.status} data-review={r.state}>
      <section aria-label={element.name}>
        <button type="button" className="v5-row" aria-current={selected} aria-expanded={selected} onClick={onSelect}>
          <span className="grow">{element.name}</span>
          <Tag tone={TONE[r.state]}>{STATE_LABEL[r.state]}</Tag>
          {r.textOnly && <Tag tone="info">Text only</Tag>}
          <span className="meta">{typeLabel(element)}{element.level ? `, level ${element.level}` : ""}</span>
        </button>
        {r.reason && <span className={`v5-sub ${r.state === "check" ? "warn" : r.state === "not_imported" ? "bad" : ""}`}>{r.reason}</span>}
        {selected && (
          <div className="v5-inline imp-panel">
            {busy && <p className="imp-hint" role="status">Working on {element.name}. Your draft stays as it is until this finishes.</p>}
            {error && <p className="error-text" role="alert">{error}</p>}
            <div className="imp-split">
              <ElementSide element={element} onSaveJson={onSaveJson} />
              <details className="imp-source-box" open>
                <summary>Source passage</summary>
                <SourcePane lines={sourceLines} passage={element.sourcePassage} picked={picked} onPick={setPicked} disabled={busy} />
              </details>
            </div>
            <ElementActions {...props} picked={picked} onPicked={() => setPicked(null)} />
            {onReport && <div className="v5-foot"><Button small variant="ghost" onClick={onReport}>Report a problem</Button></div>}
          </div>
        )}
      </section>
    </li>
  );
}

/** Description-pane view of the selected element: its reason in full and the result in the classic description typography. */
export function ElementDetail({ element }: { element: ImportElement }) {
  const r = element.review;
  return (
    <>
      <DescHead title={element.name} lead={`${typeLabel(element)}${element.level ? `, level ${element.level}` : ""} · ${STATE_LABEL[r.state]}`} />
      {r.reason && <p>{r.reason}</p>}
      <h2>From your source</h2>
      <blockquote>{element.sourcePassage?.text || element.sourceExcerpt || "No excerpt."}</blockquote>
      <h2>Result</h2>
      <p>{summaryText(element) || "Generated element."}</p>
    </>
  );
}
