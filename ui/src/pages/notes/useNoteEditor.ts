import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Api, VaultChange, VaultNote } from "../../api/contract";
import { isApiError } from "../../api/contract";
import { AUTOSAVE_MS } from "../../state/CharacterContext";
import { errorMessage } from "../../state/useAsync";

/** The open note: what was loaded (the base for the next safe write) and the user's draft. */
export interface EditorState {
  /** The note as last loaded or saved; its `hash` guards the next write. */
  note: VaultNote | null;
  /** The editor text (LF line breaks, as a browser text box holds them). */
  draft: string;
  dirty: boolean;
  saving: boolean;
  /** The note changed on disk while the user had edits: what is there now. Nothing is written until the user chooses. */
  conflict: VaultNote | null;
  /** The note was deleted on disk while the user had edits. */
  missing: boolean;
  error: string | null;
  /** A short line about what just happened (reloaded from disk). */
  notice: string | null;
}

export interface NoteEditor extends EditorState {
  /** Save the open note, then open `path`. False when the open note could not be saved (it stays open). */
  open: (path: string) => Promise<boolean>;
  edit: (text: string) => void;
  /** Save now (explicit save, or before leaving the note). True when nothing is left unsaved. */
  save: () => Promise<boolean>;
  close: () => Promise<boolean>;
  /** Drop the open note without saving (it was deleted, or the user closed it after a delete elsewhere). */
  discard: () => void;
  /** Another app changed files: reload the note, or flag a conflict when the user has edits. */
  external: (change: VaultChange) => Promise<void>;
  keepMine: () => Promise<void>;
  takeTheirs: () => void;
  /** After a delete on disk: write the draft as a new file at the same path. */
  recreate: () => Promise<void>;
}

const EMPTY: EditorState = { note: null, draft: "", dirty: false, saving: false, conflict: null, missing: false, error: null, notice: null };

/** CRLF to LF: what a text box shows; the backend puts the note's own line ending back on save. */
export const lf = (text: string): string => text.replace(/\r\n/g, "\n");

/**
 * The notes view's editor state. Writes happen only on a user change, after `AUTOSAVE_MS` of quiet when autosave is on
 * (the same convention as characters) or on an explicit save, and only while the file still hashes to what was loaded.
 */
export function useNoteEditor(api: Api, autosave: boolean): NoteEditor {
  const [state, setState] = useState<EditorState>(EMPTY);
  const ref = useRef(state);
  ref.current = state;

  /** Write the draft of `s` against its base hash. */
  const write = useCallback(async (s: EditorState): Promise<boolean> => {
    if (!s.note || !s.dirty) return true;
    if (s.note.readOnly || s.conflict || s.missing || s.saving) return false;
    const { path, hash } = s.note;
    const text = s.draft;
    setState((cur) => ({ ...cur, saving: true, error: null }));
    try {
      const r = await api.vaultSave({ path, text, baseHash: hash });
      setState((cur) => {
        if (cur.note?.path !== path) return cur;
        if (r.status === "conflict") return { ...cur, saving: false, conflict: r.disk };
        if (r.status === "missing") return { ...cur, saving: false, missing: true };
        const note = { ...cur.note, text, hash: r.hash ?? cur.note.hash, mtimeMs: r.mtimeMs ?? cur.note.mtimeMs };
        return { ...cur, saving: false, note, dirty: cur.draft !== text, notice: null };
      });
      return r.status === "saved";
    } catch (e) {
      setState((cur) => ({ ...cur, saving: false, error: errorMessage(e) }));
      return false;
    }
  }, [api]);

  const save = useCallback(() => write(ref.current), [write]);

  const open = useCallback(async (path: string) => {
    if (!(await save())) return false;
    try {
      const note = await api.vaultRead({ path });
      setState({ ...EMPTY, note, draft: lf(note.text) });
    } catch (e) {
      setState({ ...EMPTY, error: errorMessage(e) });
    }
    return true;
  }, [api, save]);

  const discard = useCallback(() => setState(EMPTY), []);

  const edit = useCallback((text: string) => {
    setState((cur) => (cur.note && !cur.note.readOnly ? { ...cur, draft: text, dirty: text !== lf(cur.note.text), notice: null } : cur));
  }, []);

  const close = useCallback(async () => {
    const ok = await save();
    if (ok) setState(EMPTY);
    return ok;
  }, [save]);

  const external = useCallback(async (change: VaultChange) => {
    const s = ref.current;
    if (!s.note || (!change.all && !change.paths.includes(s.note.path))) return;
    const { path } = s.note;
    try {
      const disk = await api.vaultRead({ path });
      setState((cur) => {
        if (cur.note?.path !== path || disk.hash === cur.note.hash) return cur;
        if (!cur.dirty) return { ...EMPTY, note: disk, draft: lf(disk.text), notice: "This note changed in another app and was reloaded." };
        return { ...cur, conflict: disk };
      });
    } catch (e) {
      if (!isApiError(e) || e.code !== "not_found") return;
      setState((cur) => {
        if (cur.note?.path !== path) return cur;
        return cur.dirty ? { ...cur, missing: true } : { ...EMPTY, notice: "The note was deleted or moved in another app." };
      });
    }
  }, [api]);

  const keepMine = useCallback(async () => {
    const s = ref.current;
    if (!s.note || !s.conflict) return;
    // Mine now replaces what is on disk: the disk version becomes the base the write is checked against.
    const next = { ...s, note: s.conflict, conflict: null, dirty: true };
    setState((cur) => ({ ...cur, note: next.note, conflict: null, dirty: true }));
    await write(next);
  }, [write]);

  const takeTheirs = useCallback(() => {
    setState((cur) => (cur.conflict ? { ...EMPTY, note: cur.conflict, draft: lf(cur.conflict.text) } : cur));
  }, []);

  const recreate = useCallback(async () => {
    const s = ref.current;
    if (!s.note || !s.missing) return;
    try {
      const r = await api.vaultSave({ path: s.note.path, text: s.draft });
      const note = await api.vaultRead({ path: s.note.path });
      setState({ ...EMPTY, note, draft: lf(note.text), notice: r.status === "saved" ? null : "The note could not be saved again." });
    } catch (e) {
      setState((cur) => ({ ...cur, error: errorMessage(e) }));
    }
  }, [api]);

  // Autosave after a quiet period, like the character autosave.
  useEffect(() => {
    if (!autosave || !state.dirty || state.conflict || state.missing || state.saving) return;
    const t = setTimeout(() => void save(), AUTOSAVE_MS);
    return () => clearTimeout(t);
  }, [autosave, state.dirty, state.draft, state.conflict, state.missing, state.saving, save]);

  return useMemo(() => ({ ...state, open, edit, save, close, discard, external, keepMine, takeTheirs, recreate }), [state, open, edit, save, close, discard, external, keepMine, takeTheirs, recreate]);
}
