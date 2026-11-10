import type { HomebrewAbility, HomebrewChoice } from "../../../api/contract";
import { CheckBox, ComboBox, EditableComboBox, Icon, NumericUpDown, TextBox } from "../../../components";
import { ABILITIES, ABILITY_CHOICES, ABILITY_WORDS } from "../builders/words";
import { namesInScope, useCreatorEnv } from "../env";
import { StatisticField } from "../pickers/StatisticPicker";

/** Shared props of the small value controls (block field shapes of hc-wb05 `fit.rs`). */
export interface ValueProps<V> {
  label: string;
  value: V | undefined;
  onChange: (v: V | undefined) => void;
  min?: number;
  max?: number;
}

const isWholeNumber = (s: string): boolean => /^-?\d+$/.test(s.trim());
const NUMBER = "#number";
const OTHER = "#other";

/** A number, or one of the field's words (`any`, `all`), or (with `allowCustom`) any text. */
export function NumberOrWords({ label, value, onChange, min = 0, max = 999, choices, allowCustom }: ValueProps<number | string> & { choices: readonly HomebrewChoice[]; allowCustom?: boolean }) {
  const isWord = typeof value === "string" && choices.some((c) => c.value === value);
  const mode = value === undefined || typeof value === "number" ? NUMBER : isWord ? String(value) : OTHER;
  const options = [{ value: NUMBER, label: "A number" }, ...choices, ...(allowCustom ? [{ value: OTHER, label: "Other" }] : [])];
  return (
    <span className="hb-row-controls">
      <ComboBox aria-label={`${label}: kind`} value={mode} options={options}
        onChange={(m) => onChange(m === NUMBER ? min : m === OTHER ? "" : m)} />
      {mode === NUMBER && <NumericUpDown label={label} value={typeof value === "number" ? value : min} min={min} max={max} onChange={onChange} compact />}
      {mode === OTHER && <TextBox aria-label={label} value={String(value ?? "")} onChange={(e) => onChange(e.target.value)} />}
    </span>
  );
}

/** `statValue`: a number written as text (`"2"`), a statistic, or minus a statistic (`"-strength:modifier"`). */
export function StatValueControl({ label, value, onChange }: ValueProps<string>) {
  const v = value ?? "";
  const mode = v === "" || isWholeNumber(v) ? "number" : v.startsWith("-") ? "minus" : "stat";
  const stat = mode === "minus" ? v.slice(1) : mode === "stat" ? v : "";
  return (
    <span className="hb-row-controls">
      <ComboBox aria-label={`${label}: kind`} value={mode} onChange={(m) => onChange(m === "number" ? "1" : "")}
        options={[{ value: "number", label: "A number" }, { value: "stat", label: "Another statistic" }, { value: "minus", label: "Minus another statistic" }]} />
      {mode === "number"
        ? <NumericUpDown label={label} value={v === "" ? 0 : Number(v)} min={-999} max={999} onChange={(n) => onChange(String(n))} compact />
        : <StatisticField label={label} value={stat} clearable={false} onChange={(s) => onChange(mode === "minus" ? `-${s}` : s)} />}
    </span>
  );
}

type NumberOrStat = number | { type: "stat"; stat: string };

/** `numberOrStatistic`: a number or `{type: "stat", stat}`. */
export function NumberOrStatControl({ label, value, onChange, min = 0, max = 99 }: ValueProps<NumberOrStat>) {
  const isStat = typeof value === "object" && value !== null;
  return (
    <span className="hb-row-controls">
      <ComboBox aria-label={`${label}: kind`} value={isStat ? "stat" : "number"} onChange={(m) => onChange(m === "stat" ? { type: "stat", stat: "" } : min)}
        options={[{ value: "number", label: "A number" }, { value: "stat", label: "A statistic" }]} />
      {isStat
        ? <StatisticField label={label} value={value.stat} clearable={false} onChange={(stat) => onChange({ type: "stat", stat })} />
        : <NumericUpDown label={label} value={typeof value === "number" ? value : min} min={min} max={max} onChange={onChange} compact />}
    </span>
  );
}

/** A column of the class table (`classCounter`): the class counters of the statistic catalog, by name. */
export function ClassCounterControl({ label, value, onChange }: ValueProps<string>) {
  const env = useCreatorEnv();
  const counters = env.stats.find((g) => g.id === "classCounters")?.stats ?? [];
  const options = [{ value: "", label: "Choose a column" }, ...(value && !counters.some((c) => c.name === value) ? [{ value, label: value }] : []), ...counters.map((c) => ({ value: c.name, label: c.label }))];
  return <ComboBox aria-label={label} value={value ?? ""} options={options} onChange={(v) => onChange(v || undefined)} />;
}

type Uses = number | string | { type: "table"; key: string };

/** `uses`: a number, the proficiency bonus, an ability modifier, or a class table column. */
export function UsesControl({ label, value, onChange }: ValueProps<Uses>) {
  const mode = typeof value === "object" && value !== null ? "table" : value === "proficiency" ? "proficiency"
    : typeof value === "string" && (ABILITIES as readonly string[]).includes(value) ? "ability" : "number";
  return (
    <span className="hb-row-controls">
      <ComboBox aria-label={`${label}: kind`} value={mode}
        onChange={(m) => onChange(m === "number" ? 1 : m === "proficiency" ? "proficiency" : m === "ability" ? "wisdom" : { type: "table", key: "" })}
        options={[{ value: "number", label: "A number" }, { value: "proficiency", label: "Proficiency bonus" }, { value: "ability", label: "An ability modifier" }, { value: "table", label: "From the class table" }]} />
      {mode === "number" && <NumericUpDown label={label} value={typeof value === "number" ? value : 1} min={1} max={99} onChange={onChange} compact />}
      {mode === "ability" && <ComboBox aria-label={`${label}: ability`} value={String(value)} options={ABILITY_CHOICES} onChange={onChange} />}
      {mode === "table" && <ClassCounterControl label={`${label}: column`} value={typeof value === "object" ? value.key : ""} onChange={(key) => onChange({ type: "table", key: key ?? "" })} />}
    </span>
  );
}

type AbilitySource = string | { param: string };

/** `abilitySource`: an ability, or the answer to a named choice. */
export function AbilitySourceControl({ label, value, onChange }: ValueProps<AbilitySource>) {
  const env = useCreatorEnv();
  const isParam = typeof value === "object" && value !== null;
  return (
    <span className="hb-row-controls">
      <ComboBox aria-label={`${label}: kind`} value={isParam ? "param" : "ability"} onChange={(m) => onChange(m === "param" ? { param: "" } : "wisdom")}
        options={[{ value: "ability", label: "An ability" }, { value: "param", label: "The answer to a named choice" }]} />
      {isParam
        ? <EditableComboBox label={`${label}: named choice`} aria-label={`${label}: named choice`} value={value.param} options={namesInScope(env, "choice")} onChange={(param) => onChange({ param })} />
        : <ComboBox aria-label={`${label}: ability`} value={typeof value === "string" ? value : ""} options={[{ value: "", label: "Choose" }, ...ABILITY_CHOICES]} onChange={(v) => onChange(v || undefined)} />}
    </span>
  );
}

/** `abilityChoice`: any N abilities, or a list of abilities. */
export function AbilityChoiceControl({ label, value, onChange }: ValueProps<number | HomebrewAbility[]>) {
  const list = Array.isArray(value);
  return (
    <span className="hb-row-controls hb-wrap">
      <ComboBox aria-label={`${label}: kind`} value={list ? "list" : "any"} onChange={(m) => onChange(m === "list" ? [] : 1)}
        options={[{ value: "any", label: "Any abilities, how many" }, { value: "list", label: "These abilities" }]} />
      {list ? ABILITIES.map((a) => (
        <label key={a} className="hb-inline-check">
          <CheckBox checked={value.includes(a)} onChange={(e) => onChange(e.target.checked ? [...value, a] : value.filter((x) => x !== a))} />{ABILITY_WORDS[a]}
        </label>
      )) : <NumericUpDown label={label} value={typeof value === "number" ? value : 1} min={1} max={6} onChange={onChange} compact />}
    </span>
  );
}

/** `numbers`: whole numbers from a range as toggle buttons (spell levels 0-9, the Magic filter's toggles). */
export function NumbersControl({ label, value, onChange, min = 0, max = 9 }: ValueProps<number[]>) {
  const list = value ?? [];
  const all = Array.from({ length: max - min + 1 }, (_, i) => min + i);
  return (
    <span className="sp-levels hb-numbers" role="group" aria-label={label}>
      {all.map((n) => (
        <button key={n} type="button" className="sp-tog" aria-pressed={list.includes(n)}
          onClick={() => onChange(list.includes(n) ? list.filter((x) => x !== n) : [...list, n].sort((a, b) => a - b))}>{n}</button>
      ))}
    </span>
  );
}

/** `splits`: ways to spread points (`[[2, 1], [1, 1, 1]]`), each a row of numbers. */
export function SplitsControl({ label, value, onChange }: ValueProps<number[][]>) {
  const list = value ?? [];
  const setRow = (i: number, row: number[]) => onChange(list.map((r, j) => (j === i ? row : r)));
  return (
    <div className="hb-splits" role="group" aria-label={label}>
      {list.map((row, i) => (
        <div key={i} className="hb-split">
          {row.map((n, j) => (
            <NumericUpDown key={j} label={`${label} ${i + 1}, part ${j + 1}`} value={n} min={1} max={10} compact onChange={(x) => setRow(i, row.map((y, k) => (k === j ? x : y)))} />
          ))}
          <button type="button" className="hb-chip-x" aria-label={`Add a part to way ${i + 1}`} onClick={() => setRow(i, [...row, 1])}><Icon name="plus" /></button>
          {row.length > 1 && <button type="button" className="hb-chip-x" aria-label={`Remove a part from way ${i + 1}`} onClick={() => setRow(i, row.slice(0, -1))}><Icon name="minus" /></button>}
          <button type="button" className="hb-chip-x" aria-label={`Remove way ${i + 1}`} onClick={() => onChange(list.filter((_, j) => j !== i))}><Icon name="x" /></button>
        </div>
      ))}
      <button type="button" className="hb-pick-btn" onClick={() => onChange([...list, [2, 1]])}><Icon name="plus" />Add a way</button>
    </div>
  );
}
