import { useMemo } from "react";
import type { ClassSpellcasting, SpellEntry } from "../../api/contract";
import { ComboBox } from "../../components";
import { Glyph } from "../equipment/Glyph";

export interface SpellFilterState {
  pinned: boolean;
  text: string;
  school: string;
  source: string;
  levels: readonly number[];
  ritual: boolean;
  concentration: boolean;
}

export const EMPTY_FILTER: SpellFilterState = { pinned: false, text: "", school: "", source: "", levels: [], ritual: false, concentration: false };
const LEVELS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] as const;

const srcOf = (s: SpellEntry) => s.sourceName;
export const isFiltering = (f: SpellFilterState) => !!(f.text.trim() || f.school || f.source || f.levels.length || f.ritual || f.concentration);

export function matchesSpell(s: SpellEntry, f: SpellFilterState): boolean {
  const q = f.text.trim().toLowerCase();
  if (q && ![s.name, s.school, srcOf(s)].some((t) => t.toLowerCase().includes(q))) return false;
  if (f.school && s.school !== f.school) return false;
  if (f.source && srcOf(s) !== f.source) return false;
  if (f.levels.length && !f.levels.includes(s.level)) return false;
  if (f.ritual && !s.ritual) return false;
  if (f.concentration && !s.concentration) return false;
  return true;
}

/** Class copy whose spell list is narrowed by the filter (the grid renders this; mutations still use the class slot id). */
export const filterClass = (c: ClassSpellcasting, f: SpellFilterState): ClassSpellcasting =>
  isFiltering(f) ? { ...c, spells: c.spells.filter((s) => matchesSpell(s, f)) } : c;

const uniq = (xs: string[]) => [...new Set(xs.filter(Boolean))].sort();

/** SPELLCASTING tab filter bar: pin, name box, school, level 0-9, ritual, concentration, source, filtered/total badge. */
export function SpellFilterBar({ classes, filter, onChange }: { classes: ClassSpellcasting[]; filter: SpellFilterState; onChange: (f: SpellFilterState) => void }) {
  const all = useMemo(() => classes.flatMap((c) => c.spells), [classes]);
  const schools = useMemo(() => uniq(all.map((s) => s.school)), [all]);
  const sources = useMemo(() => uniq(all.map(srcOf)), [all]);
  const shown = all.filter((s) => matchesSpell(s, filter)).length;
  const set = (p: Partial<SpellFilterState>) => onChange({ ...filter, ...p });
  const toggleLevel = (n: number) => set({ levels: filter.levels.includes(n) ? filter.levels.filter((x) => x !== n) : [...filter.levels, n] });
  return (
    <div className="sp-filter" role="search" aria-label="Spell filter">
      <button type="button" className="sp-pin" aria-label="Pin filter" aria-pressed={filter.pinned} title="Keep the filter when switching sections" onClick={() => set({ pinned: !filter.pinned })}><Glyph name="pin" /></button>
      <span className="eq-filter sp-text">
        <input type="text" aria-label="Filter spells" placeholder="filter name and keywords..." value={filter.text} onChange={(e) => set({ text: e.target.value })} />
        <button type="button" aria-label="Clear filter text" onClick={() => set({ text: "" })}><Glyph name="x" /></button>
      </span>
      {schools.length > 1 && <span className="sp-combo"><ComboBox aria-label="School filter" value={filter.school} onChange={(e) => set({ school: e.target.value })} options={[{ value: "", label: "--" }, ...schools.map((s) => ({ value: s, label: s }))]} /></span>}
      <span className="sp-levels" role="group" aria-label="Spell levels">
        {LEVELS.map((n) => <button key={n} type="button" className="sp-tog" aria-pressed={filter.levels.includes(n)} title={n === 0 ? "Cantrips" : `Level ${n}`} onClick={() => toggleLevel(n)}>{n}</button>)}
      </span>
      <button type="button" className="sp-tog sp-wide" aria-pressed={filter.ritual} onClick={() => set({ ritual: !filter.ritual })}>Ritual</button>
      <button type="button" className="sp-tog sp-wide" aria-pressed={filter.concentration} onClick={() => set({ concentration: !filter.concentration })}>Concentration</button>
      {sources.length > 0 && <span className="sp-combo"><ComboBox aria-label="Source filter" value={filter.source} onChange={(e) => set({ source: e.target.value })} options={[{ value: "", label: "--" }, ...sources.map((s) => ({ value: s, label: s }))]} /></span>}
      <span className="sp-badge" aria-label="Filtered spells">{shown} / {all.length}</span>
    </div>
  );
}
