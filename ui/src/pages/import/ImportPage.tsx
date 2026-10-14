import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { Alert, Button, Icon } from "../../components";
import { Expander } from "../../components/controls";
import { useToast } from "../../components/Toast";
import { takeImportFiles } from "../../state/importInbox";
import { DescHead, PaneSplit } from "../ai/Panes";
import { ProgressStep } from "./ProgressStep";
import { ReportDialog } from "./ReportDialog";
import { ReviewStep } from "./ReviewStep";
import { SourceStep } from "./SourceStep";
import { useImport } from "./useImport";
import "./import.css";

const STEPS = ["Add", "Progress", "Review", "Done"] as const;

export function ImportPage() {
  const run = useImport();
  const toast = useToast();
  const navigate = useNavigate();
  const [initialFiles] = useState(() => takeImportFiles());
  const [reporting, setReporting] = useState(false);
  const idx = ["source", "progress", "review", "done"].indexOf(run.phase);

  useEffect(() => { if (run.phase === "done" && run.pack) toast.push(`Added "${run.pack.name}" to your library.`); }, [run.phase, run.pack, toast]);

  // The step bar and errors sit in the list column so the description pane starts at the top, like other two-pane pages.
  const head = (
    <>
      <div className="v5-bar">
        <ol className="steps v5-steps" aria-label="Import steps">
          {STEPS.map((s, i) => <li key={s} aria-current={i === idx ? "step" : undefined} data-done={i < idx}>{i + 1}. {s}</li>)}
        </ol>
        {run.phase === "source" && <><span style={{ flex: 1 }} /><Link className="v5-note" to="/ai">Open the AI assistant</Link></>}
      </div>
      {run.error && <div style={{ padding: "10px 20px 0" }}><Alert tone="danger">{run.error}</Alert></div>}
    </>
  );

  return (
    <div className="v5-page import-page">
      <h1 className="sr-only">Import</h1>
      {run.phase === "source" && <SourceStep initialFiles={initialFiles} busy={run.busy} onStart={(r) => void run.start(r)} bar={head} />}
      {run.phase === "progress" && <ProgressStep events={run.events} elements={run.liveElements} bar={head} />}
      {run.phase === "review" && run.draft && <ReviewStep run={run} draft={run.draft} bar={head} />}
      {run.phase === "done" && run.pack && (
        <PaneSplit label="Done"
          list={
            <>{head}<Expander title="Added to your library" footer={
              <div className="v5-inline v5-foot">
                <Button variant="primary" onClick={() => navigate("/compendium")}>Open in Compendium</Button>
                <Button onClick={() => navigate("/characters")}>Use on a character</Button>
                <Button variant="ghost" onClick={() => void run.discard()}>Import another</Button>
                <Button variant="ghost" onClick={() => setReporting(true)}>Send a report</Button>
              </div>}>
              <p className="v5-inline" style={{ margin: 0 }}><span><Icon name="check" /> <strong>{run.pack.name}</strong> is ready ({run.pack.elementCount} elements, kept on this device only).</span></p>
            </Expander></>}
          desc={<DescHead title={run.pack.name} lead="Added to your library." />} />
      )}
      {run.phase === "done" && run.pack && reporting && <ReportDialog target={{ packId: run.pack.id }} onClose={() => setReporting(false)} />}
    </div>
  );
}
