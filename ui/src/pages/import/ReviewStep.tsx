import { type ReactNode, useState } from "react";
import type { ImportDraft } from "../../api/contract";
import { Alert, Button, TextField } from "../../components";
import { Expander } from "../../components/controls";
import { usePlatform } from "../../state/platform";
import { DescHead, PaneSplit, Tag } from "../ai/Panes";
import { ElementDetail, ElementRow } from "./ElementCard";
import { ReportDialog, selfChecksFailed } from "./ReportDialog";
import { FILTER_LABEL, FILTER_ORDER, type ReviewFilter, isFlagged, totalsOf, visibleElements } from "./reviewModel";
import type { ImportRun } from "./useImport";

function SweepBanner({ draft }: { draft: ImportDraft }) {
  const isClass = draft.detected.kinds.some((k) => k === "class" || k === "subclass");
  const { levelsPassed, errors } = draft.sweep;
  if (errors.length) return <Alert tone="warning">The level test found {errors.length} problem{errors.length > 1 ? "s" : ""}: {errors.join(" ")}</Alert>;
  if (isClass && levelsPassed >= 20) return <Alert tone="success">Level 1–20 test passed</Alert>;
  if (isClass) return <Alert tone="warning">Level test reached level {levelsPassed} of 20.</Alert>;
  return <Alert tone="success">Engine check passed</Alert>;
}

const NOTE_ITEMS_SHOWN = 8;

/** Failed checks on the import as a whole, e.g. headings of the source that were not imported. */
function ImportNotes({ draft }: { draft: ImportDraft }) {
  if (!draft.importNotes.length) return null;
  return (
    <>
      {draft.importNotes.map((n) => {
        const more = n.items.length - NOTE_ITEMS_SHOWN;
        return (
          <Alert key={n.code} tone="warning">
            {n.summary}: {n.items.slice(0, NOTE_ITEMS_SHOWN).join(", ")}{more > 0 ? ` and ${more} more` : ""}.
          </Alert>
        );
      })}
    </>
  );
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

function ReviewSummary({ totals, flagged, filter, onFilter }: { totals: ReturnType<typeof totalsOf>; flagged: number; filter: ReviewFilter; onFilter: (f: ReviewFilter) => void }) {
  return (
    <div className="v5-inline imp-filter-band">
      {flagged === 0
        ? <p className="imp-clean" role="status">Nothing needs your attention: all {totals.all} element{totals.all === 1 ? "" : "s"} imported cleanly{totals.textOnly ? `, ${totals.textOnly} shown as text only` : ""}.</p>
        : <p className="imp-flagged" role="status">{flagged} of {totals.all} element{totals.all === 1 ? "" : "s"} need{flagged === 1 ? "s" : ""} a look. The rest look fine.</p>}
      <div className="v5-foot" role="group" aria-label="Filter elements">
        {FILTER_ORDER.map((f) => (
          <Button key={f} small variant={filter === f ? "primary" : "default"} aria-pressed={filter === f} onClick={() => onFilter(f)}>
            {FILTER_LABEL[f]} ({f === "all" ? totals.all : totals[f]})
          </Button>
        ))}
      </div>
    </div>
  );
}

export function ReviewStep({ run, draft, bar }: { run: ImportRun; draft: ImportDraft; bar: ReactNode }) {
  const [instruction, setInstruction] = useState("");
  const [packName, setPackName] = useState(draft.suggestedPackName);
  const [prefix, setPrefix] = useState(draft.suggestedPrefix);
  const [picked, setPicked] = useState<string | null>(null);
  const [reporting, setReporting] = useState<string[] | null>(null);
  const { aiEnabled } = usePlatform();
  const [filter, setFilter] = useState<ReviewFilter>("all");
  const checksFailed = selfChecksFailed(draft);
  const totals = totalsOf(draft.elements);
  const flagged = totals.check + totals.not_imported;
  const blocked = !!draft.parentChoice || draft.elements.some((e) => e.status === "failed" && e.review.state !== "not_imported") || !packName.trim() || !prefix.trim();
  const failed = draft.elements.filter((e) => e.status === "failed" && e.review.state !== "not_imported").length;
  const shown = visibleElements(draft.elements, filter);
  const fallback = shown.find(isFlagged) ?? shown[0];
  const selected = shown.find((e) => e.id === picked) ?? fallback;

  const list = (
    <>
      <SweepBanner draft={draft} />
      <ImportNotes draft={draft} />
      {checksFailed && (
        <div className="imp-report-band">
          <Alert tone="warning">The self-checks found problems with this import. A report helps fix the importer; you see exactly what is sent.</Alert>
          <div><Button variant="warning" icon="send" onClick={() => setReporting([])}>Send a report</Button></div>
        </div>
      )}
      <div className="v5-foot" style={{ margin: "6px 0 10px" }}>
        <Tag tone="info">{draft.detected.kinds.join(", ")}</Tag>
        <Tag>{draft.detected.edition === "dnd2024" ? "2024 rules" : "2014 rules"}</Tag>
        <Tag>{Math.round(draft.detected.confidence * 100)}% sure</Tag>
      </div>
      <ParentChooser draft={draft} onChoose={(id) => void run.choosePar(id)} />
      {aiEnabled && <Expander title="Ask the AI to change anything" defaultOpen={false}>
        <form className="v5-inline" onSubmit={(e) => { e.preventDefault(); if (instruction.trim()) { void run.revise(instruction.trim()); setInstruction(""); } }}>
          <TextField label="Ask the AI to change anything" value={instruction} onChange={(e) => setInstruction(e.target.value)} placeholder="e.g. make the capstone weaker" />
          <div><Button type="submit" icon="sparkles" disabled={!instruction.trim() || run.revising.has("*")}>{run.revising.has("*") ? "Working..." : "Apply"}</Button></div>
        </form>
      </Expander>}
      <ReviewSummary totals={totals} flagged={flagged} filter={filter} onFilter={setFilter} />
      {shown.length === 0 ? <p className="v5-inline imp-empty">Nothing in "{FILTER_LABEL[filter]}".</p> : (
        <ul className="v5-list" aria-label="Imported elements">
          {shown.map((e) => (
            <ElementRow key={e.id} element={e} selected={e.id === selected?.id} onSelect={() => setPicked(e.id)}
              busy={run.revising.has(e.id) || run.revising.has("*")} sourceLines={draft.sourceLines} aiEnabled={aiEnabled}
              error={run.elementError?.elementId === e.id ? run.elementError.message : null}
              onRevise={(i) => { setPicked(e.id); void run.revise(i, e.id); }} onSaveJson={(j) => run.updateElement(e.id, j)}
              onAccept={(a) => { setPicked(e.id); void run.accept(e.id, a); }} onTextOnly={(v) => { setPicked(e.id); void run.setTextOnly(e.id, v); }}
              onRelocate={(a, b) => { setPicked(e.id); void run.relocate(e.id, a, b); }} onRegenerate={() => { setPicked(e.id); void run.regenerate(e.id); }} onReport={() => setReporting([e.id])} />
          ))}
        </ul>
      )}
      <Expander title="Add to your library" footer={
        <div className="v5-inline">
          {draft.parentChoice && <Alert tone="warning">Choose the parent class above first.</Alert>}
          {failed > 0 && <Alert tone="danger">Fix the failed elements before adding.</Alert>}
          {totals.not_imported > 0 && <Alert tone="info">{totals.not_imported} not-imported element{totals.not_imported > 1 ? "s are" : " is"} left out of the pack.</Alert>}
          <div className="v5-foot">
            <Button variant="primary" icon="save" disabled={blocked || run.busy} onClick={() => void run.finalize({ packName: packName.trim(), prefix: prefix.trim() })}>Add to Library</Button>
            <Button variant="ghost" onClick={() => void run.discard()}>Discard</Button>
            {!checksFailed && <Button variant="ghost" onClick={() => setReporting([])}>Send a report</Button>}
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
  return (
    <>
      <PaneSplit list={<>{bar}{list}</>} desc={desc} label="Review" />
      {reporting && <ReportDialog target={{ draftId: draft.draftId }} flagged={reporting} onClose={() => setReporting(null)} />}
    </>
  );
}
