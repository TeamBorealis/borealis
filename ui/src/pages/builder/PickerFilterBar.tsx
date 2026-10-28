import { useState } from "react";
import { useNavigate } from "react-router";
import { Icon } from "../../components";
import { NO_FILTER, yearOf, type EditionFilter, type PickerFilter, type SourceEntry } from "../../lib/versions";
import { Popover } from "./Popover";

const SOURCES_ROUTE = "/content/sources";

const EDITIONS: { value: EditionFilter; label: string }[] = [
  { value: "all", label: "All" }, { value: "dnd2024", label: "2024" }, { value: "dnd2014", label: "2014" },
];

const entries = (n: number) => `${n} ${n === 1 ? "entry" : "entries"}`;

/** "12 entries", or "9 of 12 entries" while filters hide rows. */
export const countText = (shown: number, total: number): string => (shown === total ? entries(total) : `${shown} of ${entries(total)}`);

/** "All sources", or "3 of 4 sources" while some are unticked. */
export const sourcesText = (f: PickerFilter, all: readonly SourceEntry[]): string => {
  const kept = f.sources === null ? all.length : all.filter((s) => f.sources!.includes(s.id)).length;
  return kept === all.length ? "All sources" : `${kept} of ${all.length} sources`;
};

const editionLabel = (s: SourceEntry) => (s.edition === "any" ? "Any edition" : yearOf(s.edition));

interface EditionProps {
  value: EditionFilter;
  onChange: (e: EditionFilter) => void;
}

/** The Edition filter: All / 2024 / 2014 (sec. 1.5). */
export function EditionSegments({ value, onChange }: EditionProps) {
  return (
    <span className="pick-seg" role="group" aria-label="Edition">
      {EDITIONS.map((e) => (
        <button key={e.value} type="button" className="pick-seg-btn" aria-pressed={value === e.value} onClick={() => onChange(e.value)}>{e.label}</button>
      ))}
    </span>
  );
}

interface SourceProps {
  filter: PickerFilter;
  onChange: (f: PickerFilter) => void;
  sources: SourceEntry[];
}

/** The Source filter: "All sources" / "3 of 4 sources", opening a checklist of the list's sources and "Manage Sources..." (sec. 1.5). */
export function SourceFilterButton({ filter, onChange, sources }: SourceProps) {
  const navigate = useNavigate();
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const toggleSource = (id: string, on: boolean) => {
    const kept = filter.sources ?? sources.map((s) => s.id);
    const next = on ? [...kept.filter((k) => k !== id), id] : kept.filter((k) => k !== id);
    onChange({ ...filter, sources: sources.every((s) => next.includes(s.id)) ? null : next });
  };
  return (
    <>
      <button type="button" className="pick-fsrc" aria-haspopup="dialog" aria-expanded={!!anchor} aria-label={`Source: ${sourcesText(filter, sources)}`}
        onClick={(e) => setAnchor(anchor ? null : e.currentTarget)}>
        <span>{sourcesText(filter, sources)}</span><Icon name="caret" />
      </button>
      {anchor && (
        <Popover anchor={anchor} align="start" onClose={() => setAnchor(null)} className="pick-fmenu">
          <div role="dialog" aria-label="Sources">
            {sources.map((s) => (
              <label key={s.id} className="pick-fopt">
                <input type="checkbox" checked={filter.sources === null || filter.sources.includes(s.id)} onChange={(e) => toggleSource(s.id, e.target.checked)} />
                <span className="vm-src">{s.name}</span>
                <span className="vm-sub">{editionLabel(s)}</span>
              </label>
            ))}
            <button type="button" className="pick-fmanage" onClick={() => navigate(SOURCES_ROUTE)}>Manage Sources...</button>
          </div>
        </Popover>
      )}
    </>
  );
}

interface Props {
  /** The slot title, lower-cased into the placeholder ("Search species"). */
  title: string;
  filter: PickerFilter;
  onChange: (f: PickerFilter) => void;
  sources: SourceEntry[];
  shown: number;
  total: number;
}

/** B-55 sec. 1.5: search, Edition All/2024/2014, a Source checklist, the entry count and Clear, above a picker list. */
export function PickerFilterBar({ title, filter, onChange, sources, shown, total }: Props) {
  const placeholder = `Search ${title.toLowerCase()}`;
  return (
    <div className="pick-fbar" role="search" aria-label={`Filter ${title}`}>
      <input type="search" className="pick-fsearch" placeholder={placeholder} aria-label={placeholder} value={filter.text}
        onChange={(e) => onChange({ ...filter, text: e.target.value })} />
      <span className="pick-frow">
        <span className="pick-flabel" aria-hidden="true">Edition</span>
        <EditionSegments value={filter.edition} onChange={(edition) => onChange({ ...filter, edition })} />
        <span className="pick-flabel" aria-hidden="true">Source</span>
        <SourceFilterButton filter={filter} onChange={onChange} sources={sources} />
        <span className="pick-fcount" role="status">{countText(shown, total)}</span>
        <button type="button" className="pick-fclear" onClick={() => onChange(NO_FILTER)}>Clear</button>
      </span>
    </div>
  );
}
