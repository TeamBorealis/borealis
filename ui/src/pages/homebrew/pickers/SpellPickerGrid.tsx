import { useState } from "react";
import type { HomebrewPickerRow } from "../../../api/contract";
import { ComponentCells, componentText } from "../../spells/SpellGrid";
import { EMPTY_FILTER, isFiltering, matchesSpell, SpellFilterBar, type SpellFilterState } from "../../spells/SpellFilter";
import { arrowNav } from "../../spells/rowNav";
import { RowDetail } from "../../../components/RowDetail";
import { Glyph } from "../../equipment/Glyph";
import "../../equipment/equipment.css";
import "../../spells/magic.css";
import { castingOf, spellFactsOf } from "./refRows";

const levelText = (r: HomebrewPickerRow): string => `${(r.level ?? 0) === 0 ? "Cantrip" : `Level ${r.level}`}${r.school ? ` · ${r.school}` : ""}`;

export interface SpellPickerGridProps {
  rows: readonly HomebrewPickerRow[];
  total: number;
  text: string;
  onText: (text: string) => void;
  picked: (id: string) => boolean;
  highlighted: string | null;
  onDescribe: (row: HomebrewPickerRow) => void;
  onChoose: (row: HomebrewPickerRow) => void;
  multiple: boolean;
  /** SPELL LISTS (spec 12.4): rows chosen for "Add selected" / "Remove selected" are highlighted. */
  isSelected?: (id: string) => boolean;
  /** A click on a row before it is described; true when it was taken (Ctrl/Shift+click chooses rows). */
  onRowClick?: (row: HomebrewPickerRow, e: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }) => boolean;
  /** A check box that can't change (a spell another pack put on the list), with why. */
  locked?: (id: string) => string | null;
  /** Phones (spec 16): the filter bar folds behind a Filter button. */
  foldFilter?: boolean;
}

/**
 * The spell picker's body (spec 5.1, 13.7): the Magic filter bar (filter text, school, level toggles 0-9, ritual,
 * concentration, source, "filtered / total") over the Magic grid (check / NAME / LEVEL & SCHOOL / V / S / M / SOURCE).
 * A click describes the spell, the check (or a double-click, Enter) picks it.
 */
export function SpellPickerGrid({ rows, total, text, onText, picked, highlighted, onDescribe, onChoose, multiple, isSelected, onRowClick, locked, foldFilter = false }: SpellPickerGridProps) {
  const [filter, setFilter] = useState<SpellFilterState>({ ...EMPTY_FILTER, text });
  const [filterOpen, setFilterOpen] = useState(false);
  const set = (f: SpellFilterState) => { setFilter(f); if (f.text !== filter.text) onText(f.text); };
  const pool = rows.map(spellFactsOf);
  // The typed text already narrowed the rows on the backend; the bar narrows the rest on screen.
  const local = { ...filter, text: "" };
  const shown = rows.filter((r) => matchesSpell(spellFactsOf(r), local));
  const byId = (id: string) => rows.find((r) => r.id === id);
  return (
    <div className="hb-spell-picker">
      {foldFilter && (
        <button type="button" className="hb-tool hb-filter-fold" aria-expanded={filterOpen} onClick={() => setFilterOpen(!filterOpen)}>
          Filter{isFiltering(filter) ? ` (${shown.length} / ${total})` : ""}
        </button>
      )}
      {(!foldFilter || filterOpen) && <SpellFilterBar scope={{ pool, shown: shown.length, total }} filter={filter} onChange={set} />}
      <div className="ap-scroll sp-main">
        <table className="ap-grid eq-grid sp-grid" aria-label="Spells">
          <thead>
            <tr>
              <th scope="col" className="sp-ck"><Glyph name="check" label={multiple ? "Chosen" : "Choose"} /></th>
              <th scope="col" className="sp-name">Name</th>
              <th scope="col" className="sp-lvl">Level &amp; school</th>
              <th scope="col" className="sp-vsm al-c">Verbal</th>
              <th scope="col" className="sp-vsm al-c">Somatic</th>
              <th scope="col" className="sp-vsm al-c">Material</th>
              <th scope="col" className="sp-src">Source</th>
            </tr>
          </thead>
          <tbody>
            {shown.length === 0 && <tr className="ap-empty-row"><td colSpan={7}>No spells to show.</td></tr>}
            {shown.map((r) => (
              <tr key={`${r.type}:${r.id}`} data-spell-id={r.id} className={highlighted === r.id || isSelected?.(r.id) ? "sel" : ""} aria-selected={isSelected ? isSelected(r.id) : undefined}
                onMouseDown={(e) => { if (onRowClick?.(r, e)) return; onDescribe(r); }}
                onDoubleClick={() => onChoose(r)} onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); onChoose(r); return; } arrowNav(e, (id) => { const n = byId(id); if (n) onDescribe(n); }); }}>
                <td className="sp-ck">
                  <input type="checkbox" className="ap-check" aria-label={`Choose ${r.name}`} checked={picked(r.id)} onChange={() => onChoose(r)}
                    disabled={!!locked?.(r.id)} title={locked?.(r.id) ?? undefined} />
                </td>
                <td className="sp-name"><button type="button" className="ap-name" onClick={() => onDescribe(r)}>{r.name}</button><RowDetail parts={[levelText(r), componentText(castingOf(r)), r.sourceName]} /></td>
                <td className="sp-lvl">{levelText(r)}</td>
                <ComponentCells casting={castingOf(r)} />
                <td className="sp-src">{r.sourceName}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
