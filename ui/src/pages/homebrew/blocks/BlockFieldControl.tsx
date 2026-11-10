import { type ReactNode, useId } from "react";
import type {
  HomebrewBlockField, HomebrewBuilderProblem, HomebrewCondition, HomebrewEquipped, HomebrewFormulaNode, HomebrewProblem, HomebrewTagFilter,
} from "../../../api/contract";
import { CheckBox, ComboBox, EditableComboBox, Icon, NumericUpDown, TextBox } from "../../../components";
import { ConditionBuilder } from "../builders/ConditionBuilder";
import { EquippedBuilder } from "../builders/EquippedBuilder";
import { KeptChip } from "../builders/ExprTree";
import { FormulaBuilder } from "../builders/FormulaBuilder";
import { TagFilterBuilder } from "../builders/TagFilterBuilder";
import { asExprValue } from "../builders/words";
import { namesInScope, useCreatorEnv } from "../env";
import { ChipsField } from "../pickers/ChipsField";
import { RefField } from "../pickers/RefField";
import { StatisticField } from "../pickers/StatisticPicker";
import { BlockList } from "./BlockList";
import {
  AbilityChoiceControl, AbilitySourceControl, ClassCounterControl, NumberOrStatControl, NumberOrWords, NumbersControl, SplitsControl, StatValueControl, UsesControl,
} from "./valueControls";
import { type Fields, asBlocks, asRef, asRefs, defaults, defaultVariant, isEmptyValue, numberRows, startNumber } from "./values";

/** The words every note field carries (spec 6.2). */
export const NOTE_HELP = "Shown as a note; Borealis does not apply it.";

/** Dice text the engine reads: `1d10`, `2d6+1`, `d8`. */
const DICE = /^\s*\d*d\d+(\s*[+-]\s*\d+)?\s*$/i;

const join = (path: string, more: string): string => (path ? `${path}.${more}` : more);
const strip = (problems: readonly HomebrewBuilderProblem[], path: string): HomebrewBuilderProblem[] =>
  problems.filter((p) => p.path.startsWith(`${path}.`)).map((p) => ({ ...p, path: p.path.slice(path.length + 1) }));

export interface BlockFieldProps {
  field: HomebrewBlockField;
  value: unknown;
  onChange: (value: unknown) => void;
  /** The field's path inside the block's fields (`free_cast.uses`), for its problems. */
  path: string;
  /** Problems relative to the block's fields. */
  problems: readonly HomebrewBuilderProblem[];
  /** The other fields of the same object (a select's `type` decides what its filter picks from). */
  siblings: Fields;
}

/** A labelled block field (spec 13.4 FieldHeader over the control): required mark, note help, problems in words. */
export function BlockField(props: BlockFieldProps) {
  const { field, path, problems } = props;
  const env = useCreatorEnv();
  const id = useId();
  if (field.control === "auto" || (field.expert && !env.expert)) return null;
  const own = problems.filter((p) => p.path === path);
  const msgId = own.length ? `${id}-msg` : undefined;
  const wide = ["group", "rows", "variant", "map", "blocks", "tagFilter", "equipped", "formula", "formulas", "condition", "splits", "references"].includes(field.control);
  if (field.control === "check") {
    return (
      <div className={`mg-f hb-field hb-bf ${own.length ? "hb-error" : ""}`} data-field={path}>
        <label className="hb-inline-check"><CheckBox checked={props.value === true} aria-describedby={msgId} onChange={(e) => props.onChange(e.target.checked ? true : undefined)} />{field.label}</label>
        {msgId && <div id={msgId} className="hb-field-msg">{own.map((p, i) => <p key={i}>{p.message}</p>)}</div>}
      </div>
    );
  }
  return (
    <div className={`mg-f hb-field hb-bf ${wide ? "hb-bf-wide" : ""} ${own.length ? "hb-error" : ""}`} data-field={path}>
      <label htmlFor={id} id={`${id}-label`}>
        {own.length > 0 && <Icon name="x" />}{field.label}{field.required ? <span className="hb-req" aria-hidden="true"> *</span> : null}
      </label>
      <FieldControl {...props} inputId={id} describedBy={msgId} invalid={own.length > 0} />
      {msgId && <div id={msgId} className="hb-field-msg">{own.map((p, i) => <p key={i}>{p.message}</p>)}</div>}
      {!msgId && (field.shownAsNote || field.help) && <p className="hb-field-help">{field.shownAsNote ? NOTE_HELP : field.help}</p>}
    </div>
  );
}

/** Several fields of one object (a block, a group, a row, a variant). */
export function FieldList({ fields, values, onValues, path, problems }: {
  fields: readonly HomebrewBlockField[]; values: Fields; onValues: (v: Fields) => void; path: string; problems: readonly HomebrewBuilderProblem[];
}) {
  return (
    <div className="hb-bf-grid">
      {fields.map((f) => (
        <BlockField key={f.key} field={f} value={values[f.key]} siblings={values} path={join(path, f.key)} problems={problems}
          onChange={(v) => {
            const next = { ...values };
            if (v === undefined) delete next[f.key]; else next[f.key] = v;
            onValues(next);
          }} />
      ))}
    </div>
  );
}

/** Problems of a nested block list at `path`, as the list reads a form's (`blocks.N.fields.<key>`). */
function nestedProblems(problems: readonly HomebrewBuilderProblem[], path: string): HomebrewProblem[] {
  return strip(problems, path).map((p) => ({ severity: "error", message: p.message, path: `blocks.${p.path}`, elementId: null, code: "builder" }));
}

const asString = (v: unknown): string => (typeof v === "string" ? v : typeof v === "number" ? String(v) : "");
const asStrings = (v: unknown): string[] => (Array.isArray(v) ? v.map(asString).filter(Boolean) : []);
const asObject = (v: unknown): Fields => (typeof v === "object" && v !== null && !Array.isArray(v) ? v as Fields : {});

/** A stored text where a builder value was expected: shown as kept, removable (spec 6.5). */
function StrayText({ text, onRemove }: { text: string; onRemove: () => void }) {
  return <KeptChip text={text} onRemove={onRemove} />;
}

function FieldControl({ field, value, onChange, path, problems, siblings, inputId, describedBy, invalid }: BlockFieldProps & { inputId: string; describedBy?: string; invalid: boolean }) {
  const env = useCreatorEnv();
  const label = field.label;
  const setOrClear = (v: unknown) => onChange(isEmptyValue(v) ? undefined : v);
  const builderProblems = strip(problems, path);
  switch (field.control) {
    case "text":
      return <TextBox id={inputId} value={asString(value)} aria-describedby={describedBy} invalid={invalid} onChange={(e) => setOrClear(e.target.value)} />;
    case "multiline":
      return <textarea id={inputId} className="hb-multiline" value={asString(value)} aria-describedby={describedBy} aria-invalid={invalid || undefined} onChange={(e) => setOrClear(e.target.value)} />;
    case "dice": {
      const text = asString(value);
      const bad = text !== "" && !DICE.test(text);
      return (
        <>
          <TextBox id={inputId} value={text} placeholder="1d10" aria-describedby={describedBy} invalid={invalid || bad} onChange={(e) => setOrClear(e.target.value)} />
          {bad && <p className="hb-field-help hb-warn-text">Write dice like 1d10 or 2d6+1.</p>}
        </>
      );
    }
    case "number":
      if (field.choices?.length) return <NumberOrWords label={label} value={value as number | string | undefined} onChange={onChange} min={field.min} max={field.max} choices={field.choices} allowCustom={field.allowCustom} />;
      // An optional number that is not set says so; a spinner would show its minimum as if chosen.
      if (!field.required && typeof value !== "number") {
        return <span><button type="button" className="hb-pick-btn" aria-label={`Set: ${label}`} onClick={() => onChange(startNumber(field))}><Icon name="plus" />Set</button></span>;
      }
      return (
        <span className="hb-row-controls">
          <NumericUpDown label={label} value={typeof value === "number" ? value : startNumber(field)} min={field.min ?? 0} max={field.max ?? 999} onChange={onChange} />
          {!field.required && value !== undefined && <button type="button" className="hb-chip-x" aria-label={`Clear ${label}`} onClick={() => onChange(undefined)}><Icon name="x" /></button>}
        </span>
      );
    case "combo":
      if (field.allowCustom) {
        const choices = field.choices ?? [];
        const shown = choices.find((c) => c.value === value)?.label ?? asString(value);
        return <EditableComboBox id={inputId} label={label} value={shown} options={choices.map((c) => c.label)} onChange={(t) => setOrClear(choices.find((c) => c.label === t)?.value ?? t)} />;
      }
      return <ComboBox id={inputId} aria-describedby={describedBy} value={asString(value)} onChange={setOrClear}
        options={[...(field.required ? [] : [{ value: "", label: "Not set" }]), ...(field.choices ?? [])]} />;
    case "chips":
      return <ChipsField id={inputId} label={label} value={asStrings(value)} onChange={setOrClear} choices={field.choices} allowCustom={field.allowCustom} describedBy={describedBy} invalid={invalid} />;
    case "numbers":
      return <NumbersControl label={label} value={Array.isArray(value) ? value as number[] : undefined} onChange={setOrClear} min={field.min} max={field.max} />;
    case "name": {
      const options = field.scope ? namesInScope(env, field.scope) : [];
      if (field.multiple) return <ChipsField id={inputId} label={label} value={asStrings(value)} onChange={setOrClear} allowCustom suggestions={options} describedBy={describedBy} invalid={invalid} />;
      return <EditableComboBox id={inputId} label={label} value={asString(value)} options={options} onChange={setOrClear} />;
    }
    case "statistic":
      return <StatisticField id={inputId} label={label} value={asString(value)} onChange={setOrClear} describedBy={describedBy} invalid={invalid} />;
    case "statValue":
      return <StatValueControl label={label} value={asString(value) || undefined} onChange={setOrClear} />;
    case "numberOrStatistic":
      return <NumberOrStatControl label={label} value={value as never} onChange={onChange} min={field.min ?? 0} max={field.max ?? 99} />;
    case "classCounter":
      return <ClassCounterControl label={label} value={asString(value)} onChange={onChange} />;
    case "uses":
      return <UsesControl label={label} value={value as never} onChange={onChange} />;
    case "abilitySource":
      return <AbilitySourceControl label={label} value={value as never} onChange={onChange} />;
    case "abilityChoice":
      return <AbilityChoiceControl label={label} value={value as never} onChange={onChange} />;
    case "splits":
      return <SplitsControl label={label} value={value as number[][] | undefined} onChange={setOrClear} />;
    case "reference": {
      // A `HomebrewRef` (the id travels, the name is shown; hc-wave2-join).
      const ref = asRef(value);
      return <RefField id={inputId} label={label} target={{ ...(field.picker ?? { types: [] }), multiple: false }} value={ref ? [ref] : []} describedBy={describedBy} invalid={invalid}
        onChange={(refs) => onChange(refs[0])} />;
    }
    case "references": {
      // `HomebrewRef[]`.
      return <RefField id={inputId} label={label} target={{ ...(field.picker ?? { types: [] }), multiple: true }} value={asRefs(value)} describedBy={describedBy} invalid={invalid}
        onChange={(refs) => onChange(refs.length ? refs : undefined)} />;
    }
    case "condition": {
      if (typeof value === "string") return <StrayText text={value} onRemove={() => onChange(undefined)} />;
      return <ConditionBuilder label={label} value={asExprValue<HomebrewCondition>(value)} onChange={setOrClear} problems={builderProblems} />;
    }
    case "tagFilter": {
      if (typeof value === "string") return <StrayText text={value} onRemove={() => onChange(undefined)} />;
      const types = asString(siblings.type) ? [asString(siblings.type)] : [];
      return <TagFilterBuilder label={label} value={asExprValue<HomebrewTagFilter>(value)} onChange={setOrClear} problems={builderProblems} targetTypes={types} />;
    }
    case "equipped": {
      if (typeof value === "string") return <StrayText text={value} onRemove={() => onChange(undefined)} />;
      return <EquippedBuilder label={label} value={asExprValue<HomebrewEquipped>(value)} onChange={setOrClear} problems={builderProblems} />;
    }
    case "formula": {
      if (typeof value === "string") return <StrayText text={value} onRemove={() => onChange(undefined)} />;
      return <FormulaBuilder label={label} value={asExprValue<HomebrewFormulaNode>(value)} onChange={(v) => onChange(v ?? undefined)} dialect={field.dialect ?? "formula"} problems={builderProblems} required={field.required} />;
    }
    case "formulas":
      return <FormulaList field={field} value={value} onChange={onChange} problems={builderProblems} />;
    case "group":
      return <GroupControl field={field} value={value} onChange={onChange} path={path} problems={problems} />;
    case "rows":
      return <RowsControl field={field} value={value} onChange={onChange} path={path} problems={problems} />;
    case "variant":
      return <VariantControl field={field} value={value} onChange={onChange} path={path} problems={problems} />;
    case "map":
      return <MapControl field={field} value={value} onChange={onChange} path={path} problems={problems} />;
    case "blocks": {
      // Nested blocks (a toggle's effects, `HomebrewBlock[]`): their keys are kept, a new one gets a new key, and their
      // problems (`effects.0.fields.value`) show on their cards as `blocks.0.fields.value`.
      return <BlockList label={label} blocks={asBlocks(value)} kinds={field.kinds} problems={nestedProblems(problems, path)} answer={null} nested onChange={setOrClear} />;
    }
    default:
      return <p className="hb-pending">This part can't be edited here.</p>;
  }
}

interface CompositeProps { field: HomebrewBlockField; value: unknown; onChange: (v: unknown) => void; path: string; problems: readonly HomebrewBuilderProblem[] }

/** A list of formulas (`formulas`, each in the field's dialect). */
function FormulaList({ field, value, onChange, problems }: Omit<CompositeProps, "path">) {
  const list = Array.isArray(value) ? value : [];
  const set = (next: unknown[]) => onChange(next.length ? next : undefined);
  return (
    <div className="hb-formula-list">
      {list.map((item, i) => (
        <div key={i} className="hb-formula-item">
          {typeof item === "string"
            ? <StrayText text={item} onRemove={() => set(list.filter((_, j) => j !== i))} />
            : <FormulaBuilder label={`${field.label} ${i + 1}`} value={asExprValue<HomebrewFormulaNode>(item)} dialect={field.dialect ?? "formula"} problems={strip(problems, String(i))}
              onChange={(v) => set(v ? list.map((x, j) => (j === i ? v : x)) : list.filter((_, j) => j !== i))} />}
        </div>
      ))}
      <button type="button" className="hb-pick-btn" onClick={() => set([...list, { value: { shape: "model", model: { term: "number", value: 1 } } }])}><Icon name="plus" />Add a part</button>
    </div>
  );
}

/** A `group`: one object of parts. An optional group is switched on with its check (its label) and off again. */
function GroupControl({ field, value, onChange, path, problems }: CompositeProps) {
  const on = typeof value === "object" && value !== null;
  const inner = (
    <FieldList fields={field.fields ?? []} values={asObject(value)} onValues={(v) => onChange(v)} path={path} problems={problems} />
  );
  if (field.required) return <div className="hb-bf-group">{inner}</div>;
  return (
    <div className="hb-bf-group">
      <label className="hb-inline-check"><CheckBox checked={on} onChange={(e) => onChange(e.target.checked ? defaults(field.fields ?? []) : undefined)} />Use: {field.label.toLowerCase()}</label>
      {on && inner}
    </div>
  );
}

/** `rows`: a list of objects, each with Remove, and Add. Auto ids (a written list's numbers) are written 1..n. */
function RowsControl({ field, value, onChange, path, problems }: CompositeProps) {
  const rows = Array.isArray(value) ? (value as Fields[]) : [];
  const set = (next: Fields[]) => onChange(next.length ? numberRows(field, next) : undefined);
  return (
    <div className="hb-bf-rows">
      {rows.map((row, i) => (
        <div key={i} className="hb-bf-row" role="group" aria-label={`${field.label} ${i + 1}`}>
          <FieldList fields={field.fields ?? []} values={row} onValues={(v) => set(rows.map((r, j) => (j === i ? v : r)))} path={join(path, String(i))} problems={problems} />
          <button type="button" className="hb-chip-x" aria-label={`Remove ${field.label.toLowerCase()} ${i + 1}`} onClick={() => set(rows.filter((_, j) => j !== i))}><Icon name="x" /></button>
        </div>
      ))}
      <button type="button" className="hb-pick-btn" onClick={() => set([...rows, defaults(field.fields ?? [])])}><Icon name="plus" />Add a row</button>
    </div>
  );
}

/** `variant`: an object whose `type` picks one of the shapes (a pool's maximum: fixed, statistic, class table...). */
function VariantControl({ field, value, onChange, path, problems }: CompositeProps) {
  const obj = asObject(value);
  const type = asString(obj.type);
  const variant = field.variants?.find((v) => v.value === type);
  const { type: _t, ...parts } = obj;
  return (
    <div className="hb-bf-group">
      <ComboBox aria-label={`${field.label}: kind`} value={type} onChange={(t) => onChange(t ? defaultVariant(t, field) : undefined)}
        options={[...(field.required ? [] : [{ value: "", label: "Not set" }]), ...(field.variants ?? []).map((v) => ({ value: v.value, label: v.label }))]} />
      {variant && <FieldList fields={variant.fields} values={parts} onValues={(v) => onChange({ type, ...v })} path={path} problems={problems} />}
    </div>
  );
}

/** `map`: keys to values (dice by level, a feat's answers), edited as rows of [key, value]. */
function MapControl({ field, value, onChange, path, problems }: CompositeProps) {
  const [keyField, valueField] = field.fields ?? [];
  const entries = Object.entries(asObject(value));
  if (!keyField || !valueField) return null;
  const toKey = (k: unknown) => asString(k);
  const fromKey = (k: string): unknown => (keyField.control === "number" ? Number(k) : k);
  const set = (next: [unknown, unknown][]) => {
    const obj = Object.fromEntries(next.filter(([k]) => toKey(k) !== "").map(([k, v]) => [toKey(k), v]));
    onChange(Object.keys(obj).length ? obj : undefined);
  };
  const rows: [unknown, unknown][] = entries.map(([k, v]) => [fromKey(k), v]);
  const row = (k: unknown, v: unknown, i: number): ReactNode => (
    <div key={i} className="hb-bf-row" role="group" aria-label={`${field.label} ${i + 1}`}>
      <FieldList fields={[keyField, valueField]} values={{ [keyField.key]: k, [valueField.key]: v }} path={join(path, toKey(k))} problems={problems}
        onValues={(x) => set(rows.map((r, j) => (j === i ? [x[keyField.key], x[valueField.key]] : r)))} />
      <button type="button" className="hb-chip-x" aria-label={`Remove ${field.label.toLowerCase()} ${i + 1}`} onClick={() => set(rows.filter((_, j) => j !== i))}><Icon name="x" /></button>
    </div>
  );
  const nextKey = keyField.control === "number" ? Math.max(keyField.min ?? 1, ...rows.map(([k]) => Number(k) + 1).filter(Number.isFinite)) : "";
  return (
    <div className="hb-bf-rows">
      {rows.map(([k, v], i) => row(k, v, i))}
      <button type="button" className="hb-pick-btn" disabled={keyField.control !== "number" && rows.some(([k]) => toKey(k) === "")}
        onClick={() => set([...rows, [nextKey, defaults([valueField])[valueField.key]]])}><Icon name="plus" />Add a row</button>
    </div>
  );
}

