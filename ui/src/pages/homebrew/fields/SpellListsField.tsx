import { type KeyboardEvent, useState } from "react";
import type { Api } from "../../../api/contract";
import { CheckBox, Icon, TextBox } from "../../../components";
import { useApi } from "../../../state/ApiContext";
import { useAsync } from "../../../state/useAsync";
import { namesInScope, useCreatorEnv } from "../env";
import { cleanChip } from "../pickers/ChipsField";

/** The picker type whose rows are the spell lists loaded (and drafted) classes declare (`homebrew_refs::SPELL_LIST_TYPE`). */
export const SPELL_LIST_TYPE = "Spell List";
const LIST_PAGE = 500;

/** The declared spell lists; null when the backend can't say (the field then offers the loaded class names). */
async function loadLists(api: Api, draftId: string | null): Promise<string[] | null> {
  try {
    const page = await api.homebrewReferenceOptions({ target: { types: [SPELL_LIST_TYPE] }, query: "", filters: {}, page: { offset: 0, limit: LIST_PAGE }, ...(draftId ? { draftId } : {}) });
    return page.items.map((r) => r.name);
  } catch {
    return null;
  }
}

export interface SpellListsFieldProps {
  id: string;
  label: string;
  /** The spell's support tags (its lists), in stored order. */
  value: readonly string[];
  onChange: (value: string[]) => void;
  describedBy?: string;
  invalid?: boolean;
}

const same = (a: string, b: string): boolean => a.toLowerCase() === b.toLowerCase();

/**
 * A spell's lists (spec 4.1 "Spell lists"): one check box per spell list the loaded classes declare (own homebrew
 * classes included: the `SPELL_LIST_TYPE` picker rows), then the lists the spell is on that no loaded class declares,
 * then a box for a new list name (a homebrew class still to come, or a list a feature picks spells from). Checking adds the list after the others,
 * unchecking removes it; the order of the rest is kept (it is the stored order of the spell's tags).
 */
export function SpellListsField({ id, label, value, onChange, describedBy, invalid }: SpellListsFieldProps) {
  const env = useCreatorEnv();
  const api = useApi();
  const declared = useAsync(() => loadLists(api, env.draftId), [api, env.draftId]);
  const [draft, setDraft] = useState("");
  const loaded = declared.data ?? (declared.loading ? [] : namesInScope(env, "spellList"));
  const others = value.filter((v) => !loaded.some((l) => same(l, v)));
  const shown = [...loaded, ...others];
  const isOn = (name: string) => value.some((v) => same(v, name));
  const toggle = (name: string) => onChange(isOn(name) ? value.filter((v) => !same(v, name)) : [...value, name]);
  const add = () => {
    const name = cleanChip(draft);
    setDraft("");
    if (name && !isOn(name)) onChange([...value, name]);
  };
  const onKey = (e: KeyboardEvent<HTMLSpanElement>) => {
    if (e.key === "Enter" && draft.trim()) { e.preventDefault(); add(); }
  };
  return (
    <div className={`hb-lists ${invalid ? "hb-invalid" : ""}`} id={id} role="group" aria-label={label} aria-describedby={describedBy}>
      {shown.length === 0
        ? <p className="hb-lists-none">No loaded class has a spell list. Add a list name below.</p>
        : (
          <ul className="hb-lists-grid">
            {shown.map((name) => (
              <li key={name}>
                <label className="hb-lists-item">
                  <CheckBox aria-label={name} checked={isOn(name)} onChange={() => toggle(name)} />
                  <span>{name}</span>
                </label>
              </li>
            ))}
          </ul>
        )}
      <span className="hb-chip-add" onKeyDown={onKey}>
        <TextBox value={draft} aria-label="New spell list" placeholder="Another list..." onChange={(e) => setDraft(e.target.value)} />
        <button type="button" className="hb-pick-btn" aria-label="Add spell list" disabled={!cleanChip(draft)} onClick={add}><Icon name="plus" />Add</button>
      </span>
    </div>
  );
}
