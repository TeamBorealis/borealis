import { type FormEvent, useState } from "react";
import type { PlayTracker, PlayTrackerReset } from "../../api/contract";
import { Button, EmptyState, IconButton, SelectField, TextField } from "../../components";
import { Dialog, Expander } from "../../components/controls";
import { useCharacter } from "../../state/CharacterContext";
import { PIP_LIMIT, PipButtons, UsesLeft } from "./PipButtons";
import "./spell-tracker.css";

/** Limits the host enforces too (borealis-engine commands/trackers.rs). */
export const MAX_TRACKER_NAME = 60;
export const MAX_TRACKER_MAX = 999;

const RESET_LABEL: Record<PlayTrackerReset, string> = { short_rest: "Short Rest", long_rest: "Long Rest", never: "No reset" };
const RESET_OPTIONS: { value: PlayTrackerReset; label: string }[] = [
  { value: "short_rest", label: "Short Rest" }, { value: "long_rest", label: "Long Rest" }, { value: "never", label: "Never" },
];

function TrackerRow({ t, onEdit }: { t: PlayTracker; onEdit: () => void }) {
  const { mutate } = useCharacter();
  const set = (used: number) => void mutate((api, id) => api.setTrackerUsed({ characterId: id, trackerId: t.id, used }));
  return (
    <li className="v5-line st-line">
      <span className="v5-row static grow"><span className="grow">{t.name}</span><span className="meta">{RESET_LABEL[t.reset]}</span></span>
      {t.max <= PIP_LIMIT
        ? <PipButtons max={t.max} used={t.used} onSet={set} pipName={(n, max, used) => `${t.name} ${n} of ${max}, ${used ? "used" : "available"}`} />
        : <span className="v5-ctrls st-steps">
          <IconButton icon="minus" variant="default" label={`Spend ${t.name}`} disabled={t.used >= t.max} onClick={() => set(t.used + 1)} />
          <IconButton icon="plus" variant="default" label={`Restore ${t.name}`} disabled={t.used <= 0} onClick={() => set(t.used - 1)} />
        </span>}
      <UsesLeft name={t.name} max={t.max} used={t.used} />
      <span className="v5-ctrls"><IconButton icon="pen" label={`Edit ${t.name}`} onClick={onEdit} /></span>
    </li>
  );
}

export interface TrackerProblem { field: "name" | "max"; message: string }

/** Client-side check before the host's (same limits); null when the entry is fine. */
export function trackerError(name: string, max: string): TrackerProblem | null {
  const n = name.trim();
  if (!n) return { field: "name", message: "Give the tracker a name." };
  if ([...n].length > MAX_TRACKER_NAME) return { field: "name", message: `A tracker name has at most ${MAX_TRACKER_NAME} characters.` };
  const m = Number(max);
  if (!/^\d+$/.test(max.trim()) || m < 1 || m > MAX_TRACKER_MAX) return { field: "max", message: `The maximum is a whole number from 1 to ${MAX_TRACKER_MAX}.` };
  return null;
}

/** Add or edit a tracker: name, maximum and when it resets; editing also offers Delete. */
function TrackerDialog({ tracker, onClose }: { tracker: PlayTracker | null; onClose: () => void }) {
  const { mutate, busy } = useCharacter();
  const [name, setName] = useState(tracker?.name ?? "");
  const [max, setMax] = useState(String(tracker?.max ?? 1));
  const [reset, setReset] = useState<PlayTrackerReset>(tracker?.reset ?? "long_rest");
  const [error, setError] = useState<TrackerProblem | null>(null);
  const save = async (e?: FormEvent) => {
    e?.preventDefault();
    const problem = trackerError(name, max);
    setError(problem);
    if (problem) return;
    const fields = { name: name.trim(), max: Number(max), reset };
    const done = await mutate((api, id) => (tracker
      ? api.updateTracker({ characterId: id, trackerId: tracker.id, ...fields })
      : api.addTracker({ characterId: id, ...fields })));
    if (done) onClose();
  };
  const remove = async () => {
    if (!tracker) return;
    if (await mutate((api, id) => api.removeTracker({ characterId: id, trackerId: tracker.id }))) onClose();
  };
  const errorOf = (field: TrackerProblem["field"]) => (error?.field === field ? error.message : null);
  return (
    <Dialog title={tracker ? "Edit tracker" : "Add tracker"} onClose={onClose} footer={<>
      {tracker && <Button variant="danger" disabled={busy} onClick={() => void remove()}>Delete</Button>}
      <Button onClick={onClose}>Cancel</Button>
      <Button variant="primary" disabled={busy} onClick={() => void save()}>Save</Button>
    </>}>
      <form className="st-form" onSubmit={(e) => void save(e)} noValidate>
        <TextField label="Name" value={name} maxLength={MAX_TRACKER_NAME} autoComplete="off" error={errorOf("name")} onChange={(e) => setName(e.target.value)} />
        <TextField label="Maximum" type="number" inputMode="numeric" min={1} max={MAX_TRACKER_MAX} value={max} error={errorOf("max")} onChange={(e) => setMax(e.target.value)} />
        <SelectField label="Resets on" value={reset} onChange={(e) => setReset(e.target.value as PlayTrackerReset)}>
          {RESET_OPTIONS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
        </SelectField>
        <button type="submit" hidden aria-hidden="true" tabIndex={-1} />
      </form>
    </Dialog>
  );
}

/** The user's own trackers (Drakkenheim contamination, Splicer points): pips or -/+, reset on a short rest, a long rest or never. */
export function TrackersCard({ trackers }: { trackers: PlayTracker[] }) {
  const [editing, setEditing] = useState<PlayTracker | "new" | null>(null);
  return (
    <Expander title="Trackers" headerExtra={<Button small icon="plus" aria-label="Add tracker" onClick={() => setEditing("new")}>Add</Button>}>
      <ul className="v5-list" aria-label="Trackers">
        {trackers.length === 0
          ? <li><EmptyState title="No trackers">Add a tracker for anything else you count, such as contamination or Splicer points.</EmptyState></li>
          : trackers.map((t) => <TrackerRow key={t.id} t={t} onEdit={() => setEditing(t)} />)}
      </ul>
      {editing && <TrackerDialog tracker={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
    </Expander>
  );
}
