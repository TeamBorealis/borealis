import { mdiFileDocumentPlusOutline, mdiRefresh } from "@mdi/js";
import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Navigate } from "react-router";
import type { VaultEntry, VaultEntryKind } from "../../api/contract";
import { Alert, Button, SearchInput, useConfirm, useShell } from "../../components";
import { VAULT_UNAVAILABLE } from "../../components/shell/useFolderNotice";
import { useApi } from "../../state/ApiContext";
import { usePlatform } from "../../state/platform";
import { useSettings } from "../../state/SettingsContext";
import { setStatus } from "../../state/status";
import "../builder/builder.css";
import "../content/content.css";
import { ConflictDialog, MissingDialog, NameDialog, RenameLinksDialog } from "./NoteDialogs";
import { NotePane, type NoteMode } from "./NotePane";
import { NoteSidePane } from "./NoteSidePane";
import { Svg, baseName, displayName, renameStem } from "./noteNames";
import "./notes.css";
import { useNoteActions, type OpenPath } from "./useNoteActions";
import { useNoteEditor } from "./useNoteEditor";
import { useBacklinks, useVaultData, useVaultSearch } from "./useVaultData";
import { ancestors, VaultTreeView } from "./VaultTreeView";

function kindOf(path: string): VaultEntryKind {
  if (/\.md$/i.test(path)) return "note";
  if (/\.canvas$/i.test(path)) return "canvas";
  if (/\.(?:png|jpe?g|gif|webp|bmp|svg|avif)$/i.test(path)) return "image";
  return "file";
}

/** No vault yet, or it is unplugged: say where to choose it (Settings > Content). */
function NoVault({ unavailable, error }: { unavailable: boolean; error: string | null }) {
  const { openSettings } = useShell();
  return (
    <div className="v4-content nt-novault">
      <section className="v4-csec">
        <header><h2>Notes</h2></header>
        <div className="body">
          {unavailable
            ? <Alert tone="warning" role="status">{VAULT_UNAVAILABLE}</Alert>
            : <p>Open your Obsidian vault, or any folder of Markdown notes, and read and edit its notes here. Borealis works on the files where they are: nothing is copied, and Obsidian sees every change.</p>}
          {error && <Alert tone="danger" role="alert">{error}</Alert>}
          <div className="v4-content-cmds"><Button onClick={() => openSettings("content")}>Choose vault folder...</Button></div>
        </div>
      </section>
    </div>
  );
}

/**
 * START > NOTES (GitHub #15): one Obsidian vault opened in place. Files on the left (tree or search results), the open note in
 * the middle (Read / Edit), and the description pane on the right with backlinks and tags. Desktop only for now.
 */
export function NotesPage() {
  const { mobile } = usePlatform();
  if (mobile) return <Navigate to="/characters" replace />;
  return <Notes />;
}

function Notes() {
  const api = useApi();
  const { settings } = useSettings();
  const confirm = useConfirm();
  const editor = useNoteEditor(api, settings.autosave);
  const data = useVaultData(api, editor.external);
  const [selected, setSelected] = useState<string | null>(null);
  const [mode, setMode] = useState<NoteMode>("read");
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const [query, setQuery] = useState("");
  const [tag, setTag] = useState<string | null>(null);
  const [scrollTo, setScrollTo] = useState<string | null>(null);
  const [later, setLater] = useState(false);
  const hits = useVaultSearch(api, query, tag, data.version);
  const backlinks = useBacklinks(api, selected && kindOf(selected) === "note" ? selected : null, data.version, editor.note?.hash ?? null);

  const openPath: OpenPath = useCallback(async (path, opts = {}) => {
    const kind = kindOf(path);
    const ok = kind === "note" || kind === "canvas" ? await editor.open(path) : await editor.close();
    if (!ok) { setStatus("Save or resolve the open note first."); return; }
    setExpanded((prev) => new Set([...prev, ...ancestors(path)]));
    setSelected(path);
    setScrollTo(opts.subpath ?? null);
    if (opts.mode) setMode(opts.mode);
  }, [editor]);

  const actions = useNoteActions({ api, editor, confirm, selected, reload: data.reload, openPath, setSelected, setScrollTo });

  // Leaving the view saves the open note, like leaving a character.
  const save = useRef(editor.save);
  save.current = editor.save;
  useEffect(() => () => { void save.current(); }, []);
  useEffect(() => { if (!editor.conflict) setLater(false); }, [editor.conflict]);

  const toggleFolder = (folder: string) => setExpanded((prev) => {
    const next = new Set(prev);
    if (next.has(folder)) next.delete(folder); else next.add(folder);
    return next;
  });
  const showTag = (t: string) => { setTag(t); setQuery(""); };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || e.key.toLowerCase() !== "s") return;
    // Ctrl/Cmd+S saves the note here, not the character.
    e.preventDefault();
    e.stopPropagation();
    if (!selected) return;
    void editor.save().then((ok) => ok && setStatus(`'${displayName(baseName(selected))}' saved.`));
  };

  const tree = data.tree;
  if (!tree) return data.error ? <NoVault unavailable={false} error={data.error} /> : null;
  if (!tree.dir || tree.unavailable) return <NoVault unavailable={tree.unavailable} error={data.error} />;

  const entry: VaultEntry | null = selected ? { path: selected, name: baseName(selected), kind: kindOf(selected), size: 0, mtimeMs: 0 } : null;
  const name = entry ? displayName(entry.name, entry.kind) : tree.name ?? "Notes";
  return (
    <div className="nt-page" onKeyDown={onKeyDown}>
      <aside className="nt-files" aria-label="Vault">
        <div className="nt-files-head">
          <span className="nt-vault" title={tree.dir}>{tree.name}</span>
          <span className="v4-grid-cmds" role="toolbar" aria-label="Vault commands">
            <button type="button" className="v4-grid-cmd" aria-label="New note" title="New note" onClick={() => void actions.createNote()}><Svg d={mdiFileDocumentPlusOutline} /></button>
            <button type="button" className="v4-grid-cmd" aria-label="Refresh" title="Refresh" onClick={() => void data.reload()}><Svg d={mdiRefresh} /></button>
          </span>
        </div>
        <div className="nt-search"><SearchInput value={query} onChange={(v) => { setQuery(v); setTag(null); }} label="Search notes" /></div>
        {tag && (
          <div className="nt-filter" role="status">
            <span>Notes tagged #{tag}</span>
            <button type="button" className="nt-link" onClick={() => setTag(null)}>Show all files</button>
          </div>
        )}
        <div className="nt-files-body">
          {hits ? (
            <ul className="nt-results" aria-label="Search results">
              {hits.length === 0 && <li className="nt-empty">No note matches.</li>}
              {hits.map((h) => (
                <li key={h.path}>
                  <button type="button" className={`nt-row${h.path === selected ? " sel" : ""}`} onClick={() => void openPath(h.path)}>{displayName(h.path)}</button>
                  {h.lines.map((l) => <span key={l.line} className="nt-snippet">{l.text}</span>)}
                </li>
              ))}
            </ul>
          ) : (
            <VaultTreeView entries={tree.entries} expanded={expanded} selected={selected} onToggle={toggleFolder} onOpen={(e) => void openPath(e.path)} />
          )}
          {tree.truncated && <p className="nt-muted">This vault is very large: only part of it is listed.</p>}
        </div>
      </aside>
      {entry ? (
        <div className="nt-main">
          {editor.conflict && later && (
            <Alert tone="warning" role="status">This note changed in another app. <button type="button" className="nt-link" onClick={() => setLater(false)}>Resolve...</button></Alert>
          )}
          <NotePane entry={entry} entries={tree.entries} editor={editor} mode={mode} setMode={setMode} version={data.version} scrollTo={scrollTo}
            onLink={(l) => void actions.followLink(l)} onTag={showTag} onRename={() => actions.startRename(entry.path)} onDelete={() => void actions.deleteFile(entry.path)} />
        </div>
      ) : (
        <section className="nt-main nt-none"><p className="nt-muted">Choose a note on the left, or create one.</p></section>
      )}
      <NoteSidePane title={name} backlinks={backlinks} tags={data.tags} onTag={showTag}
        onOpen={(p) => void openPath(p)} />
      {editor.conflict && !later && (
        <ConflictDialog name={name} mine={editor.draft} theirs={editor.conflict} onKeepMine={() => void editor.keepMine()} onTakeTheirs={editor.takeTheirs} onLater={() => setLater(true)} />
      )}
      {editor.missing && <MissingDialog name={name} onRecreate={() => void editor.recreate()} onDiscard={() => { editor.discard(); setSelected(null); }} />}
      {actions.rename && !actions.rename.plan && (
        <NameDialog title="Rename" label="New name" initial={renameStem(actions.rename.path)} confirm="Rename" error={actions.rename.error}
          onSubmit={(n) => void actions.submitName(n)} onClose={actions.cancelRename} />
      )}
      {actions.rename?.plan && actions.rename.plan.files.length > 0 && (
        <RenameLinksDialog plan={actions.rename.plan} onRename={(u) => void actions.doRename(u)} onClose={actions.cancelRename} />
      )}
    </div>
  );
}
