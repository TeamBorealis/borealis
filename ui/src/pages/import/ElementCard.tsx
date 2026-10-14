import { useState } from "react";
import type { ImportElement, ImportElementStatus } from "../../api/contract";
import { Button, TextField } from "../../components";
import { DescHead, Tag, type TagTone } from "../ai/Panes";

const STATUS: Record<ImportElementStatus, { label: string; tone: TagTone }> = {
  ok: { label: "OK", tone: "ok" }, repaired: { label: "Auto-fixed", tone: "info" },
  needs_review: { label: "Needs review", tone: "warn" }, failed: { label: "Failed", tone: "bad" },
};
const typeLabel = (e: ImportElement) => e.type.replace(/_/g, " ");

export interface ElementRowProps {
  element: ImportElement;
  selected: boolean;
  revising: boolean;
  onSelect: () => void;
  onRevise: (instruction: string) => void;
  onSaveJson: (json: Record<string, unknown>) => Promise<boolean>;
  /** Opens the bug report dialog with this element marked as wrong. */
  onReport?: () => void;
}

/** One generated element as a classic builder list row; the selected row opens its fix-with-AI box and raw JSON editor underneath. */
export function ElementRow({ element, selected, revising, onSelect, onRevise, onSaveJson, onReport }: ElementRowProps) {
  const [instruction, setInstruction] = useState("");
  const [open, setOpen] = useState(false);
  const [text, setText] = useState(() => JSON.stringify(element.json, null, 2));
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [synced, setSynced] = useState(element.json);
  if (synced !== element.json) { setSynced(element.json); setText(JSON.stringify(element.json, null, 2)); }
  const st = STATUS[element.status];

  const save = async () => {
    try {
      const parsed: unknown = JSON.parse(text);
      if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) throw new Error("Must be a JSON object.");
      setJsonError(null);
      await onSaveJson(parsed as Record<string, unknown>);
    } catch (e) { setJsonError(e instanceof Error ? e.message : "Invalid JSON."); }
  };

  return (
    <li className={selected ? "is-selected" : ""} data-status={element.status}>
      <section aria-label={element.name}>
        <button type="button" className="v5-row" aria-current={selected} onClick={onSelect}>
          <span className="grow">{element.name}</span>
          <Tag tone={st.tone}>{st.label}</Tag>
          <span className="meta">{typeLabel(element)}</span>
        </button>
        {element.balanceFlags.map((f) => <span key={f} className="v5-sub warn">Balance: {f}</span>)}
        {element.validationIssues.map((f) => <span key={f} className={`v5-sub ${element.status === "failed" ? "bad" : ""}`}>{f}</span>)}
        {selected && (
          <div className="v5-inline">
            <form className="v5-foot" onSubmit={(e) => { e.preventDefault(); if (instruction.trim()) { onRevise(instruction.trim()); setInstruction(""); } }}>
              <div style={{ flex: "1 1 14rem" }}><TextField label={`Fix ${element.name} with AI`} value={instruction} onChange={(e) => setInstruction(e.target.value)} placeholder="e.g. make this once per long rest" disabled={revising} /></div>
              <Button type="submit" icon="sparkles" disabled={revising || !instruction.trim()}>{revising ? "Fixing..." : "Fix with AI"}</Button>
            </form>
            <div className="v5-foot">
              <Button small variant="ghost" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? "Hide" : "Edit"} raw JSON</Button>
              {onReport && <Button small variant="ghost" onClick={onReport}>Report a problem</Button>}
            </div>
            {open && (
              <div className="stack">
                <label className="field-label" htmlFor={`json-${element.id}`}>Raw JSON for {element.name}</label>
                <textarea id={`json-${element.id}`} className="textarea mono" rows={8} spellCheck={false} value={text} onChange={(e) => setText(e.target.value)} aria-invalid={jsonError ? true : undefined} />
                {jsonError && <span className="error-text" role="alert">{jsonError}</span>}
                <div><Button small variant="primary" onClick={() => void save()}>Validate and save</Button></div>
              </div>
            )}
          </div>
        )}
      </section>
    </li>
  );
}

/** Description-pane view of an element: source excerpt and the generated result in the classic builder description typography. */
export function ElementDetail({ element }: { element: ImportElement }) {
  const description = typeof element.json.description === "string" ? element.json.description : "";
  return (
    <>
      <DescHead title={element.name} lead={`${typeLabel(element)}${element.level ? `, level ${element.level}` : ""}`} />
      <h2>From your source</h2>
      <blockquote>{element.sourceExcerpt || "No excerpt."}</blockquote>
      <h2>Result</h2>
      <p>{description || "Generated element."}</p>
    </>
  );
}
