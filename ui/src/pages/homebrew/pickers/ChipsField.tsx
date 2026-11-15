import { type KeyboardEvent, useState } from "react";
import type { HomebrewChoice } from "../../../api/contract";
import { EditableComboBox, Icon } from "../../../components";
import { useWords } from "../fields/useWords";

/** Characters a tag or chip may not hold (IM15: they would break the tag grammar or the stored list). */
const FORBIDDEN = /[,|&!()[\]{}$]/g;

const NONE: readonly string[] = [];

/** A typed chip, trimmed, single spaced and without the characters lists can't hold. */
export const cleanChip = (text: string): string => text.replace(FORBIDDEN, " ").replace(/\s+/g, " ").trim();

export interface ChipsFieldProps {
  id?: string;
  label: string;
  value: readonly string[];
  onChange: (value: string[]) => void;
  /** A closed set: each choice is a toggle chip. With `allowCustom`, choices are suggestions and any text may be added. */
  choices?: readonly HomebrewChoice[];
  allowCustom?: boolean;
  /** Suggestions for typed chips (names in scope). */
  suggestions?: readonly string[];
  /** Picker types whose words are further suggestions (`["Tag", "Feat"]`: the tags loaded feats carry). */
  suggestFrom?: readonly string[];
  describedBy?: string;
  invalid?: boolean;
}

/**
 * Chips (spec 3.2 tags, keywords; block chips): a closed set shows one toggle chip per choice (state shown by the check
 * and `aria-pressed`, never colour alone, spec 17); an open list shows each chip with its X and an add box with
 * suggestions ("New tag" allowed).
 */
export function ChipsField({ id, label, value, onChange, choices, allowCustom, suggestions = [], suggestFrom = NONE, describedBy, invalid }: ChipsFieldProps) {
  const [draft, setDraft] = useState("");
  const loaded = useWords(suggestFrom);
  const closed = !!choices?.length && !allowCustom;
  const labelOf = (v: string) => choices?.find((c) => c.value === v)?.label ?? v;
  if (closed) {
    return (
      <div className={`hb-chips ${invalid ? "hb-invalid" : ""}`} id={id} role="group" aria-label={label} aria-describedby={describedBy}>
        {choices!.map((c) => {
          const on = value.includes(c.value);
          return (
            <button key={c.value} type="button" className="hb-chip" aria-pressed={on}
              onClick={() => onChange(on ? value.filter((v) => v !== c.value) : [...value, c.value])}>
              {on && <Icon name="check" />}{c.label}
            </button>
          );
        })}
      </div>
    );
  }
  const offered = [...new Set([...(choices ?? []).map((c) => c.value), ...suggestions, ...loaded])].filter((s) => !value.includes(s));
  const add = (text: string) => {
    const chip = cleanChip(choices?.find((c) => c.label === text)?.value ?? text);
    setDraft("");
    if (chip && !value.some((v) => v.toLowerCase() === chip.toLowerCase())) onChange([...value, chip]);
  };
  const onKey = (e: KeyboardEvent<HTMLSpanElement>) => {
    if (e.key === "Enter" && draft.trim()) { e.preventDefault(); add(draft); }
  };
  return (
    <div className={`hb-chips-open ${invalid ? "hb-invalid" : ""}`} id={id} aria-describedby={describedBy}>
      {value.length > 0 && (
        <ul className="hb-ref-list" aria-label={label}>
          {value.map((v) => (
            <li key={v} className="hb-chip-item">
              <span className="hb-ref-name">{labelOf(v)}</span>
              <button type="button" className="hb-chip-x" aria-label={`Remove ${labelOf(v)}`} onClick={() => onChange(value.filter((x) => x !== v))}><Icon name="x" /></button>
            </li>
          ))}
        </ul>
      )}
      <span className="hb-chip-add" onKeyDown={onKey}>
        <EditableComboBox value={draft} options={offered.map(labelOf)} onChange={setDraft} label={label} aria-label={`New ${label.toLowerCase()}`} placeholder="Add..." />
        <button type="button" className="hb-pick-btn" aria-label={`Add to ${label}`} disabled={!cleanChip(draft)} onClick={() => add(draft)}><Icon name="plus" />Add</button>
      </span>
    </div>
  );
}
