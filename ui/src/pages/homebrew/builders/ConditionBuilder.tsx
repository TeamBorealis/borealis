import type { ReactNode } from "react";
import type { HomebrewAbility, HomebrewBuilderProblem, HomebrewCondition, HomebrewConditionRow, HomebrewExprNode, HomebrewExprValue } from "../../../api/contract";
import { ComboBox, EditableComboBox, NumericUpDown, TextBox } from "../../../components";
import { classChoices, useCreatorEnv } from "../env";
import { IdRefField } from "../pickers/RefField";
import { StatisticField } from "../pickers/StatisticPicker";
import { ExprTree, KeptChip, type RowEditor } from "./ExprTree";
import { ABILITY_CHOICES, emptyGroup, withModel } from "./words";

type Row = HomebrewConditionRow;
type RowKind = Exclude<Row["row"], "kept">;
type Group = Extract<HomebrewExprNode<Row>, { node: "group" }>;

/** Row kinds in plain words (spec 6.3 plus hc-wb02 correction 4). */
const ROW_WORDS: readonly { value: RowKind; label: string }[] = [
  { value: "has", label: "Has" },
  { value: "lacks", label: "Doesn't have" },
  { value: "ability", label: "Ability score at least" },
  { value: "characterLevel", label: "Character level at least" },
  { value: "level", label: "Level in this class at least" },
  { value: "classLevel", label: "Class level at least" },
  { value: "feature", label: "Has a feature named" },
  { value: "spellcasting", label: "Can cast spells of level" },
  { value: "type", label: "Has an element of type" },
  { value: "statistic", label: "Statistic at least" },
];

/** Element types a "Has an element of type" row offers (the engine compares them case-insensitively). */
const TYPE_SUGGESTIONS = ["Class", "Archetype", "Race", "Species", "Background", "Feat", "Spell", "Language", "Proficiency", "Option"];

export function newConditionRow(kind: RowKind): Row {
  switch (kind) {
    case "has": case "lacks": return { row: kind, id: "" };
    case "ability": return { row: kind, ability: "strength", min: 13 };
    case "characterLevel": case "level": return { row: kind, min: 4 };
    case "classLevel": return { row: kind, class: "", min: 2 };
    case "feature": return { row: kind, name: "" };
    case "spellcasting": return { row: kind, level: 1 };
    case "type": return { row: kind, value: "" };
    case "statistic": return { row: kind, stat: "", min: 1 };
  }
}

const num = (label: string, value: number, min: number, max: number, set: (n: number) => void): ReactNode => (
  <NumericUpDown label={label} value={value} min={min} max={max} onChange={set} compact />
);

/** One condition row's controls (the kind combo, then its parts). */
function ConditionRowControls({ row, onRow, label }: { row: Row; onRow: (r: Row) => void; label: string }) {
  const env = useCreatorEnv();
  if (row.row === "kept") return null;
  const kind = (
    <ComboBox aria-label={`${label}: test`} value={row.row} options={ROW_WORDS} onChange={(v) => onRow(newConditionRow(v as RowKind))} />
  );
  switch (row.row) {
    case "has": case "lacks":
      return <>{kind}<IdRefField label={`${label}: element`} target={{ types: [] }} ids={row.id ? [row.id] : []} multiple={false} onIds={(ids) => onRow({ ...row, id: ids[0] ?? "" })} /></>;
    case "ability":
      return <>{kind}<ComboBox aria-label={`${label}: ability`} value={row.ability} options={ABILITY_CHOICES} onChange={(v) => onRow({ ...row, ability: v as HomebrewAbility })} />{num(`${label}: score`, row.min, 1, 30, (min) => onRow({ ...row, min }))}</>;
    case "characterLevel": case "level":
      return <>{kind}{num(`${label}: level`, row.min, 1, 20, (min) => onRow({ ...row, min }))}</>;
    case "classLevel": {
      const classes = classChoices(env);
      const options = row.class && !classes.some((c) => c.value === row.class) ? [{ value: row.class, label: row.class }, ...classes] : classes;
      return <>{kind}<ComboBox aria-label={`${label}: class`} value={row.class} options={[{ value: "", label: "Choose a class" }, ...options]} onChange={(v) => onRow({ ...row, class: v })} />{num(`${label}: level`, row.min, 1, 20, (min) => onRow({ ...row, min }))}</>;
    }
    case "feature":
      return <>{kind}<TextBox aria-label={`${label}: feature name`} value={row.name} onChange={(e) => onRow({ ...row, name: e.target.value })} /></>;
    case "spellcasting":
      return <>{kind}{num(`${label}: spell level`, row.level, 1, 9, (level) => onRow({ ...row, level }))}</>;
    case "type":
      return <>{kind}<EditableComboBox label={`${label}: element type`} aria-label={`${label}: element type`} value={row.value} options={TYPE_SUGGESTIONS} onChange={(value) => onRow({ ...row, value })} /></>;
    case "statistic":
      return <>{kind}<StatisticField label={`${label}: statistic`} value={row.stat} clearable={false} onChange={(stat) => onRow({ ...row, stat })} />{num(`${label}: at least`, row.min, -999, 999, (min) => onRow({ ...row, min }))}</>;
  }
}

const EDITOR: RowEditor<Row> = {
  render: (row, onRow, label) => <ConditionRowControls row={row} onRow={onRow} label={label} />,
  make: () => newConditionRow("has"),
  kept: (row) => (row.row === "kept" ? row.text : null),
  addLabel: "Add condition",
};

export interface ConditionBuilderProps {
  label: string;
  value: HomebrewExprValue<HomebrewCondition> | null;
  onChange: (value: HomebrewExprValue<HomebrewCondition>) => void;
  /** Problems relative to the model (`items.1.min`). */
  problems: readonly HomebrewBuilderProblem[];
}

/**
 * The condition builder (spec 6.3, `requirements`): rows of tests grouped by All of / Any of / None of. Never shows the
 * text form. Text Borealis can't read whole stays a read-only "kept as is" chip with Remove (6.5).
 */
export function ConditionBuilder({ label, value, onChange, problems }: ConditionBuilderProps) {
  if (value?.value.shape === "kept") {
    return <KeptChip text={value.value.text} reason={value.value.reason} what="condition" onRemove={() => onChange(withModel(value, emptyGroup<Row>()))} />;
  }
  const root = (value?.value.shape === "model" && value.value.model.node === "group" ? value.value.model : emptyGroup<Row>()) as Group;
  return <ExprTree label={label} node={root} onChange={(g) => onChange(withModel(value, g))} editor={EDITOR} problems={problems} path="" />;
}
