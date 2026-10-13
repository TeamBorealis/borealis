import { type ReactNode, useState } from "react";
import type { ImportDraft, ImportElement } from "../../api/contract";
import { Alert, Button, TextField } from "../../components";
import { Expander } from "../../components/controls";
import { DescHead, PaneSplit, Tag } from "../ai/Panes";
import { ElementDetail, ElementRow } from "./ElementCard";
import type { ImportRun } from "./useImport";

function groupByLevel(elements: ImportElement[]): { key: string; title: string; items: ImportElement[] }[] {
  const top = elements.filter((e) => !e.level);
  const levels = [...new Set(elements.filter((e) => e.level).map((e) => e.level as number))].sort((a, b) => a - b);
  return [
    ...(top.length ? [{ key: "top", title: "Overview", items: top }] : []),
    ...levels.map((l) => ({ key: `l${l}`, title: `Level ${l}`, items: elements.filter((e) => e.level === l) })),
  ];
}

function SweepBanner({ draft }: { draft: ImportDraft }) {
  const isClass = draft.detected.kinds.some((k) => k === "class" || k === "subclass");
  const { levelsPassed, errors } = draft.sweep;
  if (errors.length) return <Alert tone="warning">The level test found {errors.length} problem{errors.length > 1 ? "s" : ""}: {errors.join(" ")}</Alert>;
  if (isClass && levelsPassed >= 20) return <Alert tone="success">Level 1–20 test passed ✓</Alert>;
  if (isClass) return <Alert tone="warning">Level test reached level {levelsPassed} of 20.</Alert>;
  return <Alert tone="success">Engine check passed ✓</Alert>;
}

function ParentChooser({ draft, onChoose }: { draft: ImportDraft; onChoose: (id: string) => void }) {
  const [picked, setPicked] = useState("");
  if (!draft.parentChoice) return null;
  return (
    <Expander title="Which class is this subclass for?" footer={<div className="v5-inline"><div><Button variant="primary" disabled={!picked} onClick={() => onChoose(picked)}>Use this class</Button></div></div>}>
      <fieldset className="radio-group" style={{ padding: "6px 8px" }}>
        <legend className="sr-only">Parent class</legend>
        {draft.parentChoice.candidates.map((c) => (
          <label key={c.classId} className="radio-row"><input type="radio" name="parent" value={c.classId} checked={picked === c.classId} onChange={() => setPicked(c.classId)} />{c.name}</label>
        ))}
      </fieldset>
    </Expander>
  );
}

export function ReviewStep({ run, draft, bar }: { run: ImportRun; draft: ImportDraft; bar: ReactNode }) {
  const [instruction, setInstruction] = useState("");
  const [packName, setPackName] = useState(draft.suggestedPackName);
  const [prefix, setPrefix] = useState(draft.suggestedPrefix);
  const [picked, setPicked] = useState<string | null>(null);
  const failed = draft.elements.filter((e) => e.status === "failed").length;
  const review = draft.elements.filter((e) => e.status === "needs_review").length;
  const blocked = !!draft.parentChoice || failed > 0 || !packName.trim() || !prefix.trim();
  const fallback = draft.elements.find((e) => e.status === "failed" || e.status === "needs_review") ?? draft.elements[0];
  const selected = draft.elements.find((e) => e.id === picked) ?? fallback;

  const list = (
    <>
      <SweepBanner draft={draft} />
      <div className="v5-foot" style={{ margin: "6px 0 10px" }}>
        <Tag tone="info">{draft.detected.kinds.join(", ")}</Tag>
        <Tag>{draft.detected.edition === "dnd2024" ? "2024 rules" : "2014 rules"}</Tag>
        <Tag>{Math.round(draft.detected.confidence * 100)}% sure</Tag>
        {review > 0 && <Tag tone="warn">{review} to review</Tag>}
        {failed > 0 && <Tag tone="bad">{failed} failed</Tag>}
      </div>
      <ParentChooser draft={draft} onChoose={(id) => void run.choosePar(id)} />
      <Expander title="Ask the AI to change anything">
        <form className="v5-inline" onSubmit={(e) => { e.preventDefault(); if (instruction.trim()) { void run.revise(instruction.trim()); setInstruction(""); } }}>
          <TextField label="Ask the AI to change anything" value={instruction} onChange={(e) => setInstruction(e.target.value)} placeholder="e.g. make the capstone weaker" />
          <div><Button type="submit" icon="sparkles" disabled={!instruction.trim() || run.revising.has("*")}>{run.revising.has("*") ? "Working..." : "Apply"}</Button></div>
        </form>
      </Expander>
      {groupByLevel(draft.elements).map((g) => (
        <Expander key={g.key} title={g.title} headerExtra={<span>{g.items.length}</span>}>
          <ul className="v5-list" aria-label={g.title}>
            {g.items.map((e) => (
              <ElementRow key={e.id} element={e} selected={e.id === selected?.id} onSelect={() => setPicked(e.id)}
                revising={run.revising.has(e.id) || run.revising.has("*")}
                onRevise={(i) => void run.revise(i, e.id)} onSaveJson={(j) => run.updateElement(e.id, j)} />
            ))}
          </ul>
        </Expander>
      ))}
      <Expander title="Add to your library" footer={
        <div className="v5-inline">
          {draft.parentChoice && <Alert tone="warning">Choose the parent class above first.</Alert>}
          {failed > 0 && <Alert tone="danger">Fix the failed elements before adding.</Alert>}
          <div className="v5-foot">
            <Button variant="primary" icon="save" disabled={blocked || run.busy} onClick={() => void run.finalize({ packName: packName.trim(), prefix: prefix.trim() })}>Add to Library</Button>
            <Button variant="ghost" onClick={() => void run.discard()}>Discard</Button>
          </div>
        </div>}>
        <div className="v5-inline v5-form">
          <TextField label="Pack name" value={packName} onChange={(e) => setPackName(e.target.value)} />
          <TextField label="Id prefix" hint="Short, lowercase letters" value={prefix} onChange={(e) => setPrefix(e.target.value.toLowerCase())} />
        </div>
      </Expander>
    </>
  );
  const desc = selected ? <ElementDetail element={selected} /> : <DescHead title="Nothing to review" lead="No elements were generated." />;
  return <PaneSplit list={<>{bar}{list}</>} desc={desc} label="Review" />;
}
