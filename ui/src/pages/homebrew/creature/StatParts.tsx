import type {
  HomebrewAbilities, HomebrewArmorClassEntry, HomebrewChallengeValue, HomebrewChoice, HomebrewDamageClause, HomebrewInitiative, HomebrewSensesValue,
  HomebrewSpeeds,
} from "../../../api/contract";
import { CheckBox, ComboBox, Icon, NumericUpDown, TextBox } from "../../../components";
import { useCreatorEnv } from "../env";
import { ChipsField } from "../pickers/ChipsField";
import {
  ABILITIES, armorClassOf, capitalised, challengeOf, crProficiency, diceAverage, hitPointsOf, modifier, parseDice, sensesOf, signed, standardXp,
  usualPassive,
} from "./creature";
import "./creature.css";

/** A whole number box that may be left empty (a speed it doesn't have). */
export function OptionalNumber({ label, value, onChange, min = 0, max = 9999 }: {
  label: string; value: number | undefined; onChange: (n: number | undefined) => void; min?: number; max?: number;
}) {
  return (
    <TextBox className="hb-cr-num" inputMode="numeric" aria-label={label} value={value === undefined ? "" : String(value)}
      onChange={(e) => {
        const t = e.target.value.replace(/[^\d-]/g, "");
        if (t === "" || t === "-") { onChange(undefined); return; }
        const n = Number(t);
        if (Number.isFinite(n)) onChange(Math.min(max, Math.max(min, n)));
      }} />
  );
}

interface PartProps<T> { id: string; label: string; value: unknown; onChange: (v: T, typing: boolean) => void }

/** Armor class (spec 4.11): one row per form (number, what gives it, when), or the words for one that is worked out. */
export function ArmorClassField({ id, label, value, onChange }: PartProps<ReturnType<typeof armorClassOf>>) {
  const ac = armorClassOf(value);
  const set = (entries: HomebrewArmorClassEntry[], typing = false) => onChange({ ...ac, entries }, typing);
  const row = (i: number, patch: Partial<HomebrewArmorClassEntry>, typing = false) => set(ac.entries.map((e, j) => (j === i ? { ...e, ...patch } : e)), typing);
  return (
    <div className="hb-cr-part" id={id} role="group" aria-label={label}>
      {ac.entries.map((e, i) => (
        <span key={i} className="hb-row-controls hb-cr-ac">
          <NumericUpDown label={`${label} ${i + 1}`} value={e.value} min={1} max={40} compact onChange={(n) => row(i, { value: n })} />
          <TextBox aria-label={`${label} ${i + 1}: from`} placeholder="natural armor" value={e.source ?? ""} onChange={(ev) => row(i, { source: ev.target.value }, true)} />
          <TextBox aria-label={`${label} ${i + 1}: when`} placeholder="in bear form" value={e.condition ?? ""} onChange={(ev) => row(i, { condition: ev.target.value }, true)} />
          <button type="button" className="hb-chip-x" aria-label={`Remove ${label.toLowerCase()} ${i + 1}`} onClick={() => set(ac.entries.filter((_, j) => j !== i))}><Icon name="x" /></button>
        </span>
      ))}
      {ac.entries.length === 0 && (
        <TextBox aria-label={`${label}: words when it is worked out`} placeholder="11 + the spell's level" value={ac.note} onChange={(ev) => onChange({ ...ac, note: ev.target.value }, true)} />
      )}
      <button type="button" className="hb-pick-btn" onClick={() => set([...ac.entries, { value: 10 }])}><Icon name="plus" />Add an armor class</button>
    </div>
  );
}

/** Hit points: the dice give the average (shown, and changeable); with no dice and no average, the words for them. */
export function HitPointsField({ id, label, value, onChange }: PartProps<ReturnType<typeof hitPointsOf>>) {
  const hp = hitPointsOf(value);
  const worked = diceAverage(hp.formula);
  const setFormula = (formula: string) => {
    const avg = diceAverage(formula);
    const followed = hp.average === null || hp.average === diceAverage(hp.formula);
    onChange({ ...hp, formula, average: followed ? avg : hp.average }, true);
  };
  const badDice = hp.formula.trim() !== "" && parseDice(hp.formula) === null;
  return (
    <div className="hb-cr-part" id={id} role="group" aria-label={label}>
      <span className="hb-row-controls">
        <span className="hb-inline-label">Hit dice</span>
        <TextBox aria-label={`${label}: hit dice`} placeholder="2d8+2" invalid={badDice} value={hp.formula} onChange={(e) => setFormula(e.target.value)} />
        <span className="hb-inline-label">Average</span>
        <OptionalNumber label={`${label}: average`} value={hp.average ?? undefined} min={1} onChange={(n) => onChange({ ...hp, average: n ?? null }, true)} />
        {worked !== null && hp.average !== worked && <span className="hb-field-help">The dice give {worked}.</span>}
      </span>
      {hp.average === null && hp.formula.trim() === "" && (
        <TextBox aria-label={`${label}: words when they are worked out`} placeholder="40 + 10 for each spell level above 4" value={hp.note}
          onChange={(e) => onChange({ ...hp, note: e.target.value }, true)} />
      )}
    </div>
  );
}

const SPEEDS: readonly (keyof HomebrewSpeeds & string)[] = ["walk", "burrow", "climb", "fly", "swim"];

export function SpeedsField({ id, label, value, onChange }: PartProps<HomebrewSpeeds>) {
  const s = (typeof value === "object" && value !== null ? value : {}) as HomebrewSpeeds;
  const set = (patch: Partial<HomebrewSpeeds>, typing = false) => {
    const next: HomebrewSpeeds = { ...s, ...patch };
    for (const k of Object.keys(next) as (keyof HomebrewSpeeds)[]) if (next[k] === undefined || next[k] === false) delete next[k];
    onChange(next, typing);
  };
  return (
    <div className="hb-cr-part hb-cr-grid" id={id} role="group" aria-label={label}>
      {SPEEDS.map((k) => (
        <label key={k} className="hb-cr-cell"><span className="hb-inline-label">{capitalised(k)}</span>
          <OptionalNumber label={`${capitalised(k)} speed`} value={s[k] as number | undefined} onChange={(n) => set({ [k]: n })} /></label>
      ))}
      <label className="hb-cr-cell"><CheckBox checked={s.hover === true} onChange={(e) => set({ hover: e.target.checked })} />Hover</label>
      <TextBox aria-label={`${label}: note`} placeholder="40 ft. in boar form" value={s.note ?? ""} onChange={(e) => set({ note: e.target.value || undefined }, true)} />
    </div>
  );
}

export function InitiativeField({ id, label, value, onChange }: PartProps<HomebrewInitiative | null>) {
  const env = useCreatorEnv();
  const dex = modifier(Number((env.form?.fields.abilities as HomebrewAbilities | undefined)?.dex ?? 10));
  const init = (typeof value === "object" && value !== null ? value : null) as HomebrewInitiative | null;
  if (!init) {
    return <button type="button" id={id} className="hb-pick-btn" onClick={() => onChange({ modifier: dex, score: 10 + dex }, false)}><Icon name="plus" />Add initiative ({signed(dex)})</button>;
  }
  return (
    <span className="hb-row-controls" id={id} role="group" aria-label={label}>
      <NumericUpDown label={`${label} modifier`} value={init.modifier} min={-10} max={30} compact onChange={(n) => onChange({ modifier: n, score: 10 + n }, false)} />
      <span className="hb-field-help">Score {init.score}</span>
      <button type="button" className="hb-chip-x" aria-label={`Remove ${label.toLowerCase()}`} onClick={() => onChange(null, false)}><Icon name="x" /></button>
    </span>
  );
}

/** The six scores side by side with their modifiers, as the stat block shows them. */
export function AbilitiesField({ id, label, value, onChange }: PartProps<HomebrewAbilities>) {
  const a = { str: 10, dex: 10, con: 10, int: 10, wis: 10, cha: 10, ...(typeof value === "object" && value !== null ? value : {}) } as HomebrewAbilities;
  return (
    <div className="hb-cr-abilities" id={id} role="group" aria-label={label}>
      {ABILITIES.map((ab) => (
        <div key={ab.key} className="hb-cr-ability">
          <span className="hb-cr-ab-name">{ab.short}</span>
          <NumericUpDown label={ab.label} value={a[ab.key]} min={1} max={30} compact onChange={(n) => onChange({ ...a, [ab.key]: n }, false)} />
          <span className="hb-cr-ab-mod">{signed(modifier(a[ab.key]))}</span>
        </div>
      ))}
    </div>
  );
}

/** Bonuses by key (saving throws or skills): rows of a key and its whole bonus; Add picks a key not listed yet. */
export function BonusesField({ id, label, value, choices, onChange }: PartProps<Record<string, number>> & { choices: readonly HomebrewChoice[] }) {
  const m = (typeof value === "object" && value !== null ? value : {}) as Record<string, number>;
  const keys = choices.map((c) => c.value).filter((k) => k in m);
  const free = choices.filter((c) => !(c.value in m));
  const labelOf = (k: string) => choices.find((c) => c.value === k)?.label ?? k;
  return (
    <div className="hb-cr-part" id={id} role="group" aria-label={label}>
      {keys.map((k) => (
        <span key={k} className="hb-row-controls">
          <span className="hb-cr-key">{labelOf(k)}</span>
          <NumericUpDown label={`${labelOf(k)} bonus`} value={m[k] ?? 0} min={-10} max={30} compact onChange={(n) => onChange({ ...m, [k]: n }, false)} />
          <button type="button" className="hb-chip-x" aria-label={`Remove ${labelOf(k)}`} onClick={() => { const { [k]: _r, ...rest } = m; onChange(rest, false); }}><Icon name="x" /></button>
        </span>
      ))}
      {free.length > 0 && (
        <ComboBox aria-label={`Add to ${label.toLowerCase()}`} value="" options={[{ value: "", label: "Add..." }, ...free]} onChange={(k) => k && onChange({ ...m, [k]: 0 }, false)} />
      )}
    </div>
  );
}

/** Damage lines: the types each names, and its words when it says more ("from nonmagical attacks"). */
export function DamageField({ id, label, value, choices, onChange }: PartProps<HomebrewDamageClause[]> & { choices: readonly HomebrewChoice[] }) {
  const rows = Array.isArray(value) ? value as HomebrewDamageClause[] : [];
  const set = (i: number, c: HomebrewDamageClause, typing = false) => onChange(rows.map((r, j) => (j === i ? c : r)), typing);
  return (
    <div className="hb-cr-part" id={id} role="group" aria-label={label}>
      {rows.map((r, i) => (
        <div key={i} className="hb-cr-damage">
          <ChipsField id={`${id}-${i}`} label={`${label} ${i + 1}`} value={r.types ?? []} choices={choices} allowCustom={false} onChange={(types) => set(i, { ...r, types })} />
          <TextBox aria-label={`${label} ${i + 1}: words`} placeholder="from nonmagical attacks" value={r.text ?? ""} onChange={(e) => set(i, { ...r, text: e.target.value || undefined }, true)} />
          <button type="button" className="hb-chip-x" aria-label={`Remove ${label.toLowerCase()} ${i + 1}`} onClick={() => onChange(rows.filter((_, j) => j !== i), false)}><Icon name="x" /></button>
        </div>
      ))}
      <button type="button" className="hb-pick-btn" onClick={() => onChange([...rows, { types: [] }], false)}><Icon name="plus" />Add a line</button>
    </div>
  );
}

const SENSES: readonly ["blindsight" | "darkvision" | "tremorsense" | "truesight", string][] = [
  ["blindsight", "Blindsight"], ["darkvision", "Darkvision"], ["tremorsense", "Tremorsense"], ["truesight", "Truesight"],
];

/** Senses (feet) and the passive Perception, which follows Wisdom and Perception until it is changed. */
export function SensesField({ id, label, value, onChange }: PartProps<HomebrewSensesValue>) {
  const env = useCreatorEnv();
  const s = sensesOf(value);
  const usual = usualPassive(env.form?.fields.abilities, env.form?.fields.skills);
  const set = (patch: Partial<HomebrewSensesValue>, typing = false) => onChange({ ...s, ...patch }, typing);
  return (
    <div className="hb-cr-part hb-cr-grid" id={id} role="group" aria-label={label}>
      {SENSES.map(([k, l]) => (
        <label key={k} className="hb-cr-cell"><span className="hb-inline-label">{l}</span>
          <OptionalNumber label={l} value={s[k]} onChange={(n) => set({ [k]: n })} /></label>
      ))}
      <label className="hb-cr-cell"><CheckBox checked={s.blind_beyond === true} onChange={(e) => set({ blind_beyond: e.target.checked || undefined })} />Blind beyond</label>
      <label className="hb-cr-cell"><span className="hb-inline-label">Passive Perception</span>
        <OptionalNumber label="Passive Perception" value={s.passive_perception ?? usual} onChange={(n) => set({ passive_perception: n === undefined || n === usual ? null : n })} /></label>
    </div>
  );
}

/** Challenge rating (or none, with the words that stand in for it), XP and proficiency bonus following it unless changed. */
export function ChallengeField({ id, label, value, choices, onChange }: PartProps<HomebrewChallengeValue> & { choices: readonly HomebrewChoice[] }) {
  const c = challengeOf(value);
  const set = (patch: Partial<HomebrewChallengeValue>, typing = false) => onChange({ ...c, ...patch }, typing);
  const xp = standardXp(c.cr);
  const pb = crProficiency(c.cr);
  return (
    <div className="hb-cr-part" id={id} role="group" aria-label={label}>
      <span className="hb-row-controls">
        <span className="hb-inline-label">Challenge rating</span>
        <ComboBox aria-label="Challenge rating" value={c.cr} options={[{ value: "", label: "None" }, ...choices]}
          onChange={(cr) => set({ cr, xp: null, proficiencyBonus: null, ...(cr ? { note: "" } : {}) })} />
        {c.cr && (
          <>
            <span className="hb-inline-label">XP</span>
            <OptionalNumber label="XP" value={c.xp ?? xp ?? undefined} onChange={(n) => set({ xp: n === undefined || n === xp ? null : n })} />
            <span className="hb-inline-label">In its lair</span>
            <OptionalNumber label="XP in its lair" value={c.xpLair ?? undefined} onChange={(n) => set({ xpLair: n ?? null })} />
          </>
        )}
        <span className="hb-inline-label">Proficiency bonus</span>
        <OptionalNumber label="Proficiency bonus" value={c.proficiencyBonus ?? pb ?? undefined} min={2} max={9}
          onChange={(n) => set({ proficiencyBonus: n === undefined || n === pb ? null : n })} />
      </span>
      {!c.cr && <TextBox aria-label={`${label}: words when it has none`} placeholder="Scales with the spell's level" value={c.note} onChange={(e) => set({ note: e.target.value }, true)} />}
    </div>
  );
}
