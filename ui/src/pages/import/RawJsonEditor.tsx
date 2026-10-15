import { useState } from "react";
import type { ImportElement } from "../../api/contract";
import { Button } from "../../components";

/** Collapsible raw JSON of one element, editable with validation. */
export function RawJsonEditor({ element, onSave }: { element: ImportElement; onSave: (json: Record<string, unknown>) => Promise<boolean> }) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState(() => JSON.stringify(element.json, null, 2));
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [synced, setSynced] = useState(element.json);
  if (synced !== element.json) { setSynced(element.json); setText(JSON.stringify(element.json, null, 2)); }

  const save = async () => {
    try {
      const parsed: unknown = JSON.parse(text);
      if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) throw new Error("Must be a JSON object.");
      setJsonError(null);
      await onSave(parsed as Record<string, unknown>);
    } catch (e) { setJsonError(e instanceof Error ? e.message : "Invalid JSON."); }
  };

  return (
    <div className="stack">
      <div><Button small variant="ghost" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? "Hide" : "Edit"} raw JSON</Button></div>
      {open && (
        <>
          <label className="field-label" htmlFor={`json-${element.id}`}>Raw JSON for {element.name}</label>
          <textarea id={`json-${element.id}`} className="textarea mono" rows={8} spellCheck={false} value={text} onChange={(e) => setText(e.target.value)} aria-invalid={jsonError ? true : undefined} />
          {jsonError && <span className="error-text" role="alert">{jsonError}</span>}
          <div><Button small variant="primary" onClick={() => void save()}>Validate and save</Button></div>
        </>
      )}
    </div>
  );
}
