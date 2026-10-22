import { useEffect, useMemo, useState, type KeyboardEvent, type MouseEvent } from "react";
import type { ContentSource } from "../../api/contract";
import { SearchInput } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useAsync } from "../../state/useAsync";
import { useShell } from "../../components/shell/ShellContext";
import { mdiAlertCircleOutline, mdiBookSearch, mdiContentSave, mdiFlask, mdiFolderOpen, mdiInformationOutline } from "@mdi/js";
import { CheckBox, triState } from "../../components/controls";
import { PaneShell } from "../builder/DescriptionPane";
import "../builder/builder.css";
import { loadDefaultSources, saveDefaultSources, useApplySourceRestrictions } from "./applySources";
import { SourceMenu, type MenuItem } from "./SourceMenu";
import "./content.css";

const Badge = ({ d, tip }: { d: string; tip: string }) => <span className="v4-flag" role="img" aria-label={tip} title={tip}><Svg d={d} /></span>;
const Svg = ({ d }: { d: string }) => <svg className="icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d={d} fill="currentColor" /></svg>;

/** START > SOURCES: the classic builder's source manager DataGrid (Name / Source group / Author / Version). With a character loaded the check column restricts its sources. */
export function SourcesPage() {
  const api = useApi();
  const { snapshot } = useCharacter();
  const applyRestrictions = useApplySourceRestrictions();
  const { openCompendium } = useShell();
  const sources = useAsync(() => api.listSources(), [api]);
  const packs = useAsync(() => api.listPacks(), [api]);
  const [query, setQuery] = useState("");
  const all = useMemo(() => sources.data ?? [], [sources.data]);
  const meta = useMemo(() => new Map((packs.data ?? []).map((p) => [p.id, p])), [packs.data]);
  const q = query.trim().toLowerCase();
  const rows = all.filter((s) => !q || [s.name, s.group, s.description].some((t) => t.toLowerCase().includes(q)));
  const saved = snapshot?.sourceRestrictions ?? null;
  const [draft, setDraft] = useState<string[] | null>(saved);
  useEffect(() => setDraft(saved), [saved]);
  const [described, setDescribed] = useState<string | null>(null);
  const allowed = useMemo(() => new Set(draft ?? all.map((s) => s.id)), [draft, all]);
  const normalise = (cur: Set<string>) => (cur.size === all.length ? null : [...cur]);
  const dirty = JSON.stringify(draft === null ? null : [...draft].sort()) !== JSON.stringify(saved === null ? null : [...saved].sort());

  const setMany = (ids: string[], on: boolean) => {
    const cur = new Set(allowed);
    for (const id of ids) { if (on) cur.add(id); else cur.delete(id); }
    setDraft(normalise(cur));
  };
  const toggle = (s: ContentSource) => setMany([s.id], !allowed.has(s.id));
  const sel = all.find((s) => s.id === described) ?? null;
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [anchor, setAnchor] = useState<string | null>(null);
  const order = useMemo(() => {
    const m = new Map<string, ContentSource[]>();
    for (const s of rows) m.set(s.group, [...(m.get(s.group) ?? []), s]);
    return [...m.values()].flat().map((s) => s.id);
  }, [rows]);
  /** ListView Extended: plain = single, Ctrl/Cmd toggles one, Shift selects anchor..target. */
  const pick = (id: string, e: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }) => {
    setDescribed(id);
    if (e.shiftKey && anchor && order.includes(anchor)) {
      const [a, b] = [order.indexOf(anchor), order.indexOf(id)].sort((x, y) => x - y);
      const range = order.slice(a, b! + 1);
      setSelected(new Set(e.ctrlKey || e.metaKey ? [...selected, ...range] : range));
      return;
    }
    setAnchor(id);
    if (e.ctrlKey || e.metaKey) setSelected(new Set(selected.has(id) ? [...selected].filter((x) => x !== id) : [...selected, id]));
    else setSelected(new Set([id]));
  };
  const onGridKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") { e.preventDefault(); setSelected(new Set(order)); return; }
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (!step || order.length === 0) return;
    e.preventDefault();
    const at = described ? order.indexOf(described) : -1;
    const next = order[Math.min(order.length - 1, Math.max(0, at < 0 ? 0 : at + step))]!;
    if (e.shiftKey) { setDescribed(next); const base = anchor ?? described ?? next; if (!anchor) setAnchor(base); const [a, b] = [order.indexOf(base), order.indexOf(next)].sort((x, y) => x - y); setSelected(new Set(order.slice(a, b! + 1))); }
    else if (e.ctrlKey || e.metaKey) setDescribed(next);
    else pick(next, e);
  };
  const onRowContext = (e: MouseEvent, s: ContentSource) => {
    e.preventDefault();
    setDescribed(s.id);
    if (!selected.has(s.id)) { setSelected(new Set([s.id])); setAnchor(s.id); }
    setMenu({ x: e.clientX, y: e.clientY, src: s });
  };
  const toggleSelected = () => {
    const cur = new Set(allowed);
    for (const id of selected) { if (cur.has(id)) cur.delete(id); else cur.add(id); }
    setDraft(normalise(cur));
  };
  const grouped = useMemo(() => {
    const m = new Map<string, ContentSource[]>();
    for (const s of rows) m.set(s.group, [...(m.get(s.group) ?? []), s]);
    return [...m.entries()];
  }, [rows]);
  const stored = loadDefaultSources();
  const [menu, setMenu] = useState<{ x: number; y: number; src: ContentSource } | null>(null);
  const menuItems = (src: ContentSource): MenuItem[] => {
    const author = meta.get(src.packId)?.attribution ?? "";
    const sameAuthor = author ? all.filter((a) => (meta.get(a.packId)?.attribution ?? "") === author) : [src];
    return [
      { label: allowed.has(src.id) ? "Restrict this source" : "Allow this source", run: () => toggle(src) },
      { label: "Toggle all selected sources", disabled: selected.size === 0, run: toggleSelected },
      { label: "Toggle all sources by the same author", run: () => setMany(sameAuthor.map((a) => a.id), !allowed.has(src.id)) },
      { label: "View in compendium", run: () => openCompendium({ source: src.id }) },
    ];
  };
  return (
    <div className="v4-sources-wrap">
    <div className="v4-sources">
      <div className="v4-grid-tools">
        <SearchInput value={query} onChange={setQuery} label="Search sources" placeholder="Search sources" />
        <span className="v4-grid-cmds" role="toolbar" aria-label="Source commands">
          <button type="button" className="v4-grid-cmd" aria-label="Save as default" title="Save the allowed sources as the default" onClick={() => saveDefaultSources([...allowed])}><Svg d={mdiContentSave} /></button>
          <button type="button" className="v4-grid-cmd" aria-label="Load default" title="Load the saved default sources" disabled={!stored} onClick={() => stored && setDraft(normalise(new Set(stored.filter((id) => all.some((a) => a.id === id)))))}><Svg d={mdiFolderOpen} /></button>
          <button type="button" className="v4-apply" disabled={!snapshot || !dirty} title={snapshot ? "Restrict the loaded character to the allowed sources" : "Load a character first"} onClick={() => void applyRestrictions(draft)}>Apply to character</button>
          <button type="button" className="v4-grid-cmd" aria-label="Restrict all playtest material" title="Restrict all playtest material" onClick={() => setMany(all.filter((a) => a.playtest).map((a) => a.id), false)}><Svg d={mdiFlask} /></button>
          <button type="button" className="v4-grid-cmd" aria-label="View selected source in compendium" title="View selected source in compendium" disabled={!sel} onClick={() => sel && openCompendium({ source: sel.id })}><Svg d={mdiBookSearch} /></button>
        </span>
      </div>
      <div className="v4-grid" role="grid" aria-label="Sources" aria-multiselectable="true" tabIndex={0} onKeyDown={onGridKey} aria-rowcount={rows.length + 1}>
        <div className="v4-grid-head" role="row">
          <span role="columnheader" className="chk" />
          <span role="columnheader">Name</span><span role="columnheader">Source group</span><span role="columnheader">Author</span><span role="columnheader">Version</span>
        </div>
        <div className="v4-grid-body">
          {grouped.map(([group, members]) => {
            const t = triState(members.filter((m) => allowed.has(m.id)).length, members.length);
            return (
              <div key={group} role="rowgroup" aria-label={group}>
                <div role="row" className="v4-grid-row v4-grid-group">
                  <span role="gridcell" className="chk"><CheckBox aria-label={`Allow every source in ${group}`} checked={t.checked} mixed={t.mixed} onChange={() => setMany(members.map((m) => m.id), !t.checked)} /></span>
                  <span role="gridcell" className="gn">{group}</span><span role="gridcell" /><span role="gridcell" /><span role="gridcell" />
                </div>
                {members.map((s) => {
                  const p = meta.get(s.packId);
                  return (
                    <div key={s.id} role="row" aria-selected={selected.has(s.id)} onContextMenu={(e) => onRowContext(e, s)} className={`v4-grid-row${selected.has(s.id) ? " sel" : ""}`} onClick={(e) => pick(s.id, e)} onDoubleClick={() => openCompendium({ source: s.id })}>
                      <span role="gridcell" className="chk"><CheckBox aria-label={`Allow ${s.name}`} checked={allowed.has(s.id)} onChange={() => toggle(s)} /></span>
                      <span role="gridcell" className="nm">{s.name}<span className="v4-flags">{s.playtest && <Badge d={mdiFlask} tip="Playtest material" />}{s.incomplete && <Badge d={mdiAlertCircleOutline} tip="Incomplete source" />}{s.info && <Badge d={mdiInformationOutline} tip={s.info} />}</span></span>
                      <span role="gridcell">{s.group}</span>
                      <span role="gridcell">{p?.attribution ?? ""}</span>
                      <span role="gridcell">{p?.version ?? ""}</span>
                    </div>
                  );
                })}
              </div>
            );
          })}
          {sources.data && rows.length === 0 && <p className="v4-grid-empty">No sources match.</p>}
        </div>
      </div>
    </div>
    {menu && <SourceMenu x={menu.x} y={menu.y} items={menuItems(menu.src)} onClose={() => setMenu(null)} />}
    <div className="v4-src-desc">
      {sel ? (
        <PaneShell title={sel.name} intro={sel.group} imageIcon={false} readText={sel.description}>
          <p>{sel.description}</p>
          <p className="v4-src-facts">{sel.elementCount} elements{meta.get(sel.packId)?.attribution ? ` · ${meta.get(sel.packId)?.attribution}` : ""}{meta.get(sel.packId)?.version ? ` · v${meta.get(sel.packId)?.version}` : ""}{sel.playtest ? " · playtest" : ""}{sel.incomplete ? " · incomplete" : ""}</p>
          {sel.info && <p className="v4-src-info">{sel.info}</p>}
        </PaneShell>
      ) : <aside className="desc-pane" aria-label="Description" />}
    </div>
    </div>
  );
}
