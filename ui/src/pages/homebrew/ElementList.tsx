import { type RefObject } from "react";
import type { HomebrewElementSummary, HomebrewKindDescriptor } from "../../api/contract";
import { Icon, RowDetail } from "../../components";
import { Band } from "../spells/TwoPane";
import { Glyph } from "../equipment/Glyph";
import "../equipment/equipment.css";
import "../spells/pane.css";

/** The row of an open element that has not been saved yet (it has no id until its first save). */
export interface UnsavedRow {
  name: string;
}

const editionText = (e: HomebrewElementSummary["edition"]): string => (e === "dnd2014" ? "2014" : e === "dnd2024" ? "2024" : "Pack");

/** CATEGORIES (spec 13.2, the Equipment catalogue's panel): one row per kind with its count ("Spells (12)"). */
export function Categories({ kinds, counts, current, onPick }: {
  kinds: readonly HomebrewKindDescriptor[]; counts: ReadonlyMap<string, number>; current: string | null; onPick: (kindId: string) => void;
}) {
  return (
    <nav className="eq-cats hb-cats" aria-label="Element kinds">
      <Band title="Categories" />
      <div className="eq-cats-sub">Kind</div>
      <ul>
        {kinds.map((k) => (
          <li key={k.id}><button type="button" aria-current={k.id === current} onClick={() => onPick(k.id)}>{k.pluralLabel} ({counts.get(k.id) ?? 0})</button></li>
        ))}
      </ul>
    </nav>
  );
}

/** Phones (spec 16): the kinds become a chip row above the list. */
export function KindChips({ kinds, counts, current, onPick }: {
  kinds: readonly HomebrewKindDescriptor[]; counts: ReadonlyMap<string, number>; current: string | null; onPick: (kindId: string) => void;
}) {
  return (
    <div className="hb-chips" role="group" aria-label="Element kinds">
      {kinds.map((k) => (
        <button key={k.id} type="button" className="hb-chip" aria-pressed={k.id === current} onClick={() => onPick(k.id)}>{k.pluralLabel} ({counts.get(k.id) ?? 0})</button>
      ))}
    </div>
  );
}

/**
 * The element grid of one kind (spec 13.2): NAME / EDITION, a mark for problems and one for unsaved changes, then the tool
 * row (new, duplicate, the filter box Ctrl+F focuses).
 */
export function ElementList({ kind, rows, unsaved, openId, filter, filterRef, onFilter, onOpen, onNew, onDuplicate }: {
  kind: HomebrewKindDescriptor | null; rows: readonly HomebrewElementSummary[]; unsaved: UnsavedRow | null; openId: string | null;
  filter: string; filterRef: RefObject<HTMLInputElement>; onFilter: (text: string) => void;
  onOpen: (row: HomebrewElementSummary) => void; onNew: () => void; onDuplicate: () => void;
}) {
  const q = filter.trim().toLowerCase();
  const shown = rows.filter((r) => !q || r.name.toLowerCase().includes(q));
  return (
    <div className="hb-elements">
      <Band title={kind?.pluralLabel ?? "Elements"} />
      <div className="ap-scroll hb-element-scroll">
        <table className="ap-grid" aria-label="Elements">
          <thead><tr><th>Name</th><th>Edition</th><th aria-label="Marks" /></tr></thead>
          <tbody>
            {unsaved && (
              <tr className="sel" aria-current="true">
                <td className="grow"><span className="ap-name hb-new-name">{unsaved.name}</span></td>
                <td>New</td>
                <td><span className="hb-mark-draft" title="Not saved yet"><Icon name="pen" /></span></td>
              </tr>
            )}
            {shown.map((r) => {
              const n = r.problems.errors + r.problems.warnings;
              return (
                <tr key={r.id} className={r.id === openId ? "sel" : ""} aria-current={r.id === openId || undefined}>
                  <td className="grow">
                    <button type="button" className="ap-name" onClick={() => onOpen(r)}>{r.name}</button>
                    <RowDetail parts={[editionText(r.edition), r.hasDraft && "Unsaved changes", n > 0 && `${n} ${n === 1 ? "problem" : "problems"}`]} />
                  </td>
                  <td className="hide-narrow">{editionText(r.edition)}</td>
                  <td className="hide-narrow">
                    {n > 0 && <span className={`hb-mark ${r.problems.errors ? "hb-error" : "hb-warning"}`} title={`${n} ${n === 1 ? "problem" : "problems"}`}>{n}</span>}
                    {r.hasDraft && <span className="hb-mark-draft" title="Unsaved changes"><Icon name="pen" /></span>}
                  </td>
                </tr>
              );
            })}
            {shown.length === 0 && !unsaved && (
              <tr className="ap-empty-row"><td colSpan={3}>{rows.length === 0 ? `No ${kind?.pluralLabel.toLowerCase() ?? "elements"} in this pack yet.` : "Nothing matches the filter."}</td></tr>
            )}
          </tbody>
        </table>
      </div>
      <div className="hb-tools">
        <span className="eq-cmds">
          <button type="button" className="hb-tool" disabled={!kind} title="New element (Ctrl+Shift+N)" onClick={onNew}>New</button>
          <button type="button" className="hb-tool" disabled={!openId} title="Duplicate (Ctrl+D)" onClick={onDuplicate}>Duplicate</button>
        </span>
        <span className="eq-filter">
          <input ref={filterRef} type="text" aria-label="Filter elements" placeholder="filter names... (Ctrl+F)" value={filter} onChange={(e) => onFilter(e.target.value)} />
          <button type="button" aria-label="Clear" onClick={() => onFilter("")}><Glyph name="x" /></button>
        </span>
      </div>
    </div>
  );
}
