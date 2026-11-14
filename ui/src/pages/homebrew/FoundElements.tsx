import type { KeyboardEvent } from "react";
import type { HomebrewKindDescriptor } from "../../api/contract";
import { RowDetail } from "../../components";
import type { FoundElement } from "./search";

/** One found element's key: the same id can stand in two packs. */
export const foundKey = (f: FoundElement): string => `${f.pack.id}\u0000${f.row.id}`;

export const kindLabelOf = (f: FoundElement, kinds: readonly HomebrewKindDescriptor[]): string =>
  kinds.find((k) => k.id === f.row.kind)?.label ?? f.row.elementType;

export interface FoundElementsProps {
  found: readonly FoundElement[];
  kinds: readonly HomebrewKindDescriptor[];
  selectedKey: string | null;
  onSelect: (f: FoundElement) => void;
  onOpen: (f: FoundElement) => void;
}

/**
 * The elements of every own pack that the HOMEBREW search box finds (H0eMM), in the pack grid's look: Name / Kind /
 * Pack. A click chooses one (the description pane says what it is and opens it), a double-click or Enter opens it in
 * its pack, the arrows move.
 */
export function FoundElements({ found, kinds, selectedKey, onSelect, onOpen }: FoundElementsProps) {
  const at = found.findIndex((f) => foundKey(f) === selectedKey);
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (e.key === "Enter" && at >= 0) { e.preventDefault(); onOpen(found[at]!); return; }
    if (!step || found.length === 0) return;
    e.preventDefault();
    onSelect(found[Math.min(found.length - 1, Math.max(0, at < 0 ? 0 : at + step))]!);
  };
  return (
    <div className="v4-grid hb-found-grid" role="grid" aria-label="Elements in your packs" tabIndex={0} onKeyDown={onKey} aria-rowcount={found.length + 1}>
      <div className="v4-grid-head" role="row">
        <span role="columnheader">Element</span><span role="columnheader">Kind</span><span role="columnheader">Pack</span>
      </div>
      <div className="v4-grid-body">
        {found.map((f) => {
          const key = foundKey(f);
          const kind = kindLabelOf(f, kinds);
          return (
            <div key={key} role="row" aria-selected={key === selectedKey} className={`v4-grid-row${key === selectedKey ? " sel" : ""}`}
              onClick={() => onSelect(f)} onDoubleClick={() => onOpen(f)}>
              <span role="gridcell" className="nm">{f.row.name}<RowDetail parts={[kind, f.pack.name]} /></span>
              <span role="gridcell">{kind}</span>
              <span role="gridcell">{f.pack.name}</span>
            </div>
          );
        })}
        {found.length === 0 && <div role="row"><p role="gridcell" className="v4-grid-empty">No elements match.</p></div>}
      </div>
    </div>
  );
}
