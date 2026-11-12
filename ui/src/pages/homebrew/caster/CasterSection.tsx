import { type ReactNode, useRef } from "react";
import type { HomebrewFieldSpec, HomebrewForm, HomebrewLevelCounts, HomebrewProblem, Ruleset } from "../../../api/contract";
import { CheckBox } from "../../../components";
import { FieldView } from "../FieldView";
import { problemsFor } from "../problems";
import "../class/class.css";
import "./caster.css";
import { usePinnedHead } from "../class/pinnedHead";
import {
  type CasterType, type SlotGrid, LEVEL_COUNT, gridOf, levelsOf, slotGrid, topSlotLevel, withCell, withLevels,
} from "./slots";

export interface CasterSectionProps {
  /** The kind's SPELLCASTING fields. */
  fields: readonly HomebrewFieldSpec[];
  /** The form holding the section (a class's Spellcasting feature, a feat). */
  form: HomebrewForm;
  /** Its problems, at paths relative to it (`fields.ability`). */
  problems: readonly HomebrewProblem[];
  /** The edition it writes in (the class's). */
  edition: Ruleset | null;
  expert: boolean;
  /** Prefix of the DOM ids (a class's feature inside the class form). */
  idPrefix?: string;
  /** One undoable edit of several fields; `group` merges typing (spec 10.2). */
  onFields: (patch: Record<string, unknown>, group: string | null) => void;
}

const ORDINALS = ["1st", "2nd", "3rd", "4th", "5th", "6th", "7th", "8th", "9th"];
const LEVELS = Array.from({ length: LEVEL_COUNT }, (_, i) => i + 1);

/** A cell as typed: a whole number 0-99 (empty = 0). */
const cellNumber = (text: string, max: number): number => {
  const n = Number.parseInt(text.trim() || "0", 10);
  return Number.isFinite(n) ? Math.min(Math.max(n, 0), max) : 0;
};

/** One count column of the table: a count field and its heading. */
interface CountColumn {
  field: HomebrewFieldSpec;
  heading: string;
}

/**
 * The spellcasting section (spec 4.6; Manage form sections + the class table grid, U7): "Gives spellcasting", then the
 * ability, how it gets its spells, its list and swapping; the caster type with its slots; and one grid of 20 levels
 * with the counts (cantrips, prepared or known spells) and the spell slots, editable for a custom table. Always-prepared
 * spells, ritual casting and a spellbook are rule blocks (the feature's rules below or on RULES).
 */
export function CasterSection({ fields, form, problems, edition, expert, idPrefix = "", onFields }: CasterSectionProps) {
  const byKey = new Map(fields.map((f) => [f.key, f]));
  const v = (key: string): unknown => form.fields[key];
  const view = (key: string, onChange?: (value: unknown, typing: boolean) => void): ReactNode => {
    const f = byKey.get(key);
    if (!f || (f.expert && !expert) || (f.editions && edition && !f.editions.includes(edition))) return null;
    return <FieldView key={key} field={f} value={v(key)} problems={problems} idPrefix={idPrefix}
      onChange={onChange ?? ((value, typing) => onFields({ [key]: value }, typing ? `${idPrefix}${key}` : null))} />;
  };
  const on = v("casts") === true;
  if (!on) {
    return <div className="hb-section hb-caster"><div className="mg-grid two">{view("casts")}</div></div>;
  }
  const type = (typeof v("casterType") === "string" ? v("casterType") : "") as CasterType;
  const preparation = v("preparation");
  const setType = (next: unknown) => {
    const patch: Record<string, unknown> = { casterType: next };
    // a custom table starts from the slots of the type it replaces
    if (next === "custom" && !gridOf(v("slotTable"))) patch.slotTable = slotGrid(type === "custom" ? "" : type);
    onFields(patch, null);
  };
  const formula = typeof v("prepareFormula") === "string" ? v("prepareFormula") : "";
  const columns: CountColumn[] = [];
  const add = (key: string, heading: string) => { const f = byKey.get(key); if (f) columns.push({ field: f, heading }); };
  add("cantrips", "Cantrips");
  if (preparation === "known") add("known", "Spells known");
  else if (edition !== "dnd2014" || formula === "") add("prepared", "Prepared");
  return (
    <div className="hb-caster">
      <div className="hb-section">
        <h3 className="hb-section-title">Spellcasting</h3>
        <div className="mg-grid two">
          {view("casts")}{view("casterName")}{view("ability")}{view("preparation")}{view("spellList")}{view("swap")}
        </div>
      </div>
      <div className="hb-section">
        <h3 className="hb-section-title">Spell slots</h3>
        <div className="mg-grid two">
          {view("casterType", (next) => setType(next))}{view("multiclassSlots")}
          {view("prepareFormula")}{view("preparedChange")}
        </div>
      </div>
      <div className="hb-section">
        <h3 className="hb-section-title">Spells by level</h3>
        <CasterTable type={type} custom={v("slotTable")} columns={columns} form={form} problems={problems} idPrefix={idPrefix}
          onCount={(key, value) => onFields({ [key]: value }, `${idPrefix}${key}:cells`)}
          onSlots={(grid) => onFields({ slotTable: grid }, `${idPrefix}slotTable:cells`)} />
        {expert && <CountFlags columns={columns} form={form} idPrefix={idPrefix} onCount={(key, value) => onFields({ [key]: value }, null)} />}
      </div>
    </div>
  );
}

/** The grid: LEVEL, the count columns, then the slots of each slot level (all nine for a custom table). */
function CasterTable({ type, custom, columns, form, problems, idPrefix, onCount, onSlots }: {
  type: CasterType; custom: unknown; columns: readonly CountColumn[]; form: HomebrewForm; problems: readonly HomebrewProblem[]; idPrefix: string;
  onCount: (key: string, value: unknown) => void; onSlots: (grid: SlotGrid) => void;
}) {
  const editable = type === "custom";
  const grid = slotGrid(type, custom);
  const shownSlots = editable ? 9 : topSlotLevel(grid);
  const counts = columns.map((c) => ({ ...c, levels: levelsOf(form.fields[c.field.key]) }));
  const messages = [...columns.map((c) => c.field.key), "slotTable"].flatMap((key) =>
    problemsFor({ key } as HomebrewFieldSpec, problems).map((p) => p.message));
  const setCount = (key: string, levels: number[], level: number, n: number) =>
    onCount(key, withLevels(form.fields[key], levels.map((x, i) => (i === level - 1 ? n : x))));
  const gridFrame = useRef<HTMLDivElement>(null);
  // HC10 Q3: the header row stays in view while the grid scrolls past
  usePinnedHead(gridFrame, [shownSlots, columns.length]);
  return (
    <div className="hb-levels-wrap" id={`hb-f-${idPrefix}slotTable`} ref={gridFrame}>
      <table className="ap-grid hb-levels hb-caster-table" aria-label="Spell slots and spells by level">
        <thead>
          <tr>
            <th scope="col">Level</th>
            {counts.map((c) => <th scope="col" key={c.field.key}>{c.heading}</th>)}
            {Array.from({ length: shownSlots }, (_, k) => <th scope="col" key={`s${k}`} className="hb-slot-col">{ORDINALS[k]}</th>)}
          </tr>
        </thead>
        <tbody>
          {LEVELS.map((level) => (
            <tr key={level} data-level={level}>
              <th scope="row">{level}</th>
              {counts.map((c) => (
                <td key={c.field.key} className="hb-counter">
                  <input aria-label={`${c.heading} at level ${level}`} inputMode="numeric" value={String(c.levels[level - 1] ?? 0)}
                    onChange={(e) => setCount(c.field.key, c.levels, level, cellNumber(e.target.value, 99))} />
                </td>
              ))}
              {Array.from({ length: shownSlots }, (_, k) => {
                const n = grid[level - 1]?.[k] ?? 0;
                return editable
                  ? <td key={`s${k}`} className="hb-counter"><input aria-label={`${ORDINALS[k]}-level slots at level ${level}`} inputMode="numeric" value={String(n)}
                    onChange={(e) => onSlots(withCell(grid, level, k + 1, cellNumber(e.target.value, 9)))} /></td>
                  : <td key={`s${k}`} className="hb-slot-cell" data-slot={k + 1}>{n === 0 ? "-" : n}</td>;
              })}
            </tr>
          ))}
        </tbody>
      </table>
      {type === "" && <p className="hb-field-help">No spell slots: choose a caster type for them.</p>}
      {type === "pact" && <p className="hb-field-help">Pact magic slots are all of one level and come back on a short rest.</p>}
      {messages.length > 0 && <div className="hb-field hb-error"><div className="hb-field-msg" role="alert">{messages.map((m, i) => <p key={i}>{m}</p>)}</div></div>}
    </div>
  );
}

/** Expert: the count statistics (`cantrips known`, `spells known`) and replaceable spell picks. */
function CountFlags({ columns, form, idPrefix, onCount }: {
  columns: readonly CountColumn[]; form: HomebrewForm; idPrefix: string; onCount: (key: string, value: unknown) => void;
}) {
  const flagged = columns.filter((c) => c.field.key === "cantrips" || c.field.key === "known");
  return (
    <div className="hb-caster-flags">
      {flagged.map(({ field, heading }) => {
        const value = (form.fields[field.key] ?? { levels: levelsOf(undefined), statistic: false }) as HomebrewLevelCounts;
        const id = `${idPrefix}${field.key}`;
        return (
          <span key={field.key} className="hb-caster-flag">
            <label htmlFor={`hb-${id}-stat`}><CheckBox id={`hb-${id}-stat`} checked={value.statistic === true}
              onChange={(e) => onCount(field.key, { ...value, statistic: e.target.checked })} />{heading}: also written as a statistic</label>
            {field.key === "known" && (
              <label htmlFor={`hb-${id}-replace`}><CheckBox id={`hb-${id}-replace`} checked={value.replace === true}
                onChange={(e) => onCount(field.key, { ...value, replace: e.target.checked })} />Each pick can be swapped later</label>
            )}
          </span>
        );
      })}
    </div>
  );
}
