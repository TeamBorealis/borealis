import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import type { ElementSummary, Ruleset } from "../../api/contract";
import { Alert, Button, ComboBox, Expander, SafeHtml, SearchInput } from "../../components";
import { Flyout } from "../../components/controls/Flyout";
import { useShell } from "../../components/shell/ShellContext";
import { useApi } from "../../state/ApiContext";
import { usePlatform } from "../../state/platform";
import { useAsync, useDebounced } from "../../state/useAsync";
import "./compendium.css";

const PAGE = 40;

function Detail({ id, onSource, onNavigate }: { id: string; onSource: (s: string) => void; onNavigate: () => void }) {
  const api = useApi();
  const { aiEnabled } = usePlatform();
  const d = useAsync(() => api.getElement({ elementId: id }), [api, id]);
  if (d.error) return <Alert tone="danger">{d.error}</Alert>;
  if (!d.data) return <p className="empty">Loading...</p>;
  const e = d.data;
  const facts = e.facts.length > 0 && <dl className="kv">{e.facts.map((f) => <div key={f.label}><dt>{f.label}</dt><dd>{f.value}</dd></div>)}</dl>;
  /** A monster reads as a stat block: type, AC, HP and CR first, then its traits, actions and the rest. */
  const statBlock = e.type === "monster";
  return (
    <article aria-label={e.name}>
      <h2>{e.name}</h2>
      <p className="flavour">{[e.type, ...e.rulesets.map((r) => (r === "dnd2024" ? "2024 rules" : "2014 rules"))].join(" - ")}</p>
      {e.prerequisites && <p className="flavour">Prerequisite: {e.prerequisites}</p>}
      {statBlock && facts}
      <div className="body"><SafeHtml html={e.descriptionHtml} /></div>
      {!statBlock && facts}
      <h4>Source</h4>
      <a href={`#source-${e.source}`} onClick={(ev) => { ev.preventDefault(); onSource(e.source); }}>{e.sourceName}</a>
      {aiEnabled && (
        <>
          <h4>Assistant</h4>
          <Link to={`/ai?mode=rules_qa&prompt=${encodeURIComponent(`Explain ${e.name} and how it interacts with my build.`)}`} onClick={onNavigate}>Ask the AI about this</Link>
        </>
      )}
    </article>
  );
}

/** Classic builder element browser: expander lists on the left, description pane on the right. */
export function CompendiumBrowser({ initialQuery = "", initialSource = "", initialType = "", initialSelected = null, onNavigate }: { initialQuery?: string; initialSource?: string; initialType?: string; initialSelected?: string | null; onNavigate: () => void }) {
  const api = useApi();
  const [query, setQuery] = useState(initialQuery);
  const q = useDebounced(query, 150);
  const [type, setType] = useState(initialType);
  const [source, setSource] = useState(initialSource);
  const [ruleset, setRuleset] = useState<Ruleset | "any">("any");
  const [selected, setSelected] = useState<string | null>(initialSelected);
  const [limit, setLimit] = useState(PAGE);
  const facets = useAsync(() => api.compendiumFacets(), [api]);
  const results = useAsync(() => api.searchCompendium({ query: { query: q, types: type ? [type as ElementSummary["type"]] : undefined, sources: source ? [source] : undefined, ruleset, limit, offset: 0 } }), [api, q, type, source, ruleset, limit]);
  const items = results.data?.items ?? [];
  const byType = new Map<string, ElementSummary[]>();
  for (const i of items) byType.set(i.type, [...(byType.get(i.type) ?? []), i]);
  const typeLabel = (t: string) => facets.data?.types.find((x) => x.id === t)?.label ?? t;
  const reset = <T,>(set: (v: T) => void) => (v: T) => { set(v); setLimit(PAGE); };
  return (
    <div className="v4-comp" data-view={selected ? "detail" : "list"}>
      <div className="v4-comp-left">
        <div className="v4-comp-filters">
          <SearchInput value={query} onChange={reset(setQuery)} label="Search the compendium" placeholder="Search spells, items, classes..." autoFocus />
          <ComboBox aria-label="Filter by type" value={type} onChange={(e) => reset(setType)(e.target.value)}
            options={[{ value: "", label: "All types" }, ...(facets.data?.types ?? []).map((t) => ({ value: t.id, label: `${t.label} (${t.count})` }))]} />
          <ComboBox aria-label="Filter by source" value={source} onChange={(e) => reset(setSource)(e.target.value)}
            options={[{ value: "", label: "All sources" }, ...(facets.data?.sources ?? []).map((s) => ({ value: s.id, label: s.label }))]} />
          <ComboBox aria-label="Filter by ruleset" value={ruleset} onChange={(e) => reset(setRuleset)(e.target.value as Ruleset | "any")}
            options={[{ value: "any", label: "Both rulesets" }, { value: "dnd2024", label: "2024" }, { value: "dnd2014", label: "2014" }]} />
          <span className="v4-comp-count" role="status">{results.data ? `${results.data.total} results` : ""}</span>
        </div>
        {results.error && <Alert tone="danger">{results.error}</Alert>}
        {[...byType.entries()].map(([t, list]) => (
          <Expander key={t} title={typeLabel(t)} headerExtra={<span className="v4-comp-count">{list.length}</span>}>
            <ul className="v4-el" aria-label={`Compendium results: ${typeLabel(t)}`}>
              {list.map((i) => (
                <li key={i.id}>
                  <button type="button" aria-selected={selected === i.id} title={i.shortDescription} onClick={() => setSelected(i.id)}>
                    <span className="n">{i.name}</span><span className="s">{i.sourceName}</span>
                  </button>
                </li>
              ))}
            </ul>
          </Expander>
        ))}
        {results.data && items.length === 0 && <p className="v4-comp-count">No entries match.</p>}
        {results.data && results.data.total > items.length && <div className="v4-comp-more"><Button onClick={() => setLimit((l) => l + PAGE)}>Show more</Button></div>}
      </div>
      <aside className="v4-desc" aria-label="Element description">
        <Button className="back" variant="ghost" icon="back" onClick={() => setSelected(null)}>Back to results</Button>
        {selected ? <Detail id={selected} onSource={reset(setSource)} onNavigate={onNavigate} /> : <p className="empty">Select an entry to read it.</p>}
      </aside>
    </div>
  );
}

/** Bottom flyout (classic builder: full width, top margin 98, 2px top border). Opened by the title-bar search (Enter), F1 or View > Compendium. */
export function CompendiumFlyout() {
  const { compendium, closeCompendium } = useShell();
  const counter = useRef(0);
  // Each new request (another Enter in the search box) restarts the browser with the new query.
  const rev = useMemo(() => ++counter.current, [compendium]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!compendium) return null;
  return (
    <Flyout title="Compendium" position="bottom" onClose={closeCompendium}>
      <CompendiumBrowser key={rev} initialQuery={compendium.query} initialSource={compendium.source} initialType={compendium.type} initialSelected={compendium.elementId ?? null} onNavigate={closeCompendium} />
    </Flyout>
  );
}

/** Legacy `/compendium?q=&source=&type=` links: open the flyout and fall back to the start page. */
export function CompendiumRoute() {
  const [params] = useSearchParams();
  const { openCompendium } = useShell();
  const nav = useNavigate();
  useEffect(() => {
    openCompendium({ query: params.get("q") ?? undefined, source: params.get("source") ?? undefined, type: params.get("type") ?? undefined });
    const idx = (window.history.state as { idx?: number } | null)?.idx ?? 0;
    if (idx > 0) nav(-1); else nav("/characters", { replace: true });
  }, [openCompendium, params, nav]);
  return null;
}
