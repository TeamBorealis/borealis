import { type MutableRefObject, useCallback, useEffect, useMemo, useState } from "react";
import type { HomebrewSpellListRow, HomebrewSpellListView } from "../../api/contract";
import { Alert, ComboBox, TextBox } from "../../components";
import { haptic } from "../../lib/haptics";
import { useApi } from "../../state/ApiContext";
import { setStatus } from "../../state/status";
import { errorMessage, useAsync } from "../../state/useAsync";
import { Band } from "../spells/TwoPane";
import { refusalText } from "./BulkActions";
import { SpellPickerGrid } from "./pickers/SpellPickerGrid";
import { nextSelection } from "./search";
import { useMessageBox } from "./useMessageBox";

/** How many declared lists the list chooser reads (classes are few; one page holds them all). */
const LIST_PAGE = 500;
/** A subclass's expanded list in the chooser: `expanded:<subclass id>` (spec 12.4). */
const EXPANDED = "expanded:";
const expandedOf = (value: string): string | null => (value.startsWith(EXPANDED) ? value.slice(EXPANDED.length) : null);

/** What the workspace can ask of the open SPELL LISTS view: its unsaved changes, and Save (Ctrl+S). */
export interface SpellListsHandle {
  pending: () => number;
  save: () => Promise<void>;
}

export interface SpellListsProps {
  packId: string;
  narrow: boolean;
  /** The spell the description pane shows (and WHERE USED lists the users of). */
  described: string | null;
  onDescribe: (id: string | null) => void;
  /** Filled while the view is open (Ctrl+S, leaving with unsaved changes). */
  handle: MutableRefObject<SpellListsHandle | null>;
  /** Called after a save (the description pane's WHERE USED reads again). */
  onSaved?: () => void;
}

const spells = (n: number): string => `${n} ${n === 1 ? "spell" : "spells"}`;
/** "the Wizard list", or a subclass's "The Fiend expanded list" (its name says so). */
const listWords = (v: HomebrewSpellListView): string => (v.expandedOf ? v.listTag : `the ${v.listTag} list`);

/**
 * SPELL LISTS (spec 12.4, 13.7): choose a list (one a loaded or own class declares, or another by name), then check spells
 * in the Magic grid over every loaded spell. A spell of the pack changes its own tags; any other spell gets the pack's
 * additive append ("add to this list", HC5): the original is never edited. Only what the pack added can be unchecked.
 * Checks wait for Save (Ctrl+S): one write for them all. Ctrl/Shift+click chooses rows for "Add selected" / "Remove selected".
 */
export function SpellLists({ packId, narrow, described, onDescribe, handle, onSaved }: SpellListsProps) {
  const api = useApi();
  const { ask, box } = useMessageBox();
  const lists = useAsync(async () => {
    const page = await api.homebrewReferenceOptions({ target: { types: ["Spell List"] }, query: "", filters: {}, page: { offset: 0, limit: LIST_PAGE } });
    return page.items.map((r) => r.name);
  }, [api]);
  // a subclass's expanded list (a patron's, spec 12.4): the subclasses that have one, and this pack's
  const expandedLists = useAsync(async () => {
    const page = await api.homebrewReferenceOptions({ target: { types: ["Expanded Spell List"] }, query: "", filters: {}, page: { offset: 0, limit: LIST_PAGE } });
    return page.items.map((r) => ({ value: `${EXPANDED}${r.id}`, label: `${r.name} (expanded list)` }));
  }, [api]);
  const [list, setList] = useState("");
  const [other, setOther] = useState("");
  const [view, setView] = useState<HomebrewSpellListView | null>(null);
  const [pending, setPending] = useState<ReadonlyMap<string, boolean>>(new Map());
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [anchor, setAnchor] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!list) { setView(null); return undefined; }
    let live = true;
    setError(null);
    const sub = expandedOf(list);
    api.homebrewSpellList(sub ? { packId, listTag: "", expandedOf: sub } : { packId, listTag: list })
      .then((v) => { if (live) setView(v); })
      .catch((e) => { if (live) setError(errorMessage(e)); });
    return () => { live = false; };
  }, [api, packId, list]);

  const rows = useMemo(() => view?.rows ?? [], [view]);
  const byId = useMemo(() => new Map(rows.map((r) => [r.id, r])), [rows]);
  const checked = useCallback((id: string) => pending.get(id) ?? byId.get(id)?.onList ?? false, [pending, byId]);
  const locked = (id: string): string | null => {
    const r = byId.get(id);
    return r && r.onList && !r.removable ? `Its own pack puts it on ${view ? listWords(view) : `the ${list} list`}; a homebrew pack can only add to that list.` : null;
  };
  const want = (r: HomebrewSpellListRow, on: boolean, from: ReadonlyMap<string, boolean>): Map<string, boolean> => {
    const next = new Map(from);
    if (on === r.onList) next.delete(r.id);
    else if (!on && !r.removable) return next;
    else next.set(r.id, on);
    return next;
  };
  const toggle = (r: HomebrewSpellListRow) => { if (!locked(r.id) || !checked(r.id)) setPending((p) => want(r, !checked(r.id), p)); };
  const setSelectedOn = (on: boolean) => {
    setPending((p) => [...selected].reduce((acc, id) => { const r = byId.get(id); return r ? want(r, on, acc) : acc; }, new Map(p)));
  };
  const onCount = (view?.count ?? 0) + [...pending.values()].filter(Boolean).length - [...pending.values()].filter((v) => !v).length;

  const save = useCallback(async () => {
    if (!view || pending.size === 0) return;
    setBusy(true);
    setError(null);
    const add = [...pending].filter(([, on]) => on).map(([id]) => id);
    const remove = [...pending].filter(([, on]) => !on).map(([id]) => id);
    try {
      const next = await api.homebrewSetSpellList(view.expandedOf
        ? { packId, listTag: "", expandedOf: view.expandedOf, add, remove }
        : { packId, listTag: view.listTag, add, remove });
      setView(next);
      setPending(new Map());
      haptic("save");
      setStatus(`Saved ${listWords(next)}: ${spells(next.count)}.`);
      onSaved?.();
    } catch (e) {
      setError(refusalText(e));
    } finally {
      setBusy(false);
    }
  }, [api, packId, view, pending, onSaved]);

  useEffect(() => {
    handle.current = { pending: () => pending.size, save };
    return () => { handle.current = null; };
  }, [handle, pending, save]);

  const choose = async (name: string) => {
    if (pending.size > 0) {
      const r = await ask({
        title: "Unsaved changes", cancelId: "cancel",
        message: `${pending.size} ${pending.size === 1 ? "change" : "changes"} to ${view ? listWords(view) : `the ${list} list`} ${pending.size === 1 ? "is" : "are"} not saved.`,
        buttons: [{ id: "save", label: "Save" }, { id: "discard", label: "Discard" }, { id: "cancel", label: "Cancel" }],
      });
      if (r === "cancel") return;
      if (r === "save") await save();
    }
    setPending(new Map());
    setSelected(new Set());
    setList(name.trim());
  };
  const q = text.trim().toLowerCase();
  const shown = q ? rows.filter((r) => r.name.toLowerCase().includes(q)) : rows;
  const tags = [...new Set([...(lists.data ?? []), ...(list && !expandedOf(list) && !(lists.data ?? []).includes(list) ? [list] : [])])].sort((a, b) => a.localeCompare(b));
  const options = [...tags.map((n) => ({ value: n, label: n })), ...(expandedLists.data ?? [])];

  return (
    <section className="hb-lists" aria-label="Spell lists">
      <Band title="Spell lists">{view && <span className="hb-lists-count" role="status">{spells(onCount)} on this list{pending.size > 0 ? ` (${pending.size} not saved)` : ""}</span>}</Band>
      <div className="hb-lists-head">
        <span className="hb-lists-pick">
          <ComboBox aria-label="Spell list" value={list} onChange={(v) => void choose(v)} options={[{ value: "", label: "Choose a list..." }, ...options]} />
        </span>
        <span className="hb-lists-other">
          <TextBox aria-label="Another list" placeholder="Another list..." value={other} onChange={(e) => setOther(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && other.trim()) { void choose(other); setOther(""); } }} />
          <button type="button" className="hb-tool" disabled={!other.trim()} onClick={() => { void choose(other); setOther(""); }}>Open</button>
        </span>
      </div>
      {view && (
        <p className="hb-field-help">
          {view.expandedOf
            ? (view.own ? `${view.listTag} belongs to this pack's subclass: a check adds the spell to it.`
              : `${view.listTag} belongs to another pack's subclass: a check adds the spell to it from this pack, the original stays as it is.`)
            : view.own ? `${view.listTag} is a list of this pack.` : `${view.listTag} is another pack's list: a check adds the spell to it from this pack, the original stays as it is.`}
        </p>
      )}
      <div className="eq-inv-actions hb-lists-cmds" role="toolbar" aria-label="Spell list commands">
        <button type="button" disabled={!view || selected.size === 0} onClick={() => setSelectedOn(true)}>Add selected</button>
        <button type="button" disabled={!view || selected.size === 0} onClick={() => setSelectedOn(false)}>Remove selected</button>
        <button type="button" disabled={busy || pending.size === 0} title="Save (Ctrl+S)" onClick={() => void save()}>Save</button>
        <button type="button" disabled={busy || pending.size === 0} onClick={() => setPending(new Map())}>Discard</button>
      </div>
      {error && <Alert tone="danger" role="alert">{error}</Alert>}
      {!list ? <p className="hb-pending">Choose a spell list to see which spells are on it.</p>
        : !view ? <p className="hb-pending">Loading...</p>
          : (
            <SpellPickerGrid rows={shown} total={rows.length} text={text} onText={setText} multiple foldFilter={narrow}
              picked={checked} highlighted={described} onDescribe={(r) => onDescribe(r.id)} onChoose={(r) => toggle(r as HomebrewSpellListRow)}
              locked={(id) => (checked(id) ? locked(id) : null)}
              isSelected={(id) => selected.has(id)}
              onRowClick={(r, e) => {
                const toggleSel = e.ctrlKey || e.metaKey;
                if (!toggleSel && !e.shiftKey) { setSelected(new Set([r.id])); setAnchor(r.id); return false; }
                setSelected(nextSelection(selected, shown.map((x) => x.id), r.id, anchor, { toggle: toggleSel, range: e.shiftKey }));
                setAnchor(r.id);
                return true;
              }} />
          )}
      {box}
    </section>
  );
}
