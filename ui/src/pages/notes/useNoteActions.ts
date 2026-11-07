import { useState } from "react";
import type { Api, VaultRenamePlan } from "../../api/contract";
import type { ConfirmOptions } from "../../components";
import { setStatus } from "../../state/status";
import { errorMessage } from "../../state/useAsync";
import type { NoteMode } from "./NotePane";
import type { ClickedLink } from "./NotePreview";
import { baseName, displayName } from "./noteNames";
import type { NoteEditor } from "./useNoteEditor";

type Confirm = (o: ConfirmOptions) => Promise<string>;
export type OpenPath = (path: string, opts?: { subpath?: string | null; mode?: NoteMode }) => Promise<void>;

/** The rename flow: a name first, then (when links point at the file) the link changes to confirm. */
export interface RenameState {
  path: string;
  /** The name the user typed (sent as is; the backend keeps the extension). */
  name: string;
  error: string | null;
  plan: VaultRenamePlan | null;
}

export interface NoteActions {
  rename: RenameState | null;
  startRename: (path: string) => void;
  cancelRename: () => void;
  /** The user typed the new name: show the link changes, or rename at once when there are none. */
  submitName: (name: string) => Promise<void>;
  /** The user confirmed the rename (with or without link updates). */
  doRename: (updateLinks: boolean) => Promise<void>;
  createNote: (folder?: string) => Promise<void>;
  deleteFile: (path: string) => Promise<void>;
  followLink: (link: ClickedLink) => Promise<void>;
}

interface Deps {
  api: Api;
  editor: NoteEditor;
  confirm: Confirm;
  selected: string | null;
  reload: () => Promise<void>;
  openPath: OpenPath;
  setSelected: (path: string | null) => void;
  setScrollTo: (sub: string | null) => void;
}

/** Create, rename, delete and link-following for the notes view. Every write goes through a backend command. */
export function useNoteActions({ api, editor, confirm, selected, reload, openPath, setSelected, setScrollTo }: Deps): NoteActions {
  const [rename, setRename] = useState<RenameState | null>(null);

  const report = (e: unknown) => setStatus(errorMessage(e));

  const createNote = async (folder?: string) => {
    try {
      const note = await api.vaultCreateNote({ ...(selected ? { from: selected } : {}), ...(folder !== undefined ? { folder } : {}) });
      await reload();
      await openPath(note.path, { mode: "edit" });
    } catch (e) { report(e); }
  };

  const followLink = async (link: ClickedLink) => {
    try {
      if (!link.target) {
        if (link.from === selected) setScrollTo(link.subpath);
        else await openPath(link.from, { subpath: link.subpath });
        return;
      }
      const [to] = await api.vaultResolve({ from: link.from, targets: [link.target] });
      if (to) { await openPath(to, { subpath: link.subpath }); return; }
      const name = displayName(baseName(link.target));
      const answer = await confirm({ title: "Create note?", body: `No note called ${name} is in your vault yet. Create it?`, confirmLabel: "Create note" });
      if (answer !== "confirm") return;
      const note = await api.vaultCreateNote({ name: link.target, from: link.from });
      await reload();
      await openPath(note.path, { mode: "edit" });
    } catch (e) { report(e); }
  };

  const doRename = async (updateLinks: boolean) => {
    if (!rename) return;
    const { path, name: newName } = rename;
    setRename(null);
    try {
      if (!(await editor.save())) return;
      const done = await api.vaultRename({ path, newName, updateLinks });
      await reload();
      if (selected === path) await openPath(done.to);
      else if (done.updated.includes(selected ?? "")) await openPath(selected!);
      const skipped = done.skipped.length ? ` ${done.skipped.length} note(s) changed meanwhile and kept their links.` : "";
      setStatus(`Renamed to ${displayName(baseName(done.to))}.${updateLinks ? ` Links updated in ${done.updated.length} note(s).` : ""}${skipped}`);
    } catch (e) { report(e); }
  };

  const submitName = async (name: string) => {
    if (!rename) return;
    try {
      const plan = await api.vaultRenamePreview({ path: rename.path, newName: name });
      if (plan.files.length > 0) { setRename({ ...rename, name, plan, error: null }); return; }
      // Nothing links to it: rename at once, once the open note is saved.
      if (!(await editor.save())) { setRename({ ...rename, name, plan: null, error: "Save or resolve the open note first." }); return; }
      setRename({ ...rename, name, plan, error: null });
      const done = await api.vaultRename({ path: rename.path, newName: name, updateLinks: false });
      setRename(null);
      await reload();
      if (selected === rename.path) await openPath(done.to);
      setStatus(`Renamed to ${displayName(baseName(done.to))}.`);
    } catch (e) { setRename({ ...rename, plan: null, error: errorMessage(e) }); }
  };

  const deleteFile = async (path: string) => {
    const name = displayName(baseName(path));
    try {
      const ok = await confirm({ title: "Delete?", body: `Move ${name} to the trash?`, confirmLabel: "Delete", danger: true });
      if (ok !== "confirm") return;
      let status = await api.vaultDelete({ path, permanent: false });
      if (status === "trash-unavailable") {
        const sure = await confirm({ title: "Delete for good?", body: `This system has no trash for ${name}. Delete it for good? This cannot be undone.`, confirmLabel: "Delete for good", danger: true });
        if (sure !== "confirm") return;
        status = await api.vaultDelete({ path, permanent: true });
      }
      if (selected === path) { editor.discard(); setSelected(null); }
      await reload();
      setStatus(status === "trashed" ? `${name} was moved to the trash.` : `${name} was deleted.`);
    } catch (e) { report(e); }
  };

  return {
    rename,
    startRename: (path) => setRename({ path, name: "", error: null, plan: null }),
    cancelRename: () => setRename(null),
    submitName, doRename, createNote, deleteFile, followLink,
  };
}
