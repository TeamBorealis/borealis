import type { ItemSummary } from "../../api/contract";
import { ARMOR_COLUMNS, DEFAULT_COLUMNS, columnsFor } from "./itemColumns";

/** Catalogue grid; the column set follows the category like the classic builder (armor, weapons, everything else). */
export function ItemGrid({ items, category, picked, onPick, onAdd }: { items: ItemSummary[]; category: string; picked: string | null; onPick: (i: ItemSummary) => void; onAdd: (i: ItemSummary) => void }) {
  const cols = columnsFor(category);
  return (
    <div className="ap-scroll eq-scroll">
      <table className={`ap-grid eq-grid ${cols === DEFAULT_COLUMNS ? "" : "eq-ital"} ${cols === ARMOR_COLUMNS ? "eq-armor" : ""}`} aria-label="Item catalog">
        <thead>
          <tr>
            <th scope="col">Name</th>
            {cols.map((c) => <th key={c.key} scope="col" className={`al-${c.align} k-${c.key}`}>{c.label}</th>)}
          </tr>
        </thead>
        <tbody>
          {items.length === 0 && <tr className="ap-empty-row"><td colSpan={cols.length + 1}>No items match.</td></tr>}
          {items.map((i) => (
            <tr key={i.id} className={i.id === picked ? "sel" : ""} onMouseDown={() => onPick(i)} onDoubleClick={() => onAdd(i)}>
              <td className="eq-name"><button type="button" className="ap-name" onClick={() => onPick(i)}>{i.name}</button></td>
              {cols.map((c) => <td key={c.key} className={`al-${c.align} k-${c.key}`}>{c.cell(i)}</td>)}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
