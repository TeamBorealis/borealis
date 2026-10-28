import type { ReactNode } from "react";
import type { ItemSummary, Ruleset } from "../../api/contract";
import { RowDetail } from "../../components/RowDetail";
import { editionOf, NO_FILTER, visibleRows, groupVersions, type ShownRow } from "../../lib/versions";
import { VersionButton } from "../builder/VersionMenu";
import { ARMOR_COLUMNS, DEFAULT_COLUMNS, columnsFor, type ItemRow } from "./itemColumns";

/** One add-item row (B-55 W3): one name, its shown version and the versions the filters keep. */
export type ItemListRow = ShownRow<ItemSummary>;

/** The item as a grid row: an other-edition version carries its edition, for the " (2014)" mark after its source. */
export function displayRow(i: ItemSummary, charEd: Ruleset): ItemRow {
  const ed = editionOf(i);
  return ed === "any" || ed === charEd ? i : { ...i, edition: ed };
}

/** Every item as a row, grouped by name and unfiltered (the base-item list). */
export const plainRows = (items: readonly ItemSummary[], charEd: Ruleset): ItemListRow[] => visibleRows(groupVersions(items), NO_FILTER, charEd, null, {});

interface Props {
  rows: ItemListRow[];
  category: string;
  charEd: Ruleset;
  picked: string | null;
  onPick: (i: ItemSummary) => void;
  onAdd: (i: ItemSummary) => void;
  /** Opens the row's version menu under the count button. */
  onVersions?: (row: ItemListRow, anchor: HTMLElement) => void;
  /** Key of the row whose menu is open. */
  menuKey?: string | null;
  /** Shown instead of the rows when the list has items but the filters keep none. */
  emptyText?: ReactNode;
  label?: string;
}

/**
 * Catalogue grid; the column set follows the category like the classic builder (armor, weapons, everything else). B-55 W3:
 * one row per name showing its shown version; a narrow version column (count + arrow) on every row once a row has versions.
 */
export function ItemGrid({ rows, category, charEd, picked, onPick, onAdd, onVersions, menuKey = null, emptyText, label = "Item catalog" }: Props) {
  const cols = columnsFor(category);
  const versionColumn = rows.some((r) => r.versions.length > 1);
  const span = cols.length + 1 + (versionColumn ? 1 : 0);
  return (
    <div className="ap-scroll eq-scroll">
      <table className={`ap-grid eq-grid ${cols === DEFAULT_COLUMNS ? "" : "eq-ital"} ${cols === ARMOR_COLUMNS ? "eq-armor" : ""}`} aria-label={label}>
        <thead>
          <tr>
            <th scope="col">Name</th>
            {cols.map((c) => <th key={c.key} scope="col" className={`al-${c.align} k-${c.key}`}>{c.label}</th>)}
            {versionColumn && <th scope="col" className="eq-ver"><span className="sr-only">Versions</span></th>}
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 && <tr className="ap-empty-row"><td colSpan={span}>{emptyText ?? "No items match."}</td></tr>}
          {rows.map((row) => {
            const i = row.shown;
            const r = displayRow(i, charEd);
            const sel = row.versions.some((v) => v.id === picked);
            return (
              <tr key={row.key} className={sel ? "sel" : ""} onMouseDown={() => onPick(i)} onDoubleClick={() => onAdd(i)}>
                <td className="eq-name"><button type="button" className="ap-name" onClick={() => onPick(i)}>{i.name}</button><RowDetail parts={cols.map((c) => c.cell(r))} /></td>
                {cols.map((c) => <td key={c.key} className={`al-${c.align} k-${c.key}`}>{c.cell(r)}</td>)}
                {versionColumn && (
                  <td className="eq-ver" onMouseDown={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()}>
                    {row.versions.length > 1 && onVersions && (
                      <VersionButton count={row.versions.length} rowName={row.name} open={menuKey === row.key} onOpen={(anchor) => onVersions(row, anchor)} />
                    )}
                  </td>
                )}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
