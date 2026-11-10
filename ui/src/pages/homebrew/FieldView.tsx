import { type ReactNode } from "react";
import type { HomebrewFieldSpec, HomebrewProblem } from "../../api/contract";
import { CheckBox, ComboBox, EditableComboBox, Icon, NumericUpDown, TextBox, ToggleSwitch } from "../../components";
import { fieldDomId, fieldSeverity, problemsFor } from "./problems";

export interface FieldViewProps {
  field: HomebrewFieldSpec;
  value: unknown;
  problems: readonly HomebrewProblem[];
  /** `typing` groups keystrokes in this field into one undo step (spec 10.2). */
  onChange: (value: unknown, typing: boolean) => void;
}

const text = (v: unknown): string => (typeof v === "string" ? v : v === null || v === undefined ? "" : String(v));

/**
 * One form field in the Manage > Character form style (spec 13.4): FieldHeader label (10 SemiBold uppercase) over the
 * control. A field with problems turns its label warning orange or danger red with an icon, and its sentences sit under it,
 * linked with `aria-describedby` (8.3). The controls that later items own (pickers, builders, rich text, blocks: WB-10)
 * show a note until they land.
 */
export function FieldView({ field, value, problems, onChange }: FieldViewProps) {
  const own = problemsFor(field, problems);
  const severity = fieldSeverity(own);
  const inputId = `${fieldDomId(field.key)}-input`;
  const msgId = own.length > 0 ? `${fieldDomId(field.key)}-msg` : undefined;
  return (
    <div className={`mg-f hb-field ${severity ? `hb-${severity}` : ""}`} id={fieldDomId(field.key)} data-field={field.key}>
      <label htmlFor={inputId}>
        {severity && <Icon name={severity === "error" ? "x" : "alert"} />}
        {field.label}{field.required ? <span className="hb-req" aria-hidden="true"> *</span> : null}
      </label>
      {control(field, value, onChange, inputId, msgId, severity === "error")}
      {msgId && <div id={msgId} className="hb-field-msg">{own.map((p, i) => <p key={i}>{p.message}</p>)}</div>}
      {!msgId && field.help && <p className="hb-field-help">{field.help}</p>}
    </div>
  );
}

function control(field: HomebrewFieldSpec, value: unknown, onChange: FieldViewProps["onChange"], id: string, describedBy: string | undefined, invalid: boolean): ReactNode {
  switch (field.control) {
    case "text":
      return <TextBox id={id} value={text(value)} aria-describedby={describedBy} invalid={invalid} aria-required={field.required || undefined} onChange={(e) => onChange(e.target.value, true)} />;
    case "multiline":
      return <textarea id={id} className="hb-multiline" value={text(value)} aria-describedby={describedBy} aria-invalid={invalid || undefined} onChange={(e) => onChange(e.target.value, true)} />;
    case "number":
      return (
        <span id={id} className="hb-number" aria-describedby={describedBy}>
          <NumericUpDown label={field.label} value={typeof value === "number" ? value : field.min ?? 0} min={field.min ?? 0} max={field.max ?? 999} onChange={(n) => onChange(n, false)} />
        </span>
      );
    case "combo":
      if (field.allowCustom) {
        return <EditableComboBox id={id} label={field.label} value={text(value)} options={(field.choices ?? []).map((c) => c.label).filter(Boolean)} onChange={(v) => onChange(v, true)} />;
      }
      if (!field.choices?.length) return <TextBox id={id} value={text(value)} aria-describedby={describedBy} invalid={invalid} onChange={(e) => onChange(e.target.value, true)} />;
      return <ComboBox id={id} options={field.choices} value={text(value)} aria-describedby={describedBy} onChange={(v) => onChange(v, false)} />;
    case "check":
      return <span className="hb-check"><CheckBox id={id} checked={value === true} aria-describedby={describedBy} onChange={(e) => onChange(e.target.checked, false)} /></span>;
    case "toggle":
      return <span id={id}><ToggleSwitch label={field.label} checked={value === true} onChange={(v) => onChange(v, false)} /></span>;
    default:
      return <p id={id} className="hb-pending" tabIndex={-1}>This part is edited with a control that is not in this build yet.</p>;
  }
}
