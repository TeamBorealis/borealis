import type { ReactNode } from "react";
import type { ImportElement, ImportEvent, ImportStage } from "../../api/contract";
import { Icon, ProgressBar } from "../../components";
import { Expander } from "../../components/controls";
import { DescHead, PaneSplit, Tag } from "../ai/Panes";

const STAGES: { id: Exclude<ImportStage, "ready" | "error">; label: string }[] = [
  { id: "detect", label: "Reading your source" }, { id: "plan", label: "Planning" }, { id: "generate", label: "Writing the content" },
  { id: "validate", label: "Testing it in the engine" }, { id: "repair", label: "Fixing small mistakes" },
];

export function ProgressStep({ events, elements, bar }: { events: ImportEvent[]; elements: ImportElement[]; bar: ReactNode }) {
  const last = events[events.length - 1];
  const reached = new Set(events.map((e) => e.stage));
  const msgFor = (s: ImportStage) => [...events].reverse().find((e) => e.stage === s)?.message;
  const list = (
    <Expander title="Import">
      <div className="v5-inline">
        <ProgressBar value={Math.round((last?.progress ?? 0) * 100)} max={100} label="Import progress" />
        <p className="v5-note" aria-live="polite" style={{ margin: 0 }}>{last?.message ?? "Starting..."}</p>
      </div>
      <ol className="v5-list" aria-label="Import stages" style={{ listStyle: "none" }}>
        {STAGES.map((s) => {
          const done = reached.has(s.id) && last?.stage !== s.id;
          const active = last?.stage === s.id;
          return (
            <li key={s.id} className="v5-stage" data-state={active ? "active" : done ? "done" : reached.has(s.id) ? "done" : "pending"}>
              <Icon name={done ? "check" : "chevron"} /><div><strong>{s.label}</strong>{msgFor(s.id) && <div className="v5-note">{msgFor(s.id)}</div>}</div>
            </li>
          );
        })}
      </ol>
    </Expander>
  );
  const desc = (
    <>
      <DescHead title="Working on it" lead="This usually takes under a minute." />
      <h2>Written so far</h2>
      {elements.length === 0 ? <p>Nothing yet.</p> : (
        <ul className="v5-chips" aria-label="Elements written so far">
          {elements.map((e) => <li key={e.id}><Tag tone="info">{e.name}</Tag></li>)}
        </ul>
      )}
    </>
  );
  return <PaneSplit list={<>{bar}{list}</>} desc={desc} label="Progress" />;
}
