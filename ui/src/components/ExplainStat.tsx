import { type ReactNode, useState } from "react";
import type { StatTrace, TraceStep } from "../api/contract";
import { useAiTask } from "../pages/ai/useAiTask";
import { useApi } from "../state/ApiContext";
import { useCharacter } from "../state/CharacterContext";
import { usePlatform } from "../state/platform";
import { useAsync } from "../state/useAsync";
import { Button } from "./Button";
import { Alert } from "./Card";
import { Modal } from "./Modal";
import { signed } from "./StatBlock";

function Step({ step }: { step: TraceStep }) {
  return (
    <li className={step.included ? "trace-step" : "trace-step excluded"}>
      <span className="trace-label">{step.label}{step.condition ? <span className="muted small"> ({step.condition})</span> : null}</span>
      <span className="trace-value" aria-label={step.included ? `adds ${step.value}` : "not counted"}>{step.included ? signed(step.value) : "skipped"}</span>
      {step.reason && <span className="muted small trace-reason">{step.reason}</span>}
      {step.inputs.length > 0 && <ul className="trace-inputs">{step.inputs.map((s, i) => <Step key={i} step={s} />)}</ul>}
    </li>
  );
}

export function TraceView({ trace }: { trace: StatTrace }) {
  return (
    <div className="stack">
      <p className="trace-total">Total <strong>{trace.value}</strong>{trace.base ? <span className="muted small"> (base {trace.base})</span> : null}</p>
      <ul className="trace-list" aria-label="Contributions">{trace.steps.map((s, i) => <Step key={i} step={s} />)}</ul>
      {trace.diagnostics.map((d, i) => <Alert key={i} tone={d.severity === "error" ? "danger" : d.severity === "warning" ? "warning" : "info"}>{d.message}</Alert>)}
    </div>
  );
}

/** The optional AI prose on top of the deterministic trace (only mounted when the AI assistant is on). */
function ExplainWithAi({ stat, label, characterId }: { stat: string; label: string; characterId: string }) {
  const ai = useAiTask();
  const aiText = ai.messages.find((m) => m.role === "assistant")?.text ?? "";
  return (
    <>
      <div className="row">
        <Button icon="sparkles" disabled={ai.streaming} onClick={() => void ai.send("explain", `Explain my ${label}.`, characterId, { stat })}>Explain with AI</Button>
        {ai.streaming && <Button small onClick={ai.cancel}>Stop</Button>}
      </div>
      {ai.error && <Alert tone="danger">{ai.error}</Alert>}
      {aiText && <div className="explain-ai" aria-live="polite"><p>{aiText}</p></div>}
    </>
  );
}

function ExplainDialog({ stat, label, onClose }: { stat: string; label: string; onClose: () => void }) {
  const api = useApi();
  const { snapshot } = useCharacter();
  const id = snapshot?.id ?? "";
  const trace = useAsync(() => api.explainStat({ characterId: id, stat }), [api, id, stat]);
  const { aiEnabled } = usePlatform();
  return (
    <Modal title={`Explain ${label}`} onClose={onClose} footer={<Button onClick={onClose}>Close</Button>}>
      <div className="stack">
        {trace.loading && <p className="muted">Working it out...</p>}
        {trace.error && <Alert tone="danger">{trace.error}</Alert>}
        {trace.data && <TraceView trace={trace.data} />}
        {aiEnabled && <ExplainWithAi stat={stat} label={label} characterId={id} />}
      </div>
    </Modal>
  );
}

/** Wraps a computed number: tapping it opens the provenance trace ("Explain this number"). */
export function ExplainStat({ stat, label, value, children }: { stat: string; label: string; value?: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" className="explain-trigger" aria-label={value ? `Explain ${label}, ${value}` : `Explain ${label}`} aria-haspopup="dialog" onClick={() => setOpen(true)}>{children}</button>
      {open && <ExplainDialog stat={stat} label={label} onClose={() => setOpen(false)} />}
    </>
  );
}
