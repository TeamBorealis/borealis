import { type MouseEvent, type RefObject, useState } from "react";
import type { HomebrewDraftSummary, HomebrewElementSummary, HomebrewKindDescriptor } from "../../api/contract";
import { Icon, RowDetail } from "../../components";
import { Band } from "../spells/TwoPane";
import { Glyph } from "../equipment/Glyph";
import { usePhone } from "../../lib/phone";
import "../equipment/equipment.css";
import "../spells/pane.css";
import { filterRows, isSearching, type ListFilter, nextSelection } from "./search";
import { overrideNote } from "./Overrides";

/** The row of an open element that has not been saved yet (it has no id until its first save). */
export interface UnsavedRow {
  name: string;
  /** Its Edition field (null = the pack's). */
  edition: HomebrewElementSummary["edition"];
}

const editionText = (e: HomebrewElementSummary["edition"]): string => (e === "dnd2014" ? "2014" : e === "dnd2024" ? "2024" : "Pack");

/** A CATEGORIES row that opens a view of the pack rather than a kind's elements (SPELL LISTS, spec 12.4). */
export interface ExtraCategory {
  label: string;
  current: boolean;
  onPick: () => void;
}

/** CATEGORIES (spec 13.2, the Equipment catalogue's panel): one row per kind with its count ("Spells (12)"), then the
 * pack's views. */
export function Categories({ kinds, counts, current, onPick, extra = [] }: {
  kinds: readonly HomebrewKindDescriptor[]; counts: ReadonlyMap<string, number>; current: string | null; onPick: (kindId: string) => void;
  extra?: readonly ExtraCategory[];
}) {
  return (
    <nav className="eq-cats hb-cats" aria-label="Element kinds">
      <Band title="Categories" />
      {/* Aurora Equipment: CATEGORIES, then the CATEGORY column header (U7) */}
      <div className="eq-cats-sub">Category</div>
      <ul>
        {kinds.map((k) => (
          <li key={k.id}><button type="button" aria-current={k.id === current && !extra.some((x) => x.current)} onClick={() => onPick(k.id)}>{k.pluralLabel} ({counts.get(k.id) ?? 0})</button></li>
        ))}
      </ul>
      {extra.length > 0 && (
        <>
          <div className="eq-cats-sub">Pack</div>
          <ul>{extra.map((x) => <li key={x.label}><button type="button" aria-current={x.current} onClick={x.onPick}>{x.label}</button></li>)}</ul>
        </>
      )}
    </nav>
  );
}

/** Phones (spec 16): the kinds become a chip row above the list. */
export function KindChips({ kinds, counts, current, onPick, extra = [] }: {
  kinds: readonly HomebrewKindDescriptor[]; counts: ReadonlyMap<string, number>; current: string | null; onPick: (kindId: string) => void;
  extra?: readonly ExtraCategory[];
}) {
  return (
    <div className="hb-chips" role="group" aria-label="Element kinds">
      {kinds.map((k) => (
        <button key={k.id} type="button" className="hb-chip" aria-pressed={k.id === current && !extra.some((x) => x.current)} onClick={() => onPick(k.id)}>{k.pluralLabel} ({counts.get(k.id) ?? 0})</button>
      ))}
      {extra.map((x) => <button key={x.label} type="button" className="hb-chip" aria-pressed={x.current} onClick={x.onPick}>{x.label}</button>)}
    </div>
  );
}

export interface ElementListProps {
  kind: HomebrewKindDescriptor | null;
  kinds: readonly HomebrewKindDescriptor[];
  /** Every element of the pack: the list shows the kind's, or every kind's while searching. */
  rows: readonly HomebrewElementSummary[];
  unsaved: UnsavedRow | null;
  /** The pack's other new elements that were never saved: their drafts stay on the device (spec 10.1, item IDpe8). */
  drafts: readonly HomebrewDraftSummary[];
  onOpenDraft: (draft: HomebrewDraftSummary) => void;
  openId: string | null;
  filter: ListFilter;
  filterRef: RefObject<HTMLInputElement>;
  onFilter: (filter: ListFilter) => void;
  onOpen: (row: HomebrewElementSummary) => void;
  onNew: () => void;
  onDuplicate: () => void;
  /** The chosen rows (spec 12.5): Ctrl/Shift+click, or the check boxes of Select mode. */
  selected: ReadonlySet<string>;
  onSelect: (ids: Set<string>) => void;
  /** Select mode: a check box per row (phones, spec 12.5; keyboard users on desktop). */
  selectMode: boolean;
  onSelectMode: (on: boolean) => void;
}

/**
 * The element grid (spec 13.2): NAME / EDITION, a mark for problems and one for unsaved changes, then the tool row (new,
 * duplicate, select, the filter box Ctrl+F focuses and "has problems"). The filter searches every kind of the pack by name,
 * kind, tag and text (spec 12.3); rows found in another kind say their kind.
 */
export function ElementList({ kind, kinds, rows, unsaved, drafts, onOpenDraft, openId, filter, filterRef, onFilter, onOpen, onNew, onDuplicate, selected, onSelect, selectMode, onSelectMode }: ElementListProps) {
  const [anchor, setAnchor] = useState<string | null>(null);
  const phone = usePhone();
  const searching = isSearching(filter);
  const shown = filterRows(rows, kind?.id ?? null, filter, kinds);
  const shownIds = shown.map((r) => r.id);
  const kindLabel = (r: HomebrewElementSummary) => kinds.find((k) => k.id === r.kind)?.label ?? r.elementType;
  const click = (e: MouseEvent, r: HomebrewElementSummary) => {
    const toggle = selectMode || e.ctrlKey || e.metaKey;
    if (toggle || e.shiftKey) {
      e.preventDefault();
      onSelect(nextSelection(selected, shownIds, r.id, anchor, { toggle, range: e.shiftKey }));
      setAnchor(r.id);
      return;
    }
    setAnchor(r.id);
    onSelect(new Set([r.id]));
    onOpen(r);
  };
  const inKind = rows.filter((r) => r.kind === kind?.id).length;
  const newDrafts = searching ? [] : drafts.filter((d) => d.kind === kind?.id);
  const newName = (d: HomebrewDraftSummary) => d.name.trim() || `New ${kind?.label.toLowerCase() ?? "element"}`;
  return (
    <div className="hb-elements">
      <Band title={searching ? "Found" : kind?.pluralLabel ?? "Elements"} />
      <div className="ap-scroll hb-element-scroll">
        {/* A table of buttons, not a keyboard grid: the chosen rows say so with aria-selected (aria-multiselectable is not
            allowed on a table, WB-26 axe); the header cells without a visible heading still name their column. */}
        <table className={selectMode ? "ap-grid hb-select-mode" : "ap-grid"} aria-label="Elements">
          <thead><tr>{selectMode && <th><span className="sr-only">Chosen</span></th>}<th>Name</th><th>Edition</th><th><span className="sr-only">Marks</span></th></tr></thead>
          <tbody>
            {unsaved && !searching && (
              <tr className="sel" aria-current="true">
                {selectMode && <td />}
                <td className="grow"><span className="ap-name hb-new-name">{unsaved.name}</span><RowDetail parts={[editionText(unsaved.edition), "Not saved yet"]} /></td>
                <td className="hide-narrow">{editionText(unsaved.edition)}</td>
                <td className="hide-narrow"><span className="hb-mark-draft" title="Not saved yet"><Icon name="pen" /></span></td>
              </tr>
            )}
            {newDrafts.map((d) => (
              <tr key={d.draftId}>
                {selectMode && <td />}
                <td className="grow"><button type="button" className="ap-name" onClick={() => onOpenDraft(d)}>{newName(d)}</button><RowDetail parts={[editionText(d.edition), "Not saved yet"]} /></td>
                <td className="hide-narrow">{editionText(d.edition)}</td>
                <td className="hide-narrow"><span className="hb-mark-draft" title="Not saved yet"><Icon name="pen" /></span></td>
              </tr>
            ))}
            {shown.map((r) => {
              const n = r.problems.errors + r.problems.warnings;
              const chosen = selected.has(r.id);
              return (
                <tr key={r.id} className={chosen || r.id === openId ? "sel" : ""} aria-current={r.id === openId || undefined} aria-selected={chosen}>
                  {selectMode && <td className="hb-pick"><input type="checkbox" className="ap-check" aria-label={`Choose ${r.name}`} checked={chosen}
                    onChange={() => { onSelect(nextSelection(selected, shownIds, r.id, null, { toggle: true, range: false })); setAnchor(r.id); }} /></td>}
                  <td className="grow">
                    <button type="button" className="ap-name" onClick={(e) => click(e, r)}>{r.name}</button>
                    {searching && <span className="hb-row-kind hide-narrow">{kindLabel(r)}</span>}
                    {r.override && <span className={`hb-row-kind hb-row-override hide-narrow${r.override.changed || r.override.missing ? " hb-warning" : ""}`}>{overrideNote(r.override)}</span>}
                    <RowDetail parts={[searching && kindLabel(r), r.override && overrideNote(r.override), editionText(r.edition), r.hasDraft && "Unsaved changes", n > 0 && `${n} ${n === 1 ? "problem" : "problems"}`]} />
                  </td>
                  <td className="hide-narrow">{editionText(r.edition)}</td>
                  <td className="hide-narrow">
                    {n > 0 && <span className={`hb-mark ${r.problems.errors ? "hb-error" : "hb-warning"}`} title={`${n} ${n === 1 ? "problem" : "problems"}`}>{n}</span>}
                    {r.hasDraft && <span className="hb-mark-draft" title="Unsaved changes"><Icon name="pen" /></span>}
                  </td>
                </tr>
              );
            })}
            {shown.length === 0 && newDrafts.length === 0 && !(unsaved && !searching) && (
              <tr className="ap-empty-row"><td colSpan={selectMode ? 4 : 3}>{!searching && inKind === 0 ? `No ${kind?.pluralLabel.toLowerCase() ?? "elements"} in this pack yet.` : "Nothing matches the filter."}</td></tr>
            )}
          </tbody>
        </table>
      </div>
      <div className="hb-tools">
        <span className="eq-cmds">
          <button type="button" className="hb-tool" disabled={!kind} title="New element (Ctrl+Shift+N)" onClick={onNew}>New</button>
          <button type="button" className="hb-tool" disabled={!openId} title="Duplicate (Ctrl+D)" onClick={onDuplicate}>Duplicate</button>
          <button type="button" className="hb-tool" aria-pressed={selectMode} title="Choose several elements (or Ctrl+click, Shift+click)" onClick={() => onSelectMode(!selectMode)}>Select</button>
        </span>
        <span className="eq-filter">
          <input ref={filterRef} type="text" aria-label="Filter elements" placeholder={phone ? "Filter names, tags, text..." : "filter names, tags, text..."} title={phone ? undefined : "Filter the elements (Ctrl+F)"} value={filter.text} onChange={(e) => onFilter({ ...filter, text: e.target.value })} />
          <button type="button" aria-label="Clear" onClick={() => onFilter({ ...filter, text: "" })}><Glyph name="x" /></button>
        </span>
        <button type="button" className="hb-chip hb-problems-only" aria-pressed={filter.problemsOnly} onClick={() => onFilter({ ...filter, problemsOnly: !filter.problemsOnly })}>Has problems</button>
      </div>
    </div>
  );
}
