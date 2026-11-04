import { useMemo } from "react";
import type { ClassSpellcasting, Ruleset } from "../../api/contract";
import { pickShown, type ShownRow, type VersionLike } from "../../lib/versions";
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

/** What the filter reads of a spell: a class's spell (`SpellEntry`) or a slot's option (`spellFacts`). */
export interface SpellFilterFacts {
  name: string;
  school: string;
  sourceName: string;
  level: number;
  ritual: boolean;
  concentration: boolean;
}

export const isFiltering = (f: SpellFilterState) => !!(f.text.trim() || f.school || f.source || f.levels.length || f.ritual || f.concentration);

export function matchesSpell(s: SpellFilterFacts, f: SpellFilterState): boolean {
  const q = f.text.trim().toLowerCase();
  if (q && ![s.name, s.school, s.sourceName].some((t) => t.toLowerCase().includes(q))) return false;
  if (f.school && s.school !== f.school) return false;
  if (f.source && s.sourceName !== f.source) return false;
  if (f.levels.length && !f.levels.includes(s.level)) return false;
  if (f.ritual && !s.ritual) return false;
  if (f.concentration && !s.concentration) return false;
  return true;
}

/** The filter's view of a selection slot's option (a spell offered to choose). */
export const spellFacts = (o: VersionLike): SpellFilterFacts => ({
  name: o.name, school: o.school ?? "", sourceName: o.sourceName ?? "", level: o.level ?? 0,
  ritual: !!o.spell?.ritual, concentration: !!o.spell?.concentration,
});

/**
 * GitHub #21: a selection slot's rows narrowed by the spell filter, the way the picker's own filters narrow them (B-55
 * sec. 1.5): a row stays when one of its versions passes (the chosen row always), and shows a version that passes.
 */
export function narrowRows<V extends VersionLike>(rows: readonly ShownRow<V>[], f: SpellFilterState, charEd: Ruleset): readonly ShownRow<V>[] {
  if (!isFiltering(f)) return rows;
  return rows.flatMap((r) => {
    const passing = r.versions.filter((v) => matchesSpell(spellFacts(v), f));
    const versions = r.chosen && !passing.includes(r.chosen) ? [...passing, r.chosen] : passing;
    if (versions.length === 0) return [];
    const shown = versions.includes(r.shown) ? r.shown : pickShown(versions, charEd, r.chosen?.id ?? null, null);
    return [{ ...r, versions, shown }];
  });
}

/** Class copy whose spell list is narrowed by the filter (the grid renders this; mutations still use the class slot id). */
export const filterClass = (c: ClassSpellcasting, f: SpellFilterState): ClassSpellcasting =>
  isFiltering(f) ? { ...c, spells: c.spells.filter((s) => matchesSpell(s, f)) } : c;

const uniq = (xs: string[]) => [...new Set(xs.filter(Boolean))].sort();

/**
 * The list the filter bar acts on: the spells it offers its school and source lists from, and how many of them it shows.
 * GitHub #21: while a slot is highlighted this is that slot's options (the class whose spells are being chosen).
 */
export interface FilterScope {
  /** Every spell (each version of a slot's rows) the school and source lists are built from. */
  pool: readonly SpellFilterFacts[];
  /** Rows the list shows, of `total`. */
  shown: number;
  total: number;
}

/** The scope of the class grids: every class's spells, narrowed by the filter. */
export const classesScope = (classes: readonly ClassSpellcasting[], f: SpellFilterState): FilterScope => {
  const pool = classes.flatMap((c) => c.spells);
  return { pool, shown: pool.filter((s) => matchesSpell(s, f)).length, total: pool.length };
};

/** SPELLCASTING tab filter bar: pin, name box, school, level 0-9, ritual, concentration, source, filtered/total badge. */
export function SpellFilterBar({ scope, filter, onChange }: { scope: FilterScope; filter: SpellFilterState; onChange: (f: SpellFilterState) => void }) {
  const { pool, shown, total } = scope;
  const schools = useMemo(() => uniq(pool.map((s) => s.school)), [pool]);
  const sources = useMemo(() => uniq(pool.map((s) => s.sourceName)), [pool]);
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
      <span className="sp-badge" aria-label="Filtered spells">{shown} / {total}</span>
    </div>
  );
}
