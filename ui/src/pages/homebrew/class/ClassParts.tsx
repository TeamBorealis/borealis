import { useState } from "react";
import type { HomebrewPickerTarget, HomebrewRef, HomebrewSkillChoice } from "../../../api/contract";
import { Icon, NumericUpDown, TextBox } from "../../../components";
import { setStatus } from "../../../state/status";
import { optionsOf, PackageOptionsField } from "../fields/ItemParts";
import { RefField, rememberRefs } from "../pickers/RefField";
import { ReferencePicker } from "../pickers/ReferencePicker";
import { ChildDialog, type ChildEditing, childKind, removeChecked } from "./ChildDialog";
import { childOf, entriesOf, entryName, nameOf, withChild, withField, withoutChild, withoutEntry } from "./children";

/** The skills a class offers: Proficiency elements tagged Skill. */
const SKILLS: HomebrewPickerTarget = { types: ["Proficiency"], tags: ["Skill"], multiple: true };
/** The most skills a class lets a character choose. */
const MAX_SKILLS = 18;

/** A `skillChoice` value (a new class starts with "choose 2"). */
export function skillChoiceOf(value: unknown): HomebrewSkillChoice {
  const v = (typeof value === "object" && value !== null ? value : {}) as Partial<HomebrewSkillChoice>;
  return { count: typeof v.count === "number" ? v.count : 0, skills: Array.isArray(v.skills) ? v.skills : [], ...(v.tagged ? { tagged: v.tagged } : {}) };
}

/**
 * "Skills: choose N from list" (spec 4.5): how many, and the skills to choose from (the skill picker). A 2014 class whose
 * choice is a tag filter (skills tagged with the class) shows it in words until the skills are picked instead.
 */
export function SkillChoiceField({ id, label, value, onChange }: { id: string; label: string; value: unknown; onChange: (v: HomebrewSkillChoice) => void }) {
  const choice = skillChoiceOf(value);
  return (
    <div className="hb-skill-choice" id={id} role="group" aria-label={label}>
      <span className="hb-row-controls">
        <span className="hb-inline-label">Choose</span>
        <NumericUpDown label={`${label}: how many`} value={choice.count} min={0} max={MAX_SKILLS} compact onChange={(n) => onChange({ ...choice, count: n })} />
      </span>
      {choice.tagged
        ? (
          <p className="hb-field-help">
            From the skills tagged {choice.tagged.split(",").map((t) => t.trim()).join(", ")}.{" "}
            <button type="button" className="hb-link" onClick={() => onChange({ count: choice.count, skills: [] })}>Pick the skills instead</button>
          </p>
        )
        : <RefField label={`${label}: from`} target={SKILLS} value={choice.skills} onChange={(skills: HomebrewRef[]) => onChange({ ...choice, skills })} />}
    </div>
  );
}

/**
 * The class's starting equipment (spec 4.5, 4.9): its Equipment Package, edited here (the lettered options a character
 * who starts in the class chooses from). A copied class may still offer the original's package: it is named, and Make
 * our own starts a new one.
 */
export function PackageField({ editing, field, label }: { editing: ChildEditing; field: string; label: string }) {
  const { form } = editing;
  const entry = entriesOf(form, field)[0];
  const child = entry ? childOf(form, entry) : undefined;
  const kind = childKind(editing, "Equipment Package");
  const className = nameOf(form, "the class");
  const create = () => {
    if (!kind) return;
    void editing.newChild(kind.id, (f, key) => {
      const c = f.children.find((x) => x.key === key);
      return c ? withChild(f, key, { ...c, fields: { ...c.fields, name: `Starting Equipment (${className})` } }) : f;
    }).then((k) => { if (k) setStatus("Starting equipment added."); });
  };
  if (!child) {
    return (
      <div className="hb-package-field">
        {entry && <p className="hb-field-help">Offers {entryName(form, entry)} (from another pack).</p>}
        <button type="button" className="mg-tool wide" onClick={create} disabled={!kind}><Icon name="plus" />{entry ? "Make our own" : "Add starting equipment"}</button>
      </div>
    );
  }
  const setOptions = (options: unknown, typing: boolean) =>
    editing.edit((f) => withChild(f, child.key!, { ...child, fields: { ...child.fields, options } }), typing ? "package" : null);
  const name = typeof child.fields.name === "string" ? child.fields.name : "";
  return (
    <div className="hb-package-field">
      <div className="mg-f hb-field" data-field="packageName">
        <label htmlFor="hb-package-name">Name</label>
        <TextBox id="hb-package-name" value={name} onChange={(e) => editing.edit((f) => withChild(f, child.key!, { ...child, fields: { ...child.fields, name: e.target.value } }), "packageName")} />
      </div>
      <PackageOptionsField label={label} value={optionsOf(child.fields.options)} onChange={setOptions} />
      <button type="button" className="mg-tool wide"
        onClick={() => void removeChecked(editing, child.key!, `“${name || "the starting equipment"}”`, () => editing.edit((f) => withoutChild(f, child.key!, [field])))}>
        <Icon name="minus" />No starting equipment
      </button>
    </div>
  );
}

/**
 * A parent's sub-elements (spec 3.1; a feat's Feat Features): one row each with Edit and Remove, and Add, which makes the
 * child through the backend (`homebrew_new_draft {parentId}`). The class LEVELS grid is the levelled form of this list.
 */
export function ChildrenField({ editing, field, label, type }: { editing: ChildEditing; field: string; label: string; type: string | undefined }) {
  const { form } = editing;
  const [open, setOpen] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const kind = childKind(editing, type);
  const entries = entriesOf(form, field);
  const add = async () => {
    if (!kind) return;
    const key = await editing.newChild(kind.id);
    if (key) setOpen(key);
  };
  /** Gives elements that exist (a bundled trait, another pack's feature) by reference, each once. */
  const giveExisting = (picks: HomebrewRef[]) => {
    setPicking(false);
    rememberRefs(picks);
    editing.edit((f) => {
      const now = entriesOf(f, field);
      const fresh = picks.filter((p) => !now.some((e) => e.element?.id === p.id));
      return withField(f, field, [...now, ...fresh.map((element) => ({ element }))]);
    });
  };
  // a saved child is deleted on save: characters that use it are named first (spec 10.3)
  const removeAt = (i: number) => {
    const e = entries[i];
    if (!e?.child) { editing.edit((f) => withoutEntry(f, field, i)); return; }
    const key = e.child;
    void removeChecked(editing, key, `“${entryName(form, e)}”`, () => editing.edit((f) => withoutChild(f, key, [field])));
  };
  return (
    <div className="hb-children" role="group" aria-label={label}>
      {entries.length === 0 && <p className="hb-field-help">None yet.</p>}
      <ul className="hb-ref-list">
        {entries.map((e, i) => (
          <li key={e.child ?? `${i}`} className="hb-chip-item">
            {e.child
              ? <button type="button" className="hb-feature-name" onClick={() => setOpen(e.child!)}>{entryName(form, e)}</button>
              : <span className="hb-ref-name">{entryName(form, e)}</span>}
            <button type="button" className="hb-chip-x" aria-label={`Remove ${entryName(form, e)}`} onClick={() => removeAt(i)}><Icon name="x" /></button>
          </li>
        ))}
      </ul>
      {kind && <button type="button" className="hb-pick-btn" onClick={() => void add()}><Icon name="plus" />Add a {kind.label.toLowerCase()}</button>}
      {type && <button type="button" className="hb-pick-btn" onClick={() => setPicking(true)}><Icon name="search" />Give one that exists</button>}
      {picking && type && (
        <ReferencePicker title={label} target={{ types: [type], multiple: true }} chosen={entries.flatMap((e) => (e.element ? [e.element.id] : []))}
          onChoose={giveExisting} onClose={() => setPicking(false)} />
      )}
      {open && (
        <ChildDialog editing={editing} childKey={open} onClose={() => setOpen(null)}
          onRemove={() => {
            const key = open;
            void removeChecked(editing, key, `“${entryName(form, { child: key })}”`, () => { editing.edit((f) => withoutChild(f, key, [field])); setOpen(null); });
          }} />
      )}
    </div>
  );
}
