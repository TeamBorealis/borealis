import { useState } from "react";
import type { HomebrewDraft, ValidationReport } from "../../api/contract";
import { Alert, Button, useToast } from "../../components";
import { useApi } from "../../state/ApiContext";
import { errorMessage } from "../../state/useAsync";

export function HomebrewPreview({ draft }: { draft: HomebrewDraft }) {
  const api = useApi();
  const toast = useToast();
  const [text, setText] = useState(JSON.stringify(draft.json, null, 2));
  const [report, setReport] = useState<ValidationReport | null>(null);
  const parse = (): Record<string, unknown> | null => {
    try { return JSON.parse(text) as Record<string, unknown>; } catch (e) { setReport({ valid: false, errors: [`Invalid JSON: ${(e as Error).message}`], warnings: [] }); return null; }
  };
  const validate = async () => { const j = parse(); if (j) setReport(await api.aiValidateHomebrew({ json: j })); };
  const add = async () => {
    const j = parse();
    if (!j) return;
    try { const pack = await api.aiAddHomebrewToPack({ json: j }); toast.push(`Added to "${pack.name}"`); }
    catch (e) { toast.push(errorMessage(e), "error"); }
  };
  return (
    <section aria-label="Homebrew draft">
      <h2 className="v5-sec">Homebrew draft</h2>
      <div className="stack">
        <label className="field-label" htmlFor="hb-json">Element JSON (editable)</label>
        <textarea id="hb-json" className="textarea mono" style={{ minHeight: "14rem" }} value={text} onChange={(e) => { setText(e.target.value); setReport(null); }} spellCheck={false} />
        <div className="row"><Button onClick={() => void validate()}>Validate</Button><Button variant="primary" disabled={!report?.valid} onClick={() => void add()}>Add to my pack</Button></div>
        {report && (
          <Alert tone={report.valid ? "success" : "danger"}>
            {report.valid ? "Valid against the content schema." : "Validation failed."}
            {[...report.errors, ...report.warnings].length > 0 && <ul>{report.errors.map((x) => <li key={x}>Error: {x}</li>)}{report.warnings.map((x) => <li key={x}>Warning: {x}</li>)}</ul>}
          </Alert>
        )}
      </div>
    </section>
  );
}
