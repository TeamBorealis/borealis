import { useState } from "react";
import type { FiveToolsSource } from "../../api/contract";
import { Check, Chip, SearchInput } from "../../components";
import { CheckBox, ComboBox, Expander, triState } from "../../components/controls";
import { editionLabel, type EditionFilter, setPicked, visibleGroups } from "./fiveTools";

const EDITIONS: { value: EditionFilter; label: string }[] = [
  { value: "all", label: "All editions" },
  { value: "dnd2014", label: "2014 rules" },
  { value: "dnd2024", label: "2024 rules" },
];

interface Props {
  sources: FiveToolsSource[];
  picked: string[];
  onPicked: (next: string[]) => void;
}

/** Scanned sources in Books / Adventures / Homebrew & other expanders, filtered by edition and name. */
export function FiveToolsSources({ sources, picked, onPicked }: Props) {
  const [edition, setEdition] = useState<EditionFilter>("all");
  const [query, setQuery] = useState("");
  const groups = visibleGroups(sources, edition, query);
  return (
    <div className="stack">
      <div className="v4-5et-filters">
        <SearchInput label="Filter sources" placeholder="Filter by name" value={query} onChange={setQuery} />
        <ComboBox aria-label="Edition" options={EDITIONS} value={edition} onChange={(e) => setEdition(e.target.value as EditionFilter)} />
      </div>
      {groups.length === 0 && <p className="muted">No source matches the filter.</p>}
      {groups.map((g) => {
        const abbrs = g.sources.map((s) => s.abbr);
        const state = triState(abbrs.filter((a) => picked.includes(a)).length, abbrs.length);
        return (
          <Expander key={g.id} title={`${g.title} (${g.sources.length})`}
            headerExtra={<label className="v4-5et-all"><CheckBox aria-label={`Select all ${g.title.toLowerCase()}`} checked={state.checked} mixed={state.mixed} onChange={(e) => onPicked(setPicked(picked, abbrs, e.target.checked))} /><span>All</span></label>}>
            <ul className="list" aria-label={`${g.title} sources`}>
              {g.sources.map((s) => (
                <li key={s.abbr}><div className="inv-row" style={{ padding: "var(--sp-2) var(--sp-3)" }}>
                  <Check label={`${s.name} (${s.abbr})`} description={Object.entries(s.counts).map(([k, n]) => `${n} ${k}`).join(", ")}
                    checked={picked.includes(s.abbr)} onChange={(e) => onPicked(setPicked(picked, [s.abbr], e.target.checked))} />
                  <span className="spacer" /><Chip tone="accent">{editionLabel(s.ruleset)}</Chip>
                </div></li>
              ))}
            </ul>
          </Expander>
        );
      })}
    </div>
  );
}
