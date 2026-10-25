import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import type { ElementSummary, Ruleset, SearchIndexStatus } from "../../api/contract";
import { Alert, Button, ComboBox, EditionBadge, Expander, SafeHtml, SearchInput } from "../../components";
import { Flyout } from "../../components/controls/Flyout";
import { TabControl } from "../../components/controls/TabControl";
import { useShell } from "../../components/shell/ShellContext";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { usePlatform } from "../../state/platform";
import { useSettings } from "../../state/SettingsContext";
import { useAsync, useDebounced } from "../../state/useAsync";
import { RULES_GROUP, SCOPES, badgeOf, footerText, groupRows, highlightParts, initialEdition, rowOfElement, rowOfHit, showSemanticHint, type ResultRow } from "./searchModel";
import { useSemanticStatus } from "../../state/semantic";
import "./compendium.css";

const PAGE = 40;
/** Results update as you type (spec section 11). */
const DEBOUNCE_MS = 120;
type Scope = (typeof SCOPES)[number]["id"];

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

/** A rules-text section (Rules scope): section text, then its source and the CC-BY attribution it requires. */
function RulesDetail({ docKey }: { docKey: string }) {
  const api = useApi();
  const d = useAsync(() => api.getSearchPassage({ docKey }), [api, docKey]);
  if (d.error) return <Alert tone="danger">{d.error}</Alert>;
  if (!d.data) return <p className="empty">Loading...</p>;
  const p = d.data;
  return (
    <article aria-label={p.title}>
      <h2>{p.title}</h2>
      <p className="flavour">{[p.path, p.ruleset === "dnd2024" ? "2024 rules" : "2014 rules"].join(" - ")}</p>
      <div className="body"><SafeHtml html={p.html} /></div>
      <h4>Source</h4>
      <p className="v4-desc-src">{p.page ? `${p.source}, page ${p.page}` : p.source}</p>
      <p className="v4-desc-attr">{p.attribution}</p>
    </article>
  );
}

function Snippet({ row }: { row: ResultRow }) {
  if (!row.snippet) return null;
  return <span className="snip">{highlightParts(row.snippet, row.highlights).map((p, i) => (p.hit ? <mark key={i}>{p.text}</mark> : <span key={i}>{p.text}</span>))}</span>;
}

/**
 * Index state for the footer, kept current by `search://index-status`. `builds` counts the times the
 * index became ready after the flyout saw it not ready (the first run on a device), so a query typed
 * meanwhile, answered by the plain name scan, is searched again.
 */
function useIndexStatus(): { status: SearchIndexStatus | null; builds: number } {
  const api = useApi();
  const [status, setStatus] = useState<SearchIndexStatus | null>(null);
  const [builds, setBuilds] = useState(0);
  const last = useRef<SearchIndexStatus["state"] | null>(null);
  useEffect(() => {
    let live = true;
    const seen = (s: SearchIndexStatus) => {
      if (!live) return;
      if (s.state === "ready" && last.current !== null && last.current !== "ready") setBuilds((n) => n + 1);
      last.current = s.state;
      setStatus(s);
    };
    api.searchIndexStatus().then(seen, () => undefined);
    const off = api.subscribe("search:index-status", seen);
    return () => { live = false; off(); };
  }, [api]);
  return { status, builds };
}

/** Classic builder element browser: expander lists on the left, description pane on the right. Typing searches the keyword indexes. */
export function CompendiumBrowser({ initialQuery = "", initialSource = "", initialType = "", initialSelected = null, onNavigate }: { initialQuery?: string; initialSource?: string; initialType?: string; initialSelected?: string | null; onNavigate: () => void }) {
  const api = useApi();
  const { settings } = useSettings();
  const { snapshot } = useCharacter();
  const [query, setQuery] = useState(initialQuery);
  const q = useDebounced(query, DEBOUNCE_MS).trim();
  const [scope, setScope] = useState<Scope>("all");
  const [type, setType] = useState(initialType);
  const [source, setSource] = useState(initialSource);
  const [ruleset, setRuleset] = useState<Ruleset | "any">(() => initialEdition(settings.search?.defaultEdition, snapshot?.ruleset ?? null));
  const [selected, setSelected] = useState<string | null>(initialSelected);
  const [limit, setLimit] = useState(PAGE);
  const { status, builds } = useIndexStatus();
  const { status: semantic } = useSemanticStatus();
  const { openBorealisSettings } = useShell();
  const facets = useAsync(() => api.compendiumFacets(), [api]);
  const rules = scope === "rules";
  const types = type && !rules ? [type as ElementSummary["type"]] : undefined;
  const sources = source && !rules ? [source] : undefined;
  const results = useAsync(async () => {
    if (q) {
      const page = await api.search({ request: { scope, query: q, edition: ruleset, types, sources, limit, offset: 0 } });
      return { total: page.total, rows: page.items.map(rowOfHit) };
    }
    if (rules) return { total: 0, rows: [] };
    const page = await api.searchCompendium({ query: { query: "", types, sources, ruleset, limit, offset: 0 } });
    return { total: page.total, rows: page.items.map(rowOfElement) };
  }, [api, q, scope, type, source, ruleset, limit, builds]); // eslint-disable-line react-hooks/exhaustive-deps
  const rows = results.data?.rows ?? [];
  const preferred: Ruleset = snapshot?.ruleset ?? settings.defaultRuleset;
  const typeLabel = (t: string) => (t === "rules" ? RULES_GROUP : facets.data?.types.find((x) => x.id === t)?.label ?? t);
  const reset = <T,>(set: (v: T) => void) => (v: T) => { set(v); setLimit(PAGE); };
  return (
    <div className="v4-comp" data-view={selected ? "detail" : "list"}>
      <div className="v4-comp-left">
        <TabControl tabs={SCOPES} active={scope} onChange={(id) => reset(setScope)(id as Scope)} label="Search scope" idPrefix="search-scope" variant="sub" />
        <div className="v4-comp-filters">
          <SearchInput value={query} onChange={reset(setQuery)} label="Search the compendium" placeholder="Search spells, items, classes..." autoFocus />
          {!rules && (
            <>
              <ComboBox aria-label="Filter by type" value={type} onChange={(e) => reset(setType)(e.target.value)}
                options={[{ value: "", label: "All types" }, ...(facets.data?.types ?? []).map((t) => ({ value: t.id, label: `${t.label} (${t.count})` }))]} />
              <ComboBox aria-label="Filter by source" value={source} onChange={(e) => reset(setSource)(e.target.value)}
                options={[{ value: "", label: "All sources" }, ...(facets.data?.sources ?? []).map((s) => ({ value: s.id, label: s.label }))]} />
            </>
          )}
          <ComboBox aria-label="Filter by ruleset" value={ruleset} onChange={(e) => reset(setRuleset)(e.target.value as Ruleset | "any")}
            options={[{ value: "any", label: "Both rulesets" }, { value: "dnd2024", label: "2024" }, { value: "dnd2014", label: "2014" }]} />
          <span className="v4-comp-count" role="status">{results.data ? `${results.data.total} results` : ""}</span>
        </div>
        {results.error && <Alert tone="danger">{results.error}</Alert>}
        {groupRows(rows).map(([t, list]) => (
          <Expander key={t} title={typeLabel(t)} headerExtra={<span className="v4-comp-count">{list.length}</span>}>
            <ul className="v4-el" aria-label={`Compendium results: ${typeLabel(t)}`}>
              {list.map((r) => {
                const badge = badgeOf(r, preferred);
                return (
                  <li key={r.key}>
                    <button type="button" aria-selected={selected === r.open} title={r.title} onClick={() => setSelected(r.open)}>
                      {r.path && <span className="p">{r.path.split(" > ").slice(0, -1).join(" > ")}</span>}
                      <span className="n">{r.name}</span>
                      <EditionBadge marks={{ badge, converted: false, notes: [] }} />
                      <span className="s">{r.source}</span>
                      <Snippet row={r} />
                    </button>
                  </li>
                );
              })}
            </ul>
          </Expander>
        ))}
        {results.data && rows.length === 0 && <p className="v4-comp-count">{rules && !q ? "Type to search the rules." : "No entries match."}</p>}
        {results.data && results.data.total > rows.length && <div className="v4-comp-more"><Button onClick={() => setLimit((l) => l + PAGE)}>Show more</Button></div>}
        {q && results.data && showSemanticHint(semantic) && (
          <p className="v4-comp-hint">Better results for descriptive questions: <button type="button" className="v4-comp-hint-link" onClick={() => openBorealisSettings("search")}>turn on semantic search</button></p>
        )}
        <p className="v4-comp-foot" role="note" aria-label="Search mode">{footerText(status, semantic?.active ?? false, semantic?.indexing ?? null)}</p>
      </div>
      <aside className="v4-desc" aria-label="Element description">
        <Button className="back" variant="ghost" icon="back" onClick={() => setSelected(null)}>Back to results</Button>
        {!selected && <p className="empty">Select an entry to read it.</p>}
        {selected?.startsWith("rules:") && <RulesDetail docKey={selected} />}
        {selected && !selected.startsWith("rules:") && <Detail id={selected} onSource={reset(setSource)} onNavigate={onNavigate} />}
      </aside>
    </div>
  );
}

/** Bottom flyout (classic builder: full width, top margin 98, 2px top border). Opened by the title-bar search (Enter), F1, View > Compendium, or More > Search on phones (full screen there). */
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

/** Legacy `/compendium?q=&source=&type=` links and the phone's More > Search (`/search`): open the flyout and fall back to the previous page. */
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
