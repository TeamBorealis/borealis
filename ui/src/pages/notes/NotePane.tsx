import { mdiContentSave, mdiDeleteOutline, mdiLinkVariant, mdiPaperclip, mdiRenameOutline } from "@mdi/js";
import { useEffect, useRef, useState } from "react";
import type { VaultEntry } from "../../api/contract";
import { Alert } from "../../components";
import { TabControl } from "../../components/controls/TabControl";
import { useApi } from "../../state/ApiContext";
import { setStatus } from "../../state/status";
import { errorMessage } from "../../state/useAsync";
import { toggleTask } from "./markdown/sections";
import { LinkPickerDialog } from "./NoteDialogs";
import { NotePreview, type ClickedLink } from "./NotePreview";
import { Svg, displayName } from "./noteNames";
import type { NoteEditor } from "./useNoteEditor";

export type NoteMode = "read" | "edit";

interface PaneProps {
  entry: VaultEntry;
  entries: readonly VaultEntry[];
  editor: NoteEditor;
  mode: NoteMode;
  setMode: (m: NoteMode) => void;
  version: number;
  scrollTo: string | null;
  onLink: (link: ClickedLink) => void;
  onTag: (tag: string) => void;
  onRename: () => void;
  onDelete: () => void;
}

const MODES = [{ id: "read", label: "Read" }, { id: "edit", label: "Edit" }];

function saveState(editor: NoteEditor): string {
  if (editor.saving) return "Saving...";
  if (editor.conflict) return "Changed in another app";
  if (editor.missing) return "Deleted in another app";
  return editor.dirty ? "Unsaved changes" : "";
}

function Cmd({ d, label, onClick, disabled }: { d: string; label: string; onClick: () => void; disabled?: boolean }) {
  return <button type="button" className="v4-grid-cmd" aria-label={label} title={label} disabled={disabled} onClick={onClick}><Svg d={d} /></button>;
}

/** An image of the vault, read through the backend. */
function ImageView({ entry, version }: { entry: VaultEntry; version: number }) {
  const api = useApi();
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    setUrl(null);
    api.vaultImage({ path: entry.path }).then((u) => alive && setUrl(u), (e) => alive && setError(errorMessage(e)));
    return () => { alive = false; };
  }, [api, entry.path, version]);
  if (error) return <Alert tone="warning" role="status">{error}</Alert>;
  return url ? <img className="nt-image" src={url} alt={entry.name} /> : null;
}

/** The open file: a note (Read / Edit), a text file shown as text, an image, or another file that is only listed. */
export function NotePane({ entry, entries, editor, mode, setMode, version, scrollTo, onLink, onTag, onRename, onDelete }: PaneProps) {
  const api = useApi();
  const area = useRef<HTMLTextAreaElement>(null);
  const [picking, setPicking] = useState(false);
  const note = editor.note?.path === entry.path ? editor.note : null;
  const readOnly = !!note?.readOnly;
  const isNote = entry.kind === "note";
  const shown = mode === "edit" && !readOnly && isNote ? "edit" : "read";

  /** Insert `text` at the cursor (or over the selection) of the editor. */
  const insert = (text: string) => {
    const el = area.current;
    const start = el?.selectionStart ?? editor.draft.length;
    const end = el?.selectionEnd ?? start;
    editor.edit(editor.draft.slice(0, start) + text + editor.draft.slice(end));
    requestAnimationFrame(() => { el?.focus(); el?.setSelectionRange(start + text.length, start + text.length); });
  };
  const link = async (to: string) => {
    setPicking(false);
    try { insert(await api.vaultLinkText({ from: entry.path, to, embed: false })); } catch (e) { setStatus(errorMessage(e)); }
  };
  const attach = async () => {
    try {
      const [pick] = await api.pickPath({ purpose: "vault-attachment" });
      if (!pick) return;
      const done = await api.vaultAttach({ token: pick.token, from: entry.path });
      insert(done.link);
      setStatus(`${displayName(done.path)} was added to the vault.`);
    } catch (e) { setStatus(errorMessage(e)); }
  };
  const save = async () => {
    if (await editor.save()) setStatus(`'${displayName(entry.name, entry.kind)}' saved.`);
  };
  const toggle = (line: number) => {
    const next = toggleTask(editor.draft, line);
    if (next !== null) editor.edit(next);
  };

  const body = (() => {
    if (entry.kind === "image") return <ImageView entry={entry} version={version} />;
    if (entry.kind === "file") return <p className="nt-muted">Borealis lists this file but does not open it. Open it in Obsidian or another app.</p>;
    if (!note) return editor.error ? <Alert tone="danger" role="alert">{editor.error}</Alert> : null;
    if (entry.kind === "canvas") return <pre className="nt-plain" aria-label="File text">{editor.draft}</pre>;
    if (shown === "edit") {
      return <textarea ref={area} className="nt-editor" aria-label="Note text" value={editor.draft} spellCheck onChange={(e) => editor.edit(e.target.value)} />;
    }
    return <NotePreview path={entry.path} text={editor.draft} version={version} scrollTo={scrollTo} onLink={onLink} onTag={onTag} onToggleTask={readOnly ? null : toggle} />;
  })();

  return (
    <section className="nt-note" aria-label={displayName(entry.name, entry.kind)}>
      <header className="nt-head">
        <h1 className="nt-title" title={entry.path}>{displayName(entry.name, entry.kind)}</h1>
        <span className="nt-state" role="status">{note ? saveState(editor) : ""}</span>
        <span className="v4-grid-cmds" role="toolbar" aria-label="Note commands">
          {isNote && <Cmd d={mdiContentSave} label="Save note" disabled={!editor.dirty || readOnly} onClick={() => void save()} />}
          {isNote && shown === "edit" && <Cmd d={mdiLinkVariant} label="Insert link" onClick={() => setPicking(true)} />}
          {isNote && shown === "edit" && <Cmd d={mdiPaperclip} label="Attach file" onClick={() => void attach()} />}
          <Cmd d={mdiRenameOutline} label="Rename" onClick={onRename} />
          <Cmd d={mdiDeleteOutline} label="Delete" onClick={onDelete} />
        </span>
      </header>
      {isNote && (
        <div className="nt-modes">
          <TabControl tabs={MODES.map((m) => ({ ...m, disabled: m.id === "edit" && readOnly }))} active={shown} onChange={(id) => setMode(id as NoteMode)} label="View" idPrefix="nt-mode" variant="sub" />
        </div>
      )}
      {readOnly && note?.readOnly && <Alert tone="info" role="status">{note.readOnly}</Alert>}
      {editor.notice && <p className="nt-notice" role="status">{editor.notice}</p>}
      {editor.error && note && <Alert tone="danger" role="alert">{editor.error}</Alert>}
      <div className="nt-body">{body}</div>
      {picking && <LinkPickerDialog entries={entries} onPick={(p) => void link(p)} onClose={() => setPicking(false)} />}
    </section>
  );
}
