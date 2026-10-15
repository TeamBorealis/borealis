import { useState } from "react";
import type { ImportElement } from "../../api/contract";
import { Button, TextField } from "../../components";
import type { LineRange } from "./SourcePane";

export interface ActionHandlers {
  onAccept: (accepted: boolean) => void;
  onRelocate: (startLine: number, endLine: number) => void;
  onRegenerate: () => void;
  onRevise: (instruction: string) => void;
  onTextOnly: (value: boolean | null) => void;
}

interface Props extends ActionHandlers {
  element: ImportElement;
  busy: boolean;
  aiEnabled: boolean;
  picked: LineRange | null;
  onPicked: () => void;
}

const TEXT_ONLY_CHOICES: { value: "auto" | "text" | "rules"; label: string }[] = [
  { value: "auto", label: "Automatic" }, { value: "text", label: "Text only" }, { value: "rules", label: "Has working rules" },
];
const toChoice = (o: boolean | null) => (o === null ? "auto" : o ? "text" : "rules");
const fromChoice = (c: string): boolean | null => (c === "auto" ? null : c === "text");

/** Point-and-fix for one element. Everything except "Fix with AI" works without the optional assistant. */
export function ElementActions({ element, busy, aiEnabled, picked, onPicked, onAccept, onRelocate, onRegenerate, onRevise, onTextOnly }: Props) {
  const [instruction, setInstruction] = useState("");
  const r = element.review;
  const passage = element.sourcePassage;
  const canAccept = r.state === "check" || r.accepted;
  return (
    <div className="imp-actions">
      <div className="v5-foot">
        {canAccept && (r.accepted
          ? <Button small disabled={busy} onClick={() => onAccept(false)}>Undo "Looks right"</Button>
          : <Button small variant="primary" icon="check" disabled={busy} onClick={() => onAccept(true)}>Looks right</Button>)}
        <Button small disabled={busy || !picked} onClick={() => { if (picked) { onRelocate(picked.start, picked.end); onPicked(); } }}
          title={picked ? undefined : "Pick lines in the source first"}>
          Use this passage{picked ? ` (${picked.start === picked.end ? `line ${picked.start}` : `lines ${picked.start} to ${picked.end}`})` : ""}
        </Button>
        <Button small disabled={busy || !passage} onClick={onRegenerate}
          title={passage ? "Build this element again from its current passage" : "Pick a passage first"}>Regenerate</Button>
      </div>
      {r.state !== "not_imported" && (
        <fieldset className="radio-group imp-textonly-choice">
          <legend>How the sheet treats this element</legend>
          {TEXT_ONLY_CHOICES.map((c) => (
            <label key={c.value} className="radio-row">
              <input type="radio" name={`text-only-${element.id}`} value={c.value} disabled={busy} checked={toChoice(r.textOnlyOverride) === c.value} onChange={() => onTextOnly(fromChoice(c.value))} />
              {c.label}
            </label>
          ))}
        </fieldset>
      )}
      {aiEnabled && (
        <form className="v5-foot" onSubmit={(e) => { e.preventDefault(); if (instruction.trim()) { onRevise(instruction.trim()); setInstruction(""); } }}>
          <div style={{ flex: "1 1 14rem" }}><TextField label={`Fix ${element.name} with AI`} value={instruction} onChange={(e) => setInstruction(e.target.value)} placeholder="e.g. make this once per long rest" disabled={busy} /></div>
          <Button type="submit" icon="sparkles" disabled={busy || !instruction.trim()}>{busy ? "Fixing..." : "Fix with AI"}</Button>
        </form>
      )}
    </div>
  );
}
