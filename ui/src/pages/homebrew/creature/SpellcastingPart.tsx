import { useState } from "react";
import type { CreatureAbility, HomebrewRef, HomebrewSpellFrequency, HomebrewSpellList, HomebrewStatSpellcasting } from "../../../api/contract";
import { ComboBox, Icon, NumericUpDown, TextBox } from "../../../components";
import { ReferencePicker } from "../pickers/ReferencePicker";
import { rememberRefs } from "../pickers/RefField";
import { ABILITIES } from "./creature";

const FREQUENCIES = [
  { value: "at_will", label: "At will" }, { value: "1", label: "1/Day" }, { value: "2", label: "2/Day" }, { value: "3", label: "3/Day" },
  { value: "1e", label: "1/Day each" }, { value: "2e", label: "2/Day each" }, { value: "3e", label: "3/Day each" },
];

const frequencyKey = (f: HomebrewSpellFrequency): string => (f === "at_will" ? "at_will" : `${f.per_day.uses}${f.per_day.each ? "e" : ""}`);
const frequencyOf = (key: string): HomebrewSpellFrequency => (key === "at_will" ? "at_will" : { per_day: { uses: Number.parseInt(key, 10), each: key.endsWith("e") } });

/** One list: how often, then its spells (picked with the spell picker; a spell not loaded keeps its name). */
function SpellListRow({ list, index, onChange, onRemove }: { list: HomebrewSpellList; index: number; onChange: (l: HomebrewSpellList) => void; onRemove: () => void }) {
  const [picking, setPicking] = useState(false);
  const label = `Spell list ${index + 1}`;
  const add = (picks: HomebrewRef[]) => {
    rememberRefs(picks);
    setPicking(false);
    const known = new Set(list.spells.map((s) => s.id ?? s.name));
    onChange({ ...list, spells: [...list.spells, ...picks.filter((p) => !known.has(p.id)).map((p) => ({ name: p.name, id: p.id }))] });
  };
  return (
    <div className="hb-cr-spell-list" role="group" aria-label={label}>
      <span className="hb-row-controls">
        <ComboBox aria-label={`${label}: how often`} value={frequencyKey(list.frequency)} options={FREQUENCIES} onChange={(k) => onChange({ ...list, frequency: frequencyOf(k) })} />
        <button type="button" className="hb-pick-btn" aria-haspopup="dialog" onClick={() => setPicking(true)}><Icon name="search" />Add spells</button>
        <button type="button" className="hb-chip-x" aria-label={`Remove ${label.toLowerCase()}`} onClick={onRemove}><Icon name="x" /></button>
      </span>
      <ul className="hb-ref-list" aria-label={`${label}: spells`}>
        {list.spells.map((s, j) => (
          <li key={`${s.id ?? s.name}-${j}`} className="hb-chip-item">
            <span className="hb-ref-name">{s.name}{s.note ? ` (${s.note})` : ""}</span>
            <button type="button" className="hb-chip-x" aria-label={`Remove ${s.name}`} onClick={() => onChange({ ...list, spells: list.spells.filter((_, k) => k !== j) })}><Icon name="x" /></button>
          </li>
        ))}
      </ul>
      {picking && <ReferencePicker title={`${label}: spells`} target={{ types: ["Spell"], multiple: true }} chosen={list.spells.flatMap((s) => (s.id ? [s.id] : []))}
        onChoose={add} onClose={() => setPicking(false)} />}
    </div>
  );
}

/** A Spellcasting entry's ability, save DC, attack bonus and spell lists (spec 4.11 "spellcasting blocks (spell pickers)"). */
export function SpellcastingPart({ value, onChange }: { value: HomebrewStatSpellcasting; onChange: (v: HomebrewStatSpellcasting, typing: boolean) => void }) {
  const set = (patch: Partial<HomebrewStatSpellcasting>) => onChange({ ...value, ...patch }, false);
  return (
    <div className="hb-cr-part" role="group" aria-label="Spellcasting">
      <span className="hb-row-controls">
        <span className="hb-inline-label">Ability</span>
        <ComboBox aria-label="Spellcasting ability" value={value.ability ?? ""} options={[{ value: "", label: "" }, ...ABILITIES.map((a) => ({ value: a.key, label: a.label }))]}
          onChange={(a) => set({ ability: (a || undefined) as CreatureAbility | undefined })} />
        <span className="hb-inline-label">Save DC</span>
        <NumericUpDown label="Spell save DC" value={value.save_dc ?? 10} min={1} max={30} compact onChange={(n) => set({ save_dc: n })} />
        <span className="hb-inline-label">Attack bonus</span>
        <TextBox className="hb-cr-num" aria-label="Spell attack bonus" inputMode="numeric" value={value.attack_bonus === undefined ? "" : String(value.attack_bonus)}
          onChange={(e) => { const t = e.target.value.trim(); set({ attack_bonus: t === "" || Number.isNaN(Number(t)) ? undefined : Number(t) }); }} />
      </span>
      {value.lists.map((l, i) => (
        <SpellListRow key={i} list={l} index={i} onChange={(next) => set({ lists: value.lists.map((x, j) => (j === i ? next : x)) })}
          onRemove={() => set({ lists: value.lists.filter((_, j) => j !== i) })} />
      ))}
      <button type="button" className="hb-pick-btn" onClick={() => set({ lists: [...value.lists, { frequency: "at_will", spells: [] }] })}><Icon name="plus" />Add a spell list</button>
    </div>
  );
}
