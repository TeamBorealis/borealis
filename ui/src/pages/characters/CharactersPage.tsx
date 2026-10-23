import { CheckBox } from "../../components/controls";
import { useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router";
import type { CharacterSummary, RecheckReport } from "../../api/contract";
import { Alert, Button, Modal, SearchInput, TextField, useConfirm, useToast } from "../../components";
import { fileToBase64 } from "../../lib/files";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { usePlatform } from "../../state/platform";
import { useSettings } from "../../state/SettingsContext";
import { errorMessage, useAsync } from "../../state/useAsync";
import { type CharacterImportEntry, CharacterImportReportDialog } from "./CharacterImportReportDialog";
import { CharacterTile } from "./CharacterTile";
import "./characters.css";
import { type NewCharacterForm, NewCharacterDialog, applyStandardArray } from "./NewCharacterDialog";

function groupBy(list: CharacterSummary[]): [string, CharacterSummary[]][] {
  const map = new Map<string, CharacterSummary[]>();
  for (const c of list) map.set(c.group, [...(map.get(c.group) ?? []), c]);
  return [...map.entries()]
    .sort(([a], [b]) => (a === "Characters" ? -1 : b === "Characters" ? 1 : a.localeCompare(b)))
    .map(([g, items]) => [g, [...items].sort((a, b) => Number(b.favorite) - Number(a.favorite) || a.name.localeCompare(b.name))]);
}

/** Fold a re-check result into the open report: what is no longer listed has been resolved. */
function applyRecheck(entries: CharacterImportEntry[], rr: RecheckReport): CharacterImportEntry[] {
  const latest = new Map([...rr.updated, ...rr.outstanding].map((e) => [e.characterId, e]));
  return entries.map((e) => {
    if (!e.report) return e;
    const now = latest.get(e.report.summary.id);
    return { ...e, report: { ...e.report, missingContent: now?.stillMissing ?? [], unmappedSlots: now?.stillUnmapped ?? [] } };
  });
}

export function CharactersPage() {
  const api = useApi();
  const nav = useNavigate();
  const toast = useToast();
  const confirm = useConfirm();
  const { settings, update } = useSettings();
  const ch = useCharacter();
  // Open-with imports (GlobalImport) navigate here with a fresh `imported` state: list again even when already showing.
  const imported = (useLocation().state as { imported?: number } | null)?.imported;
  const list = useAsync(() => api.listCharacters(), [api, imported]);
  const [query, setQuery] = useState("");
  const [finding, setFinding] = useState(false);
  const [creating, setCreating] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [params, setParams] = useSearchParams();
  const { mobile, ready } = usePlatform();
  const [entries, setEntries] = useState<CharacterImportEntry[] | null>(null);
  const [rechecking, setRechecking] = useState(false);
  const [editing, setEditing] = useState<CharacterSummary | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  // Shell actions arrive as /characters?new=1 (New Character), ?load=1 (File > Load Character) and ?find=1 (File > Search Characters).
  useEffect(() => {
    const a = params.get("new") === "1" ? "new" : params.get("load") === "1" ? "load" : params.get("find") === "1" ? "find" : null;
    if (!a || !ready) return; // wait for settings: desktop and mobile load differently
    if (a === "new") setCreating(true);
    // A webview ignores a scripted click on a file input outside a user gesture (this runs after the menu navigated),
    // so desktop asks the backend for the native open panel; mobile keeps the system picker.
    else if (a === "load") { if (mobile) fileRef.current?.click(); else void loadNative(); }
    else setFinding(true);
    setParams({}, { replace: true });
  // eslint-disable-next-line react-hooks/exhaustive-deps -- runs once per shell action
  }, [params, setParams, ready]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (list.data ?? []).filter((c) => !q || c.name.toLowerCase().includes(q) || c.build.toLowerCase().includes(q) || c.group.toLowerCase().includes(q));
  }, [list.data, query]);

  /** Leaves the current character, honouring unsaved changes (Save / Discard / Cancel). */
  const leaveCurrent = async (): Promise<boolean> => {
    const cur = ch.snapshot;
    if (!cur?.dirty) return true;
    if (settings.autosave) { await ch.save(); return true; }
    const r = await confirm({ title: "Unsaved changes", body: `${cur.name} has unsaved changes. Save?`, confirmLabel: "Save", extra: { label: "Discard", value: "discard" } });
    if (r === "cancel") return false;
    if (r === "confirm") await ch.save();
    return true;
  };

  const open = async (c: CharacterSummary) => {
    if (ch.snapshot?.id !== c.id && !(await leaveCurrent())) return;
    if (await ch.open(c.id)) nav("/builder/race");
  };

  const create = async (f: NewCharacterForm) => {
    if (!(await leaveCurrent())) return;
    try {
      const snap = await api.createCharacter({ request: {
        name: f.name, ruleset: f.ruleset, startingLevel: f.startingLevel, gender: f.gender, abilityMethod: f.abilityMethod,
        options: { multiclassing: f.multiclassing, feats: f.feats, averageHp: f.averageHp },
      } });
      ch.adopt(await applyStandardArray(api, snap, f));
      void update({ abilityMethod: f.abilityMethod });
      setCreating(false);
      nav("/builder/race");
    } catch (e) { toast.push(errorMessage(e), "error"); }
  };

  const remove = async (c: CharacterSummary) => {
    const r = await confirm({ title: "Delete character", body: `Delete ${c.name}? This cannot be undone.`, confirmLabel: "Delete", danger: true });
    if (r !== "confirm") return;
    if (ch.snapshot?.id === c.id) await ch.close(true);
    await api.deleteCharacter({ characterId: c.id });
    list.reload();
  };

  /** Imports every chosen file (one report entry each); a bad file does not stop the others. */
  const importAll = async (picked: { fileName: string; content: () => Promise<string> }[]) => {
    if (picked.length === 0) return;
    const results: CharacterImportEntry[] = [];
    for (const f of picked) {
      try {
        results.push({ fileName: f.fileName, report: await api.importCharacter({ fileName: f.fileName, contentBase64: await f.content() }) });
      } catch (e) { results.push({ fileName: f.fileName, error: errorMessage(e) }); }
    }
    setEntries(results);
    list.reload();
  };

  const onFiles = async (files: FileList | null) => {
    await importAll((files ? [...files] : []).map((file) => ({ fileName: file.name, content: () => fileToBase64(file) })));
    if (fileRef.current) fileRef.current.value = "";
  };

  /** File > Load Character on desktop: the backend's open panel, then the picked files' contents by token. */
  const loadNative = async () => {
    try {
      const picked = await api.pickPath({ purpose: "character-file" });
      if (picked.length === 0) return;
      const files = await api.takePickedFiles({ purpose: "character-file", tokens: picked.map((p) => p.token) });
      await importAll(files.map((f) => ({ fileName: f.fileName, content: async () => f.contentBase64 })));
    } catch (e) { toast.push(errorMessage(e), "error"); }
  };

  const recheck = async () => {
    setRechecking(true);
    try {
      const rr = await api.recheckMissingContent();
      setEntries((cur) => (cur ? applyRecheck(cur, rr) : cur));
      list.reload();
      const n = rr.updated.reduce((t, e) => t + e.resolved, 0);
      toast.push(rr.checked === 0 ? "No character is waiting for content."
        : n > 0 ? `Found ${n} missing item${n === 1 ? "" : "s"} for ${rr.updated.length} character${rr.updated.length === 1 ? "" : "s"}.`
        : "No new content was found for the missing items.", n > 0 ? "info" : "info");
    } catch (e) { toast.push(errorMessage(e), "error"); }
    setRechecking(false);
  };

  const waiting = (list.data ?? []).some((c) => c.missingContent > 0);

  const groups = groupBy(shown);
  return (
    <div className="v4-start" style={{ ["--col" as string]: `${settings.portraitSize}px` }}>
      <input ref={fileRef} type="file" multiple {...(mobile ? {} : { accept: ".dnd5e,.json,application/json" })} hidden aria-label="Import character files" onChange={(e) => void onFiles(e.target.files)} />
      <div className="v4-start-left">
        <h1 className="v4-section">Welcome to Borealis</h1>
        <p className="v4-lead">Open any saved hero here to play, advance, or adjust them.</p>
        <p className="v4-lead">Heroes may also be sorted into named groups, and the ones you star appear at the top of whichever group holds them.</p>
        <button type="button" className="v4-newbtn" onClick={() => setCreating(true)}><span className="t">New character</span><span className="d">Create a new character</span></button>
        {mobile && <button type="button" className="v4-newbtn" onClick={() => fileRef.current?.click()}><span className="t">Import characters</span><span className="d">Load one or more .dnd5e files</span></button>}
        {waiting && <button type="button" className="v4-newbtn small" disabled={rechecking} onClick={() => void recheck()}><span className="t">Re-check missing content</span><span className="d">Use content you installed since importing</span></button>}
      </div>
      <div className="v4-start-right">
        {list.error && <Alert tone="danger">{list.error}</Alert>}
        {((list.data ?? []).length > 0 || finding) && (
          <div className="v4-rule">
            <h2>Characters</h2>
            {finding && <div className="v4-search"><SearchInput value={query} onChange={setQuery} label="Search characters" autoFocus onKeyDown={(e) => { if (e.key === "Escape") { setQuery(""); setFinding(false); } }} /></div>}
          </div>
        )}
        {finding && list.data && shown.length === 0 && <p className="v4-lead">No characters match your search.</p>}
        {groups.map(([group, items]) => (
          <section key={group} className="v4-group" aria-label={`Group ${group}`}>
            {group !== "Characters" && <h3>{group}</h3>}
            <div className="v4-tiles">
              {items.map((c) => (
                <CharacterTile key={c.id} c={c} selected={selectedId === c.id} loaded={ch.snapshot?.id === c.id}
                  onSelect={() => setSelectedId(c.id)} onLoad={() => void open(c)} onEdit={() => setEditing(c)} onDelete={() => void remove(c)} />
              ))}
            </div>
          </section>
        ))}
      </div>
      {creating && <NewCharacterDialog onCreate={(f) => void create(f)} onClose={() => setCreating(false)} />}
      {entries && <CharacterImportReportDialog entries={entries} rechecking={rechecking} onClose={() => setEntries(null)}
        onOpenContent={() => { setEntries(null); nav("/content"); }} onRecheck={() => void recheck()} />}
      {editing && <EditDialog c={editing} groups={groupBy(list.data ?? []).map(([g]) => g)} onClose={() => setEditing(null)}
        onSave={(patch) => { void api.updateCharacterMeta({ characterId: editing.id, ...patch }).then(() => { setEditing(null); list.reload(); }); }}
        onDuplicate={() => { void api.duplicateCharacter({ characterId: editing.id }).then(() => { setEditing(null); list.reload(); }); }} />}
    </div>
  );
}

function EditDialog({ c, groups, onSave, onDuplicate, onClose }: {
  c: CharacterSummary; groups: string[]; onSave: (patch: { group: string; favorite: boolean }) => void; onDuplicate: () => void; onClose: () => void;
}) {
  const [g, setG] = useState(c.group);
  const [fav, setFav] = useState(c.favorite);
  return (
    <Modal title={`Edit ${c.name}`} onClose={onClose}
      footer={<><Button onClick={onDuplicate}>Duplicate</Button><Button onClick={onClose}>Cancel</Button><Button variant="primary" disabled={!g.trim()} onClick={() => onSave({ group: g.trim(), favorite: fav })}>Save</Button></>}>
      <TextField label="Group" list="groups" value={g} data-autofocus onChange={(e) => setG(e.target.value)} />
      <datalist id="groups">{groups.map((x) => <option key={x} value={x} />)}</datalist>
      <label className="v4-fav"><CheckBox checked={fav} onChange={(e) => setFav(e.target.checked)} /> Favorite (listed first in its group)</label>
    </Modal>
  );
}
