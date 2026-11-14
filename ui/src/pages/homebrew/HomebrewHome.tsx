import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { mdiDownload, mdiPlus, mdiSend, mdiUpload } from "@mdi/js";
import type { ContentPack, HomebrewElementSummary, HomebrewProblemCounts } from "../../api/contract";
import { Button, RowDetail, SearchInput, useToast } from "../../components";
import { filesToInputs } from "../../lib/files";
import { useApi } from "../../state/ApiContext";
import { startImportPreview } from "../../state/importPreview";
import { useAsync } from "../../state/useAsync";
import { PaneShell } from "../builder/DescriptionPane";
import { openSendToDevice } from "../transfer/openTransfer";
import { ExportPackDialog } from "./ExportPackDialog";
import { FoundElements, foundKey, kindLabelOf } from "./FoundElements";
import { NewPackFlyout } from "./NewPackFlyout";
import { editionLabel, lastPackId, rememberPack } from "./packs";
import { searchElements, type FoundElement } from "./search";
import "../builder/builder.css";
import "../content/content.css";
import "./homebrew.css";

const Svg = ({ d }: { d: string }) => <svg className="icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d={d} fill="currentColor" /></svg>;

const NO_COUNTS: HomebrewProblemCounts = { errors: 0, warnings: 0 };

/** Every pack's element rows: the pack list counts their problems (spec 8.3) and its search box searches them (H0eMM). */
async function elementLists(api: ReturnType<typeof useApi>, packs: readonly ContentPack[]): Promise<Map<string, HomebrewElementSummary[]>> {
  const lists = await Promise.all(packs.map((p) => api.homebrewListElements({ packId: p.id }).catch(() => [])));
  return new Map(packs.map((p, i) => [p.id, lists[i]!]));
}

const problemsOf = (rows: readonly HomebrewElementSummary[]): HomebrewProblemCounts =>
  rows.reduce((c, e) => ({ errors: c.errors + e.problems.errors, warnings: c.warnings + e.problems.warnings }), NO_COUNTS);

/**
 * START > HOMEBREW (spec 13.1): the user's own packs in the Sources manager's look: search box, command buttons
 * (new pack, import, export, send), a Name / Edition / Elements / Version grid, the description pane with the pack's facts.
 * The search box also finds the elements of every pack (H0eMM), listed under the packs; each opens in its pack.
 */
export function HomebrewHome() {
  const api = useApi();
  const nav = useNavigate();
  const toast = useToast();
  const [params, setParams] = useSearchParams();
  const packs = useAsync(() => api.listPacks(), [api]);
  useEffect(() => api.subscribe("content:reloaded", packs.reload), [api, packs.reload]);
  const own = useMemo(() => (packs.data ?? []).filter((p) => p.kind === "homebrew").sort((a, b) => a.name.localeCompare(b.name)), [packs.data]);
  const lists = useAsync(() => elementLists(api, own), [api, own]);
  const kinds = useAsync(() => api.homebrewKinds().catch(() => []), [api]);
  const drafts = useAsync(() => api.homebrewListDrafts().catch(() => []), [api, own]);
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [exporting, setExporting] = useState<ContentPack | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const q = query.trim().toLowerCase();
  const rows = own.filter((p) => !q || p.name.toLowerCase().includes(q) || p.attribution.toLowerCase().includes(q));
  const found = useMemo(() => searchElements(own, lists.data ?? new Map(), query, kinds.data ?? []), [own, lists.data, query, kinds.data]);
  const [foundId, setFoundId] = useState<string | null>(null);
  const chosen = found.find((f) => foundKey(f) === foundId) ?? null;
  const selected = chosen ? null : own.find((p) => p.id === selectedId) ?? null;
  const selectPack = (id: string) => { setFoundId(null); setSelectedId(id); };
  const selectFound = (f: FoundElement) => { setSelectedId(null); setFoundId(foundKey(f)); };
  const openFound = (f: FoundElement) => { rememberPack(f.pack.id); nav(`/homebrew/${encodeURIComponent(f.pack.id)}?element=${encodeURIComponent(f.row.id)}`); };
  const openPack = (p: ContentPack, suffix = "") => { rememberPack(p.id); nav(`/homebrew/${encodeURIComponent(p.id)}${suffix}`); };

  // File > New homebrew element...: into the last pack worked on (or the only one); with no pack, a pack first.
  const wantsNew = params.get("new") === "1";
  useEffect(() => {
    if (!wantsNew || !packs.data) return;
    const target = own.find((p) => p.id === lastPackId()) ?? own[0];
    if (target) { openPack(target, "?new=1"); return; }
    setCreating(true);
    setParams({}, { replace: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wantsNew, packs.data]);

  const onCreated = (pack: ContentPack) => { setCreating(false); packs.reload(); openPack(pack, wantsNew ? "?new=1" : ""); };
  const onImport = async (list: FileList | null) => {
    const files = Array.from(list ?? []);
    if (fileRef.current) fileRef.current.value = "";
    if (files.length) await startImportPreview(api, { files: await filesToInputs(files) }, (m) => toast.push(m, "error"));
  };
  const onGridKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (e.key === "Enter" && selected) { e.preventDefault(); openPack(selected); return; }
    if (!step || rows.length === 0) return;
    e.preventDefault();
    const at = rows.findIndex((p) => p.id === selectedId);
    selectPack(rows[Math.min(rows.length - 1, Math.max(0, at < 0 ? 0 : at + step))]!.id);
  };
  const problemMark = (p: ContentPack) => {
    const c = problemsOf(lists.data?.get(p.id) ?? []);
    const n = c.errors + c.warnings;
    return n > 0 ? <span className={`hb-mark ${c.errors ? "hb-error" : "hb-warning"}`} title={`${n} ${n === 1 ? "problem" : "problems"}`}>{n}</span> : null;
  };
  const packDrafts = (drafts.data ?? []).filter((d) => d.packId === selected?.id).length;
  return (
    <div className="v4-sources-wrap hb-home">
      <div className="v4-sources">
        <h1 className="sr-only">Homebrew</h1>
        <div className="v4-grid-tools">
          <SearchInput value={query} onChange={setQuery} label="Search packs and elements" placeholder="Search packs and elements" />
          <span className="v4-grid-cmds" role="toolbar" aria-label="Pack commands">
            <button type="button" className="v4-grid-cmd" aria-label="New pack" title="New pack" onClick={() => setCreating(true)}><Svg d={mdiPlus} /></button>
            <button type="button" className="v4-grid-cmd" aria-label="Import pack" title="Import a pack file" onClick={() => fileRef.current?.click()}><Svg d={mdiUpload} /></button>
            <button type="button" className="v4-grid-cmd" aria-label="Export pack" title="Export the selected pack to a file" disabled={!selected} onClick={() => setExporting(selected)}><Svg d={mdiDownload} /></button>
            <button type="button" className="v4-grid-cmd" aria-label="Send to device" title="Send to another device" onClick={() => openSendToDevice()}><Svg d={mdiSend} /></button>
          </span>
          <input ref={fileRef} type="file" hidden accept=".zip,application/zip" aria-label="Choose a pack file" onChange={(e) => void onImport(e.target.files)} />
        </div>
        <div className="v4-grid hb-pack-grid" role="grid" aria-label="Homebrew packs" tabIndex={0} onKeyDown={onGridKey} aria-rowcount={rows.length + 1}>
          <div className="v4-grid-head" role="row">
            <span role="columnheader">Name</span><span role="columnheader">Edition</span><span role="columnheader">Elements</span><span role="columnheader">Version</span>
          </div>
          <div className="v4-grid-body">
            {rows.map((p) => (
              <div key={p.id} role="row" aria-selected={p.id === selectedId} className={`v4-grid-row${p.id === selectedId ? " sel" : ""}`}
                onClick={() => selectPack(p.id)} onDoubleClick={() => openPack(p)}>
                <span role="gridcell" className="nm">{p.name}{problemMark(p)}<RowDetail parts={[editionLabel(p.rulesets), `${p.elementCount} ${p.elementCount === 1 ? "element" : "elements"}`, `v${p.version}`]} /></span>
                <span role="gridcell">{editionLabel(p.rulesets)}</span>
                <span role="gridcell">{p.elementCount}</span>
                <span role="gridcell">{p.version}</span>
              </div>
            ))}
            {/* the empty line is a row of the grid (a grid needs rows; phones hide the header row, WB-26 axe) */}
            {packs.data && own.length === 0 && <div role="row"><p role="gridcell" className="v4-grid-empty">No homebrew packs yet. Make one with New pack.</p></div>}
            {packs.data && own.length > 0 && rows.length === 0 && <div role="row"><p role="gridcell" className="v4-grid-empty">No packs match.</p></div>}
          </div>
        </div>
        {q && own.length > 0 && <FoundElements found={found} kinds={kinds.data ?? []} selectedKey={foundId} onSelect={selectFound} onOpen={openFound} />}
      </div>
      <div className="v4-src-desc">
        {chosen ? (
          <PaneShell title={chosen.row.name} intro={kindLabelOf(chosen, kinds.data ?? [])} imageIcon={false}>
            <p className="desc-line"><i>Pack.</i> {chosen.pack.name}</p>
            {chosen.row.tags.length > 0 && <p className="desc-line"><i>Tags.</i> {chosen.row.tags.join(", ")}</p>}
            <p className="hb-open"><Button onClick={() => openFound(chosen)}>Open element</Button></p>
          </PaneShell>
        ) : selected ? (
          <PaneShell title={selected.name} intro="Homebrew pack" imageIcon={false}>
            <p className="desc-line"><i>Edition.</i> {editionLabel(selected.rulesets)}</p>
            <p className="desc-line"><i>Elements.</i> {selected.elementCount}</p>
            <p className="desc-line"><i>Version.</i> {selected.version}</p>
            {selected.attribution && <p className="desc-line"><i>Author.</i> {selected.attribution}</p>}
            <p className="desc-line"><i>Licence.</i> {selected.license === "none" ? "All rights reserved" : selected.license}</p>
            {packDrafts > 0 && <p className="desc-line"><i>Unsaved changes.</i> {packDrafts} {packDrafts === 1 ? "element" : "elements"}</p>}
            <p className="hb-open"><Button onClick={() => openPack(selected)}>Open pack</Button></p>
          </PaneShell>
        ) : <aside className="desc-pane" aria-label="Description" />}
      </div>
      {creating && <NewPackFlyout onCreated={onCreated} onClose={() => setCreating(false)} />}
      {exporting && <ExportPackDialog pack={exporting} onClose={() => setExporting(null)} />}
    </div>
  );
}
