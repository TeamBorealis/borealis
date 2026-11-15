import type { HomebrewChoiceRow, HomebrewExprValue, HomebrewFeatGrant, HomebrewPickerTarget, HomebrewTagFilter, HomebrewWrittenList } from "../../../api/contract";
import { ComboBox, Icon, NumericUpDown, TextBox } from "../../../components";
import { TagFilterBuilder } from "../builders/TagFilterBuilder";
import { asExprValue } from "../builders/words";
import { RefField, refsOf } from "../pickers/RefField";
import { useWords } from "./useWords";

// The background form's own controls (WB-14, spec 4.3): the origin feat with the feat's choices the background makes,
// language and tool choices, and the written lists of a 2014 background's characteristics.

/** The pseudo picker type of a feat's own choices (`borealis_authoring::form::FEAT_PARAMETER_PICKER`). */
export const FEAT_PARAMETER_PICKER = "Feat Parameter";
/** The most a choice or a characteristic offers (`codec::background` MAX_CHOICE / MAX_PICKS). */
const MAX_PICKS = 10;

/** The engine's key of a feat's choice: "Spell List" is `spell_list` (`param_key`). */
export const paramKey = (name: string): string => name.trim().toLowerCase().replace(/[ -]/g, "_");

/** A `featGrant` value (a new background starts with no feat). */
export function featGrantOf(value: unknown): HomebrewFeatGrant {
  const v = (typeof value === "object" && value !== null ? value : {}) as Partial<HomebrewFeatGrant>;
  const params = typeof v.params === "object" && v.params !== null ? v.params : {};
  return { feat: refsOf(v.feat)[0] ?? null, params };
}

/** One of the feat's own choices: "Spell List: (the character's choice) / Cleric / Druid / Wizard". */
function FeatParam({ featId, name, value, onChange }: { featId: string; name: string; value: string; onChange: (v: string) => void }) {
  const options = useWords([FEAT_PARAMETER_PICKER, featId, name]);
  const choices = [{ value: "", label: "The character chooses" }, ...options.map((o) => ({ value: o, label: o }))];
  if (value && !options.includes(value)) choices.push({ value, label: value });
  return (
    <span className="hb-row-controls hb-feat-param">
      <span className="hb-inline-label">{name}</span>
      <ComboBox aria-label={`${name} of the feat`} options={choices} value={value} onChange={onChange} />
    </span>
  );
}

/**
 * The origin feat (spec 4.3): the feat picker, then one combo per choice of the feat's own (its `select_param` names,
 * Magic Initiate's Spell List) to make it for the character, or leave it theirs.
 */
export function FeatGrantField({ id, label, value, target, onChange, describedBy, invalid }: {
  id: string; label: string; value: unknown; target: HomebrewPickerTarget; onChange: (v: HomebrewFeatGrant) => void; describedBy?: string; invalid?: boolean;
}) {
  const grant = featGrantOf(value);
  const featId = grant.feat?.id ?? "";
  const names = useWords(featId ? [FEAT_PARAMETER_PICKER, featId] : []);
  const setParam = (name: string, v: string) => {
    const params = { ...grant.params };
    delete params[paramKey(name)];
    if (v) params[paramKey(name)] = v;
    onChange({ ...grant, params });
  };
  // a choice the background makes that the feat (as loaded) doesn't name is still shown, to be cleared
  const known = new Set(names.map(paramKey));
  const unknown = Object.keys(grant.params).filter((k) => !known.has(k));
  return (
    <div className="hb-feat-grant" role="group" aria-label={label}>
      <RefField id={id} label={label} target={{ ...target, multiple: false }} value={grant.feat ? [grant.feat] : []} describedBy={describedBy} invalid={invalid}
        onChange={(refs) => onChange({ feat: refs[0] ?? null, params: refs[0]?.id === featId ? grant.params : {} })} />
      {names.map((n) => <FeatParam key={n} featId={featId} name={n} value={grant.params[paramKey(n)] ?? ""} onChange={(v) => setParam(n, v)} />)}
      {unknown.map((k) => (
        <span key={k} className="hb-row-controls hb-feat-param">
          <span className="hb-inline-label">{k}: {grant.params[k]}</span>
          <button type="button" className="hb-chip-x" aria-label={`Clear ${k}`} onClick={() => setParam(k, "")}><Icon name="x" /></button>
        </span>
      ))}
    </div>
  );
}

/** A `choices` value. */
export function choiceRowsOf(value: unknown): HomebrewChoiceRow[] {
  return Array.isArray(value) ? value.filter((r): r is HomebrewChoiceRow => typeof r === "object" && r !== null && "kind" in r) : [];
}

const CHOICE_KINDS = [{ value: "language", label: "Languages" }, { value: "proficiency", label: "Tools or other proficiencies" }];
const TARGET_TYPES: Record<HomebrewChoiceRow["kind"], string[]> = { language: ["Language"], proficiency: ["Proficiency"] };

/**
 * Language and tool choices (spec 4.3): per row, how many the character chooses, of what (languages, tools) and from
 * which tags (the tag filter builder, `Standard||Exotic`). Add a choice adds "1 language, any Standard or Exotic".
 */
export function ChoicesField({ id, label, value, onChange }: { id: string; label: string; value: unknown; onChange: (v: HomebrewChoiceRow[]) => void }) {
  const rows = choiceRowsOf(value);
  const set = (i: number, row: HomebrewChoiceRow) => onChange(rows.map((r, j) => (j === i ? row : r)));
  const add = () => onChange([...rows, { kind: "language", count: 1, from: null, name: "" }]);
  return (
    <div className="hb-choices" id={id} role="group" aria-label={label}>
      {rows.length === 0 && <p className="hb-field-help">None yet.</p>}
      {rows.map((r, i) => (
        <div key={i} className="hb-choice-row">
          <span className="hb-row-controls">
            <span className="hb-inline-label">Choose</span>
            <NumericUpDown label={`Choice ${i + 1}: how many`} value={r.count} min={1} max={MAX_PICKS} compact onChange={(n) => set(i, { ...r, count: n })} />
            <ComboBox aria-label={`Choice ${i + 1}: what`} options={CHOICE_KINDS} value={r.kind} onChange={(k) => set(i, { ...r, kind: k as HomebrewChoiceRow["kind"] })} />
            <button type="button" className="hb-chip-x" aria-label={`Remove choice ${i + 1}`} onClick={() => onChange(rows.filter((_, j) => j !== i))}><Icon name="x" /></button>
          </span>
          <TagFilterBuilder label={`Choice ${i + 1}: from`} value={asExprValue<HomebrewTagFilter>(r.from)} targetTypes={TARGET_TYPES[r.kind] ?? []} problems={[]}
            onChange={(from: HomebrewExprValue<HomebrewTagFilter>) => set(i, { ...r, from })} />
        </div>
      ))}
      <button type="button" className="hb-pick-btn" onClick={add}><Icon name="plus" />Add a choice</button>
    </div>
  );
}

/** A `writtenList` value. */
export function writtenListOf(value: unknown): HomebrewWrittenList {
  const v = (typeof value === "object" && value !== null ? value : {}) as Partial<HomebrewWrittenList>;
  return { count: typeof v.count === "number" ? v.count : 1, items: Array.isArray(v.items) ? v.items.filter((t): t is string => typeof t === "string") : [] };
}

/**
 * A characteristic the character picks from (spec 4.3: Personality Trait, Ideal, Bond, Flaw): how many they pick, and
 * the entries, numbered 1, 2, 3... in order (the d6/d8 of the table). Typing in a row is one undo step.
 */
export function WrittenListField({ id, label, value, onChange }: { id: string; label: string; value: unknown; onChange: (v: HomebrewWrittenList, typing: boolean) => void }) {
  const list = writtenListOf(value);
  const setItem = (i: number, text: string) => onChange({ ...list, items: list.items.map((t, j) => (j === i ? text : t)) }, true);
  return (
    <div className="hb-written-list" id={id} role="group" aria-label={label}>
      <span className="hb-row-controls">
        <span className="hb-inline-label">The character picks</span>
        <NumericUpDown label={`${label}: how many`} value={list.count} min={1} max={MAX_PICKS} compact onChange={(n) => onChange({ ...list, count: n }, false)} />
      </span>
      <ol className="hb-list-items">
        {list.items.map((t, i) => (
          <li key={i} className="hb-list-item">
            <span className="hb-list-number" aria-hidden="true">{i + 1}</span>
            <TextBox aria-label={`${label} ${i + 1}`} value={t} onChange={(e) => setItem(i, e.target.value)} />
            <button type="button" className="hb-chip-x" aria-label={`Remove ${label.toLowerCase()} ${i + 1}`} onClick={() => onChange({ ...list, items: list.items.filter((_, j) => j !== i) }, false)}><Icon name="x" /></button>
          </li>
        ))}
      </ol>
      <button type="button" className="hb-pick-btn" onClick={() => onChange({ ...list, items: [...list.items, ""] }, false)}><Icon name="plus" />Add an entry</button>
    </div>
  );
}
