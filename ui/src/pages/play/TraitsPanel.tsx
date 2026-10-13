import type { PlayDefenseEntry, PlayFeature, PlayState } from "../../api/contract";
import { EmptyState, SafeHtml } from "../../components";
import { Expander } from "../../components/controls";
import { Tag } from "../ai/Panes";

function DefenseGroup({ title, entries }: { title: string; entries: PlayDefenseEntry[] }) {
  if (entries.length === 0) return null;
  return (
    <div>
      <div className="v5-group">{title}</div>
      <ul className="v5-chips" aria-label={title}>
        {entries.map((e, i) => (
          <li key={`${e.name}-${i}`}>
            <Tag tone={e.active ? "ok" : ""} title={`${e.source}${e.active ? "" : e.whileToggle ? ` (while ${e.whileToggle})` : " (inactive)"}`}>
              {e.name}{e.active ? "" : e.whileToggle ? ` (while ${e.whileToggle})` : " (inactive)"}
            </Tag>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function DefensesCard({ play }: { play: PlayState }) {
  const d = play.defenses;
  const empty = !d.resistances.length && !d.immunities.length && !d.vulnerabilities.length && !d.conditionImmunities.length;
  return (
    <Expander title="Defenses">
      {empty ? <EmptyState title="No special defenses" /> : (
        <div className="v5-inline">
          <DefenseGroup title="Resistances" entries={d.resistances} />
          <DefenseGroup title="Immunities" entries={d.immunities} />
          <DefenseGroup title="Condition immunities" entries={d.conditionImmunities} />
          <DefenseGroup title="Vulnerabilities" entries={d.vulnerabilities} />
        </div>
      )}
    </Expander>
  );
}

export function SensesCard({ play }: { play: PlayState }) {
  const speeds = Object.entries(play.speeds).filter(([k, v]) => v > 0 || k === "walk");
  return (
    <Expander title="Senses and speeds">
      <div className="v5-inline">
        <ul className="v5-chips" aria-label="Speeds">{speeds.map(([k, v]) => <li key={k}><Tag>{k} {v} ft.</Tag></li>)}</ul>
        {play.senses.length === 0 ? <p className="v5-note" style={{ margin: 0 }}>No special senses.</p> : (
          <ul className="v5-chips" aria-label="Senses">{play.senses.map((s, i) => <li key={i}><Tag title={s.source}>{s.name} {s.range} ft.{s.note ? ` (${s.note})` : ""}</Tag></li>)}</ul>
        )}
      </div>
    </Expander>
  );
}

export function SituationalCard({ play }: { play: PlayState }) {
  if (!play.advantages.length && !play.notes.length && !play.initiativeAdvantage) return null;
  return (
    <Expander title="Situational">
      <ul className="v5-list" aria-label="Situational">
        {play.initiativeAdvantage && <li className="v5-row static"><span className="grow"><strong>Initiative</strong> Advantage on initiative rolls.</span><Tag tone="ok">Advantage</Tag></li>}
        {play.advantages.map((a, i) => (
          <li key={i} className="v5-row static">
            <span className="grow"><strong>{a.roll}</strong> {a.text}<span className="v5-note" style={{ display: "block" }}>{a.source}{a.whileToggle && !a.active ? ` · while ${a.whileToggle}` : ""}</span></span>
            <Tag tone={a.active ? "ok" : ""}>{a.active ? "Now" : "Conditional"}</Tag>
          </li>
        ))}
        {play.notes.map((n, i) => <li key={`n${i}`} className="v5-row static"><span className="grow">{n.text}<span className="v5-note" style={{ display: "block" }}>{n.source}</span></span></li>)}
      </ul>
    </Expander>
  );
}

export function FeaturesCard({ features, selectedId, onSelect }: { features: PlayFeature[]; selectedId: string | null; onSelect: (id: string) => void }) {
  return (
    <Expander title="Features and traits" headerExtra={<span>{features.length}</span>}>
      {features.length === 0 ? <EmptyState title="No features yet" /> : (
        <ul className="v5-list" aria-label="Features">
          {features.map((f) => (
            <li key={f.id} className={f.id === selectedId ? "is-selected" : ""}>
              <button type="button" className="v5-row" aria-current={f.id === selectedId} onClick={() => onSelect(f.id)}>
                <span className="grow">{f.name}</span>
                <span className="meta">{f.level ? `Level ${f.level}` : f.source}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </Expander>
  );
}

/** Description pane body for a feature. */
export function FeatureDetail({ feature }: { feature: PlayFeature | undefined }) {
  if (!feature) return <><h1 className="v5-desc-title">Traits</h1><p className="v5-lead">Select a feature to read its description.</p></>;
  return (
    <>
      <h1 className="v5-desc-title">{feature.name}</h1>
      <p className="v5-lead">{feature.source}{feature.level ? ` · level ${feature.level}` : ""}{feature.grantedBy ? ` · from ${feature.grantedBy}` : ""}</p>
      {feature.description ? <SafeHtml html={feature.description} /> : <p>{feature.sheetSummary ?? "No description."}</p>}
    </>
  );
}
