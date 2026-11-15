import { useState } from "react";
import type { HomebrewCreatureEntry, HomebrewLairOption, HomebrewLairSection, HomebrewLegendary, HomebrewProblem } from "../../../api/contract";
import { Icon, NumericUpDown, TextBox } from "../../../components";
import { noteStatus } from "../statusNote";
import { MgIcon } from "../../manage/mgIcons";
import "../../manage/manage.css";
import { EntryDialog } from "./EntryDialog";
import { entriesOf, entryTitle } from "./creature";

/** The sentences of entry `i` of the field at `path` (`fields.actions.2`). */
const entryProblems = (problems: readonly HomebrewProblem[], path: string, i: number): string[] =>
  problems.filter((p) => p.severity !== "info" && (p.path === `${path}.${i}` || p.path?.startsWith(`${path}.${i}.`))).map((p) => p.message);

type Editing = { index: number; entry: HomebrewCreatureEntry; isNew: boolean };

export interface EntryListProps {
  id?: string;
  label: string;
  /** "Trait", "Action", "Legendary action". */
  noun: string;
  value: unknown;
  onChange: (entries: HomebrewCreatureEntry[]) => void;
  problems: readonly HomebrewProblem[];
  /** The field's path, for its entries' problems. */
  path: string;
  costs?: boolean;
}

/**
 * A creature's traits, actions, bonus actions, reactions or legendary actions (spec 4.11): Aurora's Attacks list editor
 * (13.6), a toolbar with Add, Move up and Move down, one row per entry with Edit and Remove. Alt+Up / Alt+Down move.
 */
export function EntryList({ id, label, noun, value, onChange, problems, path, costs = false }: EntryListProps) {
  const entries = entriesOf(value);
  const [sel, setSel] = useState<number | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const move = (i: number, by: -1 | 1) => {
    const j = i + by;
    if (j < 0 || j >= entries.length) return;
    const next = [...entries];
    [next[i], next[j]] = [next[j]!, next[i]!];
    onChange(next);
    setSel(j);
    noteStatus(`${noun} moved ${by < 0 ? "up" : "down"}.`);
  };
  const remove = (i: number) => {
    onChange(entries.filter((_, j) => j !== i));
    setSel(null);
    noteStatus(`${noun} removed. Undo brings it back.`);
  };
  const done = (e: HomebrewCreatureEntry) => {
    if (!editing) return;
    const next = editing.isNew ? [...entries, e] : entries.map((x, j) => (j === editing.index ? e : x));
    setSel(editing.isNew ? next.length - 1 : editing.index);
    setEditing(null);
    onChange(next);
    noteStatus(editing.isNew ? `${noun} added.` : `${noun} changed.`);
  };
  return (
    <div className="hb-blocks hb-cr-entries" id={id}>
      <div className="mg-toolbar" role="toolbar" aria-label={`${label} commands`}>
        <button type="button" className="mg-tool wide" onClick={() => setEditing({ index: entries.length, entry: { name: "", text: "" }, isNew: true })}>
          <MgIcon name="addAttack" />Add {noun.toLowerCase()}
        </button>
        <button type="button" className="mg-tool" aria-label={`Move selected ${noun.toLowerCase()} up`} disabled={sel === null || sel <= 0} onClick={() => sel !== null && move(sel, -1)}><MgIcon name="moveUp" /></button>
        <button type="button" className="mg-tool" aria-label={`Move selected ${noun.toLowerCase()} down`} disabled={sel === null || sel >= entries.length - 1} onClick={() => sel !== null && move(sel, 1)}><MgIcon name="moveDown" /></button>
      </div>
      {entries.length > 0 && (
        <ul className="atk-list hb-block-list" aria-label={label}>
          {entries.map((e, i) => {
            const bad = entryProblems(problems, path, i);
            const title = entryTitle(e);
            return (
              <li key={i} className={`atk hb-block ${bad.length ? "hb-block-bad" : ""}`} tabIndex={0} aria-current={sel === i} aria-label={title}
                onClick={() => setSel(i)} onFocus={() => setSel(i)} onDoubleClick={() => setEditing({ index: i, entry: e, isNew: false })}
                onKeyDown={(ev) => {
                  if (ev.target !== ev.currentTarget) return;
                  if (ev.altKey && (ev.key === "ArrowUp" || ev.key === "ArrowDown")) { ev.preventDefault(); move(i, ev.key === "ArrowUp" ? -1 : 1); }
                  else if (ev.key === "Enter") { ev.preventDefault(); setEditing({ index: i, entry: e, isNew: false }); }
                  else if (ev.key === "Delete") { ev.preventDefault(); remove(i); }
                }}>
                <div className="info">
                  <h3>{bad.length > 0 && <Icon name="x" />}{title}</h3>
                  <p>{e.text}</p>
                  {bad.map((m, j) => <p key={j} className="hb-block-msg">{m}</p>)}
                </div>
                <span className="btns">
                  <button type="button" aria-label={`Edit: ${title}`} title="Edit (Enter)" onClick={(ev) => { ev.stopPropagation(); setEditing({ index: i, entry: e, isNew: false }); }}><MgIcon name="edit" /></button>
                  <button type="button" aria-label={`Remove: ${title}`} title="Remove (Delete)" onClick={(ev) => { ev.stopPropagation(); remove(i); }}><MgIcon name="cardMinus" /></button>
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {editing && (
        <EntryDialog noun={noun} entry={editing.entry} isNew={editing.isNew} costs={costs} problems={editing.isNew ? [] : entryProblems(problems, path, editing.index)}
          onOk={done} onCancel={() => setEditing(null)} />
      )}
    </div>
  );
}

const legendaryOf = (v: unknown): HomebrewLegendary | null => (typeof v === "object" && v !== null ? { preamble: "", actions: [], ...(v as Partial<HomebrewLegendary>) } : null);

/** Legendary actions: uses per round (more in its lair), the paragraph before them, and the actions with their cost. */
export function LegendaryField({ id, label, value, onChange, problems, path }: {
  id: string; label: string; value: unknown; onChange: (v: HomebrewLegendary | null, typing: boolean) => void; problems: readonly HomebrewProblem[]; path: string;
}) {
  const l = legendaryOf(value);
  if (!l) {
    return <button type="button" id={id} className="hb-pick-btn" onClick={() => onChange({ per_round: 3, preamble: "", actions: [] }, false)}><Icon name="plus" />Add legendary actions</button>;
  }
  const set = (patch: Partial<HomebrewLegendary>, typing = false) => onChange({ ...l, ...patch }, typing);
  return (
    <div className="hb-cr-part" id={id} role="group" aria-label={label}>
      <span className="hb-row-controls">
        <span className="hb-inline-label">Uses each round</span>
        <NumericUpDown label="Legendary action uses" value={l.per_round ?? 3} min={1} max={9} compact onChange={(n) => set({ per_round: n })} />
        <span className="hb-inline-label">In its lair</span>
        <NumericUpDown label="Legendary action uses in its lair" value={l.lair_uses ?? l.per_round ?? 3} min={1} max={9} compact
          onChange={(n) => set({ lair_uses: n === (l.per_round ?? 3) ? undefined : n })} />
        <button type="button" className="hb-chip-x" aria-label="Remove legendary actions" onClick={() => onChange(null, false)}><Icon name="x" /></button>
      </span>
      <textarea className="hb-multiline" aria-label="Before the legendary actions" value={l.preamble} onChange={(e) => set({ preamble: e.target.value }, true)} />
      <EntryList label="Legendary actions" noun="Legendary action" value={l.actions} onChange={(actions) => set({ actions })} problems={problems} path={`${path}.actions`} costs />
    </div>
  );
}

/** Lair actions or regional effects: the paragraph before, the options (a name when it has one, and its text), the one after. */
export function LairField({ id, label, value, onChange }: { id: string; label: string; value: unknown; onChange: (v: HomebrewLairSection | null, typing: boolean) => void }) {
  if (typeof value !== "object" || value === null) {
    return <button type="button" id={id} className="hb-pick-btn" onClick={() => onChange({ preamble: "", options: [], closing: "" }, false)}><Icon name="plus" />Add {label.toLowerCase()}</button>;
  }
  const s = value as HomebrewLairSection;
  const options = s.options ?? [];
  const set = (patch: Partial<HomebrewLairSection>, typing = true) => onChange({ ...s, ...patch }, typing);
  const setOption = (i: number, o: HomebrewLairOption) => set({ options: options.map((x, j) => (j === i ? o : x)) });
  return (
    <div className="hb-cr-part" id={id} role="group" aria-label={label}>
      <span className="hb-row-controls">
        <span className="hb-inline-label">Before</span>
        <button type="button" className="hb-chip-x" aria-label={`Remove ${label.toLowerCase()}`} onClick={() => onChange(null, false)}><Icon name="x" /></button>
      </span>
      <textarea className="hb-multiline" aria-label={`${label}: before the options`} value={s.preamble ?? ""} onChange={(e) => set({ preamble: e.target.value })} />
      <ol className="hb-list-items">
        {options.map((o, i) => (
          <li key={i} className="hb-list-item hb-cr-lair-option">
            <TextBox aria-label={`${label} ${i + 1}: name`} placeholder="Name (if it has one)" value={o.name ?? ""} onChange={(e) => setOption(i, { ...o, name: e.target.value || undefined })} />
            <textarea className="hb-multiline" aria-label={`${label} ${i + 1}: text`} value={o.text} onChange={(e) => setOption(i, { ...o, text: e.target.value })} />
            <button type="button" className="hb-chip-x" aria-label={`Remove ${label.toLowerCase()} ${i + 1}`} onClick={() => set({ options: options.filter((_, j) => j !== i) }, false)}><Icon name="x" /></button>
          </li>
        ))}
      </ol>
      <button type="button" className="hb-pick-btn" onClick={() => set({ options: [...options, { text: "" }] }, false)}><Icon name="plus" />Add an option</button>
      <textarea className="hb-multiline" aria-label={`${label}: after the options`} value={s.closing ?? ""} onChange={(e) => set({ closing: e.target.value })} />
    </div>
  );
}
