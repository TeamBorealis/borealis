import { forwardRef, type KeyboardEvent, type MouseEvent, useEffect, useId, useRef, useState } from "react";
import type { Ruleset } from "../../api/contract";
import { Flyout, Icon } from "../../components";
import { usePhone } from "../../lib/phone";
import { menuSections, type VersionLike } from "../../lib/versions";
import { Popover } from "./Popover";

/** Accessible name of a row's version button. */
export const versionsLabel = (n: number, rowName: string): string => `${n} versions of ${rowName}`;

/** The version column's control: the count bubble and a small down arrow (B-55 sec. 1.3). */
export const VersionButton = forwardRef<HTMLButtonElement, {
  count: number; rowName: string; open: boolean; onOpen: (anchor: HTMLElement) => void; className?: string;
}>(function VersionButton({ count, rowName, open, onOpen, className = "" }, ref) {
  const stop = (e: MouseEvent) => e.stopPropagation();
  return (
    <button ref={ref} type="button" className={`pick-ver-btn ${className}`.trim()} aria-haspopup="listbox" aria-expanded={open}
      aria-label={versionsLabel(count, rowName)} title={`${count} versions: choose a source`}
      onMouseDown={stop} onDoubleClick={stop} onClick={(e) => { e.stopPropagation(); onOpen(e.currentTarget); }}>
      <span className="pick-ver-count" aria-hidden="true">{count}</span>
      <Icon name="caret" />
    </button>
  );
});

interface ListProps {
  rowName: string;
  versions: VersionLike[];
  charEd: Ruleset;
  shownId: string;
  chosenId: string | null;
  onPick: (v: VersionLike) => void;
}

const optionLabel = (v: VersionLike, chosen: boolean): string =>
  [v.sourceName ?? v.name, chosen ? "chosen" : "", v.disabledReason ?? ""].filter(Boolean).join(", ");

/** The menu body: a listbox with one group per edition (sec. 1.4). Up/Down move, Enter or Space picks. */
function VersionList({ rowName, versions, charEd, shownId, chosenId, onPick }: ListProps) {
  const sections = menuSections(versions, charEd);
  const flat = sections.flatMap((s) => s.versions);
  const [active, setActive] = useState(() => Math.max(0, flat.findIndex((v) => v.id === shownId)));
  const ref = useRef<HTMLDivElement>(null);
  const base = useId();
  const optId = (i: number) => `${base}-o${i}`;
  useEffect(() => { ref.current?.focus(); }, []);
  const pick = (v: VersionLike) => { if (!v.disabledReason) onPick(v); };
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const last = flat.length - 1;
    const moves: Record<string, number> = { ArrowDown: Math.min(last, active + 1), ArrowUp: Math.max(0, active - 1), Home: 0, End: last };
    if (e.key in moves) { e.preventDefault(); setActive(moves[e.key]!); return; }
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); const v = flat[active]; if (v) pick(v); }
  };
  return (
    <div ref={ref} role="listbox" tabIndex={0} className="vm-list" aria-label={`Versions of ${rowName}`}
      aria-activedescendant={flat.length ? optId(active) : undefined} onKeyDown={onKey}>
      {sections.map((s) => (
        <div key={s.edition} role="group" aria-labelledby={`${base}-${s.edition}`} className="vm-group">
          <div id={`${base}-${s.edition}`} className="vm-head">{s.heading}</div>
          {s.versions.map((v) => {
            const idx = flat.indexOf(v);
            const chosen = v.id === chosenId;
            return (
              <div key={v.id} id={optId(idx)} role="option" aria-selected={v.id === shownId} aria-disabled={v.disabledReason ? true : undefined}
                aria-label={optionLabel(v, chosen)} className={`vm-opt ${idx === active ? "active" : ""} ${v.disabledReason ? "disabled" : ""}`}
                onMouseEnter={() => setActive(idx)} onClick={() => pick(v)}>
                <span className="vm-tick" aria-hidden="true">{v.id === shownId && <Icon name="check" />}</span>
                <span className="vm-src" aria-hidden="true">{v.sourceName ?? v.name}</span>
                {chosen && <span className="vm-sub" aria-hidden="true">chosen</span>}
                {v.disabledReason && <span className="vm-reason" aria-hidden="true">{v.disabledReason}</span>}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

/** The version menu: a popup under the row's source on desktop, a bottom sheet on phones (sec. 2). */
export function VersionMenu({ anchor, onClose, ...list }: ListProps & { anchor: HTMLElement; onClose: (restoreFocus: boolean) => void }) {
  const phone = usePhone();
  if (phone) {
    return (
      <Flyout title={list.rowName} sheet="auto" onClose={() => onClose(true)}>
        <VersionList {...list} />
      </Flyout>
    );
  }
  return (
    <Popover anchor={anchor} align="end" onClose={onClose} className="vm">
      <VersionList {...list} />
    </Popover>
  );
}
