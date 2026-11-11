import { type ReactNode } from "react";
import type {
  HomebrewBlock, HomebrewCondition, HomebrewEquipped, HomebrewFieldSpec, HomebrewFormulaNode, HomebrewMapsToValue, HomebrewProblem, HomebrewSetterRow, HomebrewSheetValue,
  HomebrewTagFilter,
} from "../../api/contract";
import { CheckBox, ComboBox, EditableComboBox, Icon, NumericUpDown, TextBox, ToggleSwitch } from "../../components";
import { BlockList, REPEAT_KINDS } from "./blocks/BlockList";
import { NumbersControl } from "./blocks/valueControls";
import type { ChildEditing } from "./class/ChildDialog";
import { ChildrenField, PackageField, SkillChoiceField } from "./class/ClassParts";
import { LevelsGrid } from "./class/LevelsGrid";
import { ConditionBuilder } from "./builders/ConditionBuilder";
import { EquippedBuilder } from "./builders/EquippedBuilder";
import { FormulaBuilder } from "./builders/FormulaBuilder";
import { TagFilterBuilder } from "./builders/TagFilterBuilder";
import { asExprValue, relativeProblems } from "./builders/words";
import { useCreatorEnv } from "./env";
import { ChoicesField, FeatGrantField, WrittenListField } from "./fields/BackgroundParts";
import { MapsToField, SettersField, SheetField } from "./fields/FormParts";
import { ContentsField, contentsOf, optionsOf, PackageOptionsField } from "./fields/ItemParts";
import { SpellListsField } from "./fields/SpellListsField";
import { useWords } from "./fields/useWords";
import { ChipsField } from "./pickers/ChipsField";
import { RefField, refsOf } from "./pickers/RefField";
import { StatisticField } from "./pickers/StatisticPicker";
import { fieldDomId, fieldSeverity, problemsFor } from "./problems";
import { RichTextEditor } from "./rich/RichTextEditor";
import type { DocJson } from "./rich/types";
import "./parts.css";

export interface FieldViewProps {
  field: HomebrewFieldSpec;
  value: unknown;
  problems: readonly HomebrewProblem[];
  /** `typing` groups keystrokes in this field into one undo step (spec 10.2). */
  onChange: (value: unknown, typing: boolean) => void;
  /** The form's kind, for the description toolbar's extras (spell, class). */
  kindId?: string;
  /** Prefix of the field's DOM ids (a child's fields in its dialog over the parent's form, WB-16). */
  idPrefix?: string;
  /** The form and its edits, for fields that hold children (the class LEVELS grid, a package, sub-features; WB-16). */
  childEditing?: ChildEditing;
}

const text = (v: unknown): string => (typeof v === "string" ? v : v === null || v === undefined ? "" : String(v));
const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);

/** Builder controls show their row problems on the rows; under the field only the field's own sentences. */
const BUILDERS: ReadonlySet<HomebrewFieldSpec["control"]> = new Set(["condition", "tagFilter", "equipped", "formula", "blocks"]);
/** Controls that take the whole row of the form grid. */
const WIDE: ReadonlySet<HomebrewFieldSpec["control"]> = new Set(["condition", "tagFilter", "equipped", "formula", "richText", "sheet", "setters", "mapsTo", "blocks", "references", "contents", "packageOptions", "spellLists", "levels", "package", "children", "skillChoice", "numbers",
  "featGrant", "choices", "writtenList"]);

/** A closed combo's choices, with the stored value kept as a choice when content holds one the list doesn't (a homebrew
 * school), and a blank first choice while nothing is chosen (a new spell's school). */
function withCurrent(choices: readonly { value: string; label: string }[], current: string): { value: string; label: string }[] {
  const all = [...choices];
  if (current !== "" && !all.some((c) => c.value === current)) all.push({ value: current, label: current });
  if (current === "" && !all.some((c) => c.value === "")) all.unshift({ value: "", label: "" });
  return all;
}

/**
 * One form field in the Manage > Character form style (spec 13.4): FieldHeader label (10 SemiBold uppercase) over the
 * control. A field with problems turns its label warning orange or danger red with an icon, and its sentences sit under it,
 * linked with `aria-describedby` (8.3). Read-only fields (the Source label) are shown, not edited.
 */
export function FieldView({ field, value, problems, onChange, kindId, idPrefix = "", childEditing }: FieldViewProps) {
  const own = problemsFor(field, problems);
  const severity = fieldSeverity(own);
  const shown = BUILDERS.has(field.control) ? own.filter((p) => p.path === `fields.${field.key}`) : own;
  const domId = fieldDomId(`${idPrefix}${field.key}`);
  const inputId = `${domId}-input`;
  const msgId = shown.length > 0 ? `${domId}-msg` : undefined;
  const wide = WIDE.has(field.control);
  if (field.control === "check" && !field.readOnly) {
    return (
      <div className={`mg-f hb-field ${severity ? `hb-${severity}` : ""}`} id={domId} data-field={field.key}>
        <label htmlFor={inputId}>{severity && <Icon name={severity === "error" ? "x" : "alert"} />}{field.label}</label>
        <span className="hb-check"><CheckBox id={inputId} checked={value === true} aria-describedby={msgId} onChange={(e) => onChange(e.target.checked, false)} /></span>
        {msgId && <div id={msgId} className="hb-field-msg">{shown.map((p, i) => <p key={i}>{p.message}</p>)}</div>}
        {!msgId && field.help && <p className="hb-field-help">{field.help}</p>}
      </div>
    );
  }
  return (
    <div className={`mg-f hb-field ${wide ? "hb-wide" : ""} ${severity ? `hb-${severity}` : ""}`} id={domId} data-field={field.key}>
      <label htmlFor={inputId}>
        {severity && <Icon name={severity === "error" ? "x" : "alert"} />}
        {field.label}{field.required ? <span className="hb-req" aria-hidden="true"> *</span> : null}
      </label>
      {field.readOnly
        ? <p id={inputId} className="hb-readonly" tabIndex={-1}>{(Array.isArray(value) ? value.join(", ") : text(value)) || "-"}</p>
        : <Control field={field} value={value} onChange={onChange} id={inputId} describedBy={msgId} invalid={severity === "error"} problems={problems} kindId={kindId} childEditing={childEditing} />}
      {msgId && <div id={msgId} className="hb-field-msg">{shown.map((p, i) => <p key={i}>{p.message}</p>)}</div>}
      {!msgId && field.help && <p className="hb-field-help">{field.help}</p>}
    </div>
  );
}

/** A combo that takes typed text: the field's choices, then the words its picker type offers (the generic form's open
 * types, WB-19), each once. */
function OpenCombo({ field, id, value, onChange }: { field: HomebrewFieldSpec; id: string; value: string; onChange: (v: string) => void }) {
  const loaded = useWords(field.picker?.types ?? []);
  const own = (field.choices ?? []).map((c) => c.label).filter(Boolean);
  const options = [...own, ...loaded.filter((w) => !own.includes(w))];
  return <EditableComboBox id={id} label={field.label} value={value} options={options} onChange={onChange} />;
}

interface ControlProps {
  field: HomebrewFieldSpec; value: unknown; onChange: FieldViewProps["onChange"]; id: string; describedBy: string | undefined; invalid: boolean;
  problems: readonly HomebrewProblem[]; kindId?: string; childEditing?: ChildEditing;
}

function Control({ field, value, onChange, id, describedBy, invalid, problems, kindId, childEditing }: ControlProps): ReactNode {
  const env = useCreatorEnv();
  const label = field.label;
  const builderProblems = relativeProblems(problems, `fields.${field.key}`);
  switch (field.control) {
    case "text":
      return <TextBox id={id} value={text(value)} aria-describedby={describedBy} invalid={invalid} aria-required={field.required || undefined} onChange={(e) => onChange(e.target.value, true)} />;
    case "multiline":
      return <textarea id={id} className="hb-multiline" value={text(value)} aria-describedby={describedBy} aria-invalid={invalid || undefined} onChange={(e) => onChange(e.target.value, true)} />;
    case "number":
      return (
        <span id={id} className="hb-number" aria-describedby={describedBy}>
          <NumericUpDown label={label} value={typeof value === "number" ? value : Math.min(Math.max(0, field.min ?? 0), field.max ?? 999)} min={field.min ?? 0} max={field.max ?? 999} onChange={(n) => onChange(n, false)} />
        </span>
      );
    case "combo":
      if (field.allowCustom) return <OpenCombo field={field} id={id} value={text(value)} onChange={(v) => onChange(v, true)} />;
      if (!field.choices?.length) return <TextBox id={id} value={text(value)} aria-describedby={describedBy} invalid={invalid} onChange={(e) => onChange(e.target.value, true)} />;
      return <ComboBox id={id} options={withCurrent(field.choices, text(value))} value={text(value)} aria-describedby={describedBy} onChange={(v) => onChange(v, false)} />;
    case "toggle":
      return <span id={id}><ToggleSwitch label={label} checked={value === true} onChange={(v) => onChange(v, false)} /></span>;
    case "tags": case "chips":
      return <ChipsField id={id} label={label} value={strings(value)} onChange={(v) => onChange(v, false)} choices={field.choices} allowCustom={field.allowCustom ?? !field.choices?.length} describedBy={describedBy} invalid={invalid} />;
    case "reference": case "references": {
      const multiple = field.control === "references" || !!field.picker?.multiple;
      return <RefField id={id} label={label} target={{ ...(field.picker ?? { types: [] }), multiple }} value={refsOf(value)} describedBy={describedBy} invalid={invalid}
        onChange={(refs) => onChange(multiple ? refs : refs[0] ?? null, false)} />;
    }
    case "statistic":
      return <StatisticField id={id} label={label} value={text(value)} onChange={(v) => onChange(v, false)} describedBy={describedBy} invalid={invalid} />;
    case "condition":
      return <div id={id}><ConditionBuilder label={label} value={asExprValue<HomebrewCondition>(value)} onChange={(v) => onChange(v, false)} problems={builderProblems} /></div>;
    case "tagFilter":
      return <div id={id}><TagFilterBuilder label={label} value={asExprValue<HomebrewTagFilter>(value)} onChange={(v) => onChange(v, false)} problems={builderProblems} /></div>;
    case "equipped":
      return <div id={id}><EquippedBuilder label={label} value={asExprValue<HomebrewEquipped>(value)} onChange={(v) => onChange(v, false)} problems={builderProblems} /></div>;
    case "formula":
      return <div id={id}><FormulaBuilder label={label} value={asExprValue<HomebrewFormulaNode>(value)} onChange={(v) => onChange(v, false)} dialect="formula" problems={builderProblems} required={field.required} /></div>;
    case "richText":
      return <RichTextEditor id={id} label={label} value={value as DocJson | null} onChange={(doc, typing) => onChange(doc, typing)} describedBy={describedBy} invalid={invalid}
        extras={kindId === "spell" ? "spell" : kindId === "class" ? "class" : null} edition={env.edition} />;
    case "sheet":
      return <SheetField id={id} value={(value as HomebrewSheetValue | null) ?? null} onChange={(v, typing) => onChange(v, typing)} />;
    case "setters": {
      // the generic form offers the setter names elements of its type use (WB-19)
      const type = typeof env.form?.fields.type === "string" ? env.form.fields.type.trim() : "";
      const nameTypes = field.picker && type ? [...field.picker.types, type] : [];
      return <SettersField id={id} value={Array.isArray(value) ? value as HomebrewSetterRow[] : []} nameTypes={nameTypes} onChange={(v, typing) => onChange(v, typing)} />;
    }
    case "spellLists":
      return <SpellListsField id={id} label={label} value={strings(value)} onChange={(v) => onChange(v, false)} describedBy={describedBy} invalid={invalid} />;
    case "mapsTo":
      return <MapsToField id={id} value={(value as HomebrewMapsToValue | null) ?? null} onChange={(v) => onChange(v, false)} />;
    case "contents":
      return <ContentsField id={id} label={label} value={contentsOf(value)} onChange={(rows) => onChange(rows, false)} />;
    case "packageOptions":
      return <PackageOptionsField id={id} label={label} value={optionsOf(value)} onChange={(options, typing) => onChange(options, typing)} />;
    case "blocks":
      return <div id={id}><BlockList label={label} blocks={Array.isArray(value) ? value as HomebrewBlock[] : []} onChange={(b) => onChange(b, false)} problems={problems} answer={env.answer}
        repeat={kindId !== undefined && REPEAT_KINDS.includes(kindId)} /></div>;
    case "numbers":
      return <span id={id}><NumbersControl label={label} value={Array.isArray(value) ? value as number[] : []} min={field.min ?? 1} max={field.max ?? 20} onChange={(v) => onChange(v ?? [], false)} /></span>;
    case "featGrant":
      return <FeatGrantField id={id} label={label} value={value} target={field.picker ?? { types: ["Feat"] }} describedBy={describedBy} invalid={invalid} onChange={(v) => onChange(v, false)} />;
    case "choices":
      return <ChoicesField id={id} label={label} value={value} onChange={(v) => onChange(v, false)} />;
    case "writtenList":
      return <WrittenListField id={id} label={label} value={value} onChange={(v, typing) => onChange(v, typing)} />;
    case "skillChoice":
      return <SkillChoiceField id={id} label={label} value={value} onChange={(v) => onChange(v, false)} />;
    case "levels":
      if (childEditing) return <div id={id}><LevelsGrid editing={childEditing} featureType={field.picker?.types[0]} /></div>;
      break;
    case "package":
      if (childEditing) return <div id={id}><PackageField editing={childEditing} field={field.key} label={label} /></div>;
      break;
    case "children":
      if (childEditing) return <div id={id}><ChildrenField editing={childEditing} field={field.key} label={label} type={field.picker?.types[0]} /></div>;
      break;
    default:
      break;
  }
  // The class table is a part of the LEVELS grid; child lists need the form (a child's own dialog has none).
  return <p id={id} className="hb-pending" tabIndex={-1}>This part is edited with the {field.control === "classTable" ? "LEVELS grid" : field.control === "levelCounts" || field.control === "slotTable" ? "table on the SPELLCASTING tab" : "element it belongs to"}.</p>;
}
