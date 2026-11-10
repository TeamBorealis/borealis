import { useState } from "react";
import type { HomebrewMapsToValue, HomebrewRef, HomebrewSetterRow, HomebrewSheetLine, HomebrewSheetValue } from "../../../api/contract";
import { CheckBox, ComboBox, EditableComboBox, Icon, NumericUpDown, TextBox } from "../../../components";
import { useCreatorEnv } from "../env";
import { RefField } from "../pickers/RefField";
import { StatisticPicker, statLabel } from "../pickers/StatisticPicker";

/** A sheet line's parts: text, and values written as `{{statistic}}` (spec 7.4). */
type Part = { text: string } | { stat: string };

const PLACEHOLDER = /\{\{([^{}]*)\}\}/g;

export function templateParts(text: string): Part[] {
  const out: Part[] = [];
  let at = 0;
  for (const m of text.matchAll(PLACEHOLDER)) {
    if (m.index! > at) out.push({ text: text.slice(at, m.index) });
    out.push({ stat: m[1]!.trim() });
    at = m.index! + m[0].length;
  }
  if (at < text.length || out.length === 0) out.push({ text: text.slice(at) });
  return out;
}

export const templateText = (parts: readonly Part[]): string => parts.map((p) => ("stat" in p ? `{{${p.stat}}}` : p.text)).join("");

/** Literal braces are refused in these fields (spec 7.4): a value goes in through Insert value. */
const noBraces = (s: string): string => s.replace(/[{}]/g, "");

/**
 * Plain text with value chips (spec 7.4: sheet lines, name formats): typed text never needs `{`, a value is inserted
 * from the statistic picker and shown by its words ("Rages"), never as `{{barbarian rage:count}}`.
 */
export function TemplateText({ label, value, onChange }: { label: string; value: string; onChange: (text: string, typing: boolean) => void }) {
  const env = useCreatorEnv();
  const [picking, setPicking] = useState(false);
  const parts = templateParts(value);
  const set = (next: Part[], typing: boolean) => onChange(templateText(next), typing);
  return (
    <span className="hb-template" role="group" aria-label={label}>
      {parts.map((p, i) => ("stat" in p ? (
        <span key={i} className="hb-chip-item hb-value-chip">
          <span className="hb-ref-name">{statLabel(env, p.stat)}</span>
          <button type="button" className="hb-chip-x" aria-label={`Remove the value ${statLabel(env, p.stat)}`} onClick={() => set(parts.filter((_, j) => j !== i), false)}><Icon name="x" /></button>
        </span>
      ) : (
        <TextBox key={i} aria-label={i === 0 ? label : `${label}, text ${i + 1}`} value={p.text} className="hb-template-text"
          onChange={(e) => set(parts.map((x, j) => (j === i ? { text: noBraces(e.target.value) } : x)), true)} />
      )))}
      <button type="button" className="hb-pick-btn" onClick={() => setPicking(true)}><Icon name="plus" />Insert value</button>
      {picking && <StatisticPicker title="Insert value" onClose={() => setPicking(false)} onChoose={(stat) => { setPicking(false); set([...parts, { stat }, { text: "" }], false); }} />}
    </span>
  );
}

const ACTIONS = ["Action", "Bonus Action", "Reaction"];
const EMPTY_SHEET: HomebrewSheetValue = { display: null, alt: null, usage: null, action: null, lines: [] };
const orNull = (s: string): string | null => (s.trim() ? s : null);

/** "On the character sheet" (spec 3.2): listed or not, short name, usage, action, and level-scaled lines with values. */
export function SheetField({ id, value, onChange }: { id?: string; value: HomebrewSheetValue | null; onChange: (v: HomebrewSheetValue, typing: boolean) => void }) {
  const v = value ?? EMPTY_SHEET;
  const set = (patch: Partial<HomebrewSheetValue>, typing = false) => onChange({ ...v, ...patch }, typing);
  const setLine = (i: number, line: HomebrewSheetLine, typing = false) => set({ lines: v.lines.map((l, j) => (j === i ? line : l)) }, typing);
  return (
    <div className="hb-sheet" id={id}>
      <label className="hb-inline-check"><CheckBox checked={v.display !== false} onChange={(e) => set({ display: e.target.checked ? null : false })} />Show on the character sheet</label>
      <div className="mg-grid two">
        <div className="mg-f"><label htmlFor={`${id}-alt`}>Short name</label><TextBox id={`${id}-alt`} value={v.alt ?? ""} onChange={(e) => set({ alt: orNull(e.target.value) }, true)} /></div>
        <div className="mg-f"><label htmlFor={`${id}-usage`}>Usage</label><TextBox id={`${id}-usage`} placeholder="1/Long Rest" value={v.usage ?? ""} onChange={(e) => set({ usage: orNull(e.target.value) }, true)} /></div>
        <div className="mg-f"><label htmlFor={`${id}-action`}>Action</label><EditableComboBox id={`${id}-action`} label="Action" value={v.action ?? ""} options={ACTIONS} onChange={(a) => set({ action: orNull(a) }, true)} /></div>
      </div>
      <div className="hb-bf-rows">
        {v.lines.map((line, i) => (
          <div key={i} className="hb-bf-row" role="group" aria-label={`Sheet line ${i + 1}`}>
            <TemplateText label={`Sheet line ${i + 1}`} value={line.text} onChange={(text, typing) => setLine(i, { ...line, text }, typing)} />
            <span className="hb-row-controls">
              <span className="hb-inline-label">From level</span>
              <NumericUpDown label={`Sheet line ${i + 1}: from level`} value={line.level ?? 1} min={1} max={20} compact onChange={(n) => setLine(i, { ...line, level: n })} />
            </span>
            <button type="button" className="hb-chip-x" aria-label={`Remove sheet line ${i + 1}`} onClick={() => set({ lines: v.lines.filter((_, j) => j !== i) })}><Icon name="x" /></button>
          </div>
        ))}
        <button type="button" className="hb-pick-btn" onClick={() => set({ lines: [...v.lines, { text: "", level: null, usage: null, action: null }] })}><Icon name="plus" />Add a sheet line</button>
      </div>
    </div>
  );
}

/** "Other properties" (spec 4.12, expert): setters as name / value rows, attributes as name / value pairs. */
export function SettersField({ id, value, onChange }: { id?: string; value: readonly HomebrewSetterRow[]; onChange: (rows: HomebrewSetterRow[], typing: boolean) => void }) {
  const setRow = (i: number, row: HomebrewSetterRow, typing = true) => onChange(value.map((r, j) => (j === i ? row : r)), typing);
  return (
    <div className="hb-bf-rows" id={id}>
      {value.map((row, i) => (
        <div key={i} className="hb-bf-row" role="group" aria-label={`Property ${i + 1}`}>
          <span className="hb-row-controls hb-wrap">
            <TextBox aria-label={`Property ${i + 1}: name`} placeholder="Name" value={row.name} onChange={(e) => setRow(i, { ...row, name: e.target.value })} />
            <TextBox aria-label={`Property ${i + 1}: value`} placeholder="Value" value={row.value} onChange={(e) => setRow(i, { ...row, value: e.target.value })} />
            {Object.entries(row.attributes).map(([k, a]) => (
              <span key={k} className="hb-chip-item">
                <span className="hb-ref-name">{k}: {a}</span>
                <button type="button" className="hb-chip-x" aria-label={`Remove ${k} from property ${i + 1}`} onClick={() => { const { [k]: _gone, ...rest } = row.attributes; setRow(i, { ...row, attributes: rest }, false); }}><Icon name="x" /></button>
              </span>
            ))}
          </span>
          <button type="button" className="hb-chip-x" aria-label={`Remove property ${i + 1}`} onClick={() => onChange(value.filter((_, j) => j !== i), false)}><Icon name="x" /></button>
        </div>
      ))}
      <button type="button" className="hb-pick-btn" onClick={() => onChange([...value, { name: "", value: "", attributes: {} }], false)}><Icon name="plus" />Add a property</button>
    </div>
  );
}

const KINDS = [{ value: "", label: "Not set" }, { value: "equivalent", label: "Equivalent" }, { value: "approximate", label: "Approximate" }, { value: "none", label: "No counterpart" }];

/** "2014/2024 counterpart" (expert, spec 3.2): an element of each edition and how close it is. */
export function MapsToField({ id, value, onChange }: { id?: string; value: HomebrewMapsToValue | null; onChange: (v: HomebrewMapsToValue | null) => void }) {
  const v = value ?? { dnd2014: null, dnd2024: null, kind: null };
  const set = (patch: Partial<HomebrewMapsToValue>) => {
    const next = { ...v, ...patch };
    onChange(!next.dnd2014 && !next.dnd2024 && !next.kind ? null : next);
  };
  const one = (r: HomebrewRef | null) => (r ? [r] : []);
  return (
    <div className="mg-grid two" id={id}>
      <div className="mg-f"><label>2014 element</label><RefField label="2014 counterpart" target={{ types: [] }} value={one(v.dnd2014)} onChange={(r) => set({ dnd2014: r[0] ?? null })} /></div>
      <div className="mg-f"><label>2024 element</label><RefField label="2024 counterpart" target={{ types: [] }} value={one(v.dnd2024)} onChange={(r) => set({ dnd2024: r[0] ?? null })} /></div>
      <div className="mg-f"><label htmlFor={`${id}-kind`}>How close</label><ComboBox id={`${id}-kind`} value={v.kind ?? ""} options={KINDS} onChange={(k) => set({ kind: (k || null) as HomebrewMapsToValue["kind"] })} /></div>
    </div>
  );
}
