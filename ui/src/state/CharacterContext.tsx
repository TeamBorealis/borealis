import { type ReactNode, createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { Api, CharacterSnapshot } from "../api/contract";
import { useToast } from "../components/Toast";
import { onBundleApplied } from "../pages/transfer/openBundle";
import { useApi } from "./ApiContext";
import { useSettings } from "./SettingsContext";
import { describeAdopt, describeChange, setStatus } from "./status";
import { errorMessage } from "./useAsync";

const STORAGE_KEY = "lk.currentCharacter";
const AUTOSAVE_MS = 1500;

interface CharacterCtx {
  snapshot: CharacterSnapshot | null;
  /**
   * Goes up whenever the open character's build may have changed (opened, adopted, mutated, re-read); a save does not move
   * it. Lists that read the backend on their own (pick options) reload on it (GitHub #7).
   */
  revision: number;
  busy: boolean;
  open: (id: string, silent?: boolean) => Promise<boolean>;
  adopt: (s: CharacterSnapshot) => void;
  /** Runs a backend mutation, adopts the returned snapshot, toasts failures. Returns the snapshot or null. */
  mutate: (fn: (api: Api, characterId: string) => Promise<CharacterSnapshot>) => Promise<CharacterSnapshot | null>;
  /** Ctrl/Cmd+S, SAVE, File > Save: opens classic builder's Save flyout (nothing is written yet). */
  save: () => void;
  saveOpen: boolean;
  closeSave: () => void;
  /** Flyout's "Save character": applies filename/group/favourite, writes the file, reports "'NAME' Saved". */
  commitSave: (meta: { name: string; group: string; favorite: boolean }) => Promise<boolean>;
  /** False until the user saves (or loads from disk); autosave drafts never flip it. */
  savedByUser: boolean;
  close: (discard: boolean) => Promise<void>;
}

const Ctx = createContext<CharacterCtx | null>(null);

const remember = (id: string | null) => {
  try {
    if (id) sessionStorage.setItem(STORAGE_KEY, id);
    else sessionStorage.removeItem(STORAGE_KEY);
  } catch { /* storage unavailable */ }
};

export function CharacterProvider({ children, restore = true }: { children: ReactNode; restore?: boolean }) {
  const api = useApi();
  const toast = useToast();
  const { settings } = useSettings();
  const [snapshot, setSnapshot] = useState<CharacterSnapshot | null>(null);
  const [revision, setRevision] = useState(0);
  const [busy, setBusy] = useState(false);
  const [saveOpen, setSaveOpen] = useState(false);
  const [savedByUser, setSavedByUser] = useState(false);
  const idRef = useRef<string | null>(null);
  const snapshotRef = useRef<CharacterSnapshot | null>(null);
  snapshotRef.current = snapshot;

  const adopt = useCallback((s: CharacterSnapshot) => {
    idRef.current = s.id;
    remember(s.id);
    setSnapshot(s);
    setRevision((r) => r + 1);
    setSavedByUser(false);
    setStatus(describeAdopt(s));
  }, []);

  const open = useCallback(async (id: string, silent = false) => {
    setBusy(true);
    try {
      adopt(await api.loadCharacter({ characterId: id }));
      setSavedByUser(true); // a file loaded from disk already has its path
      return true;
    } catch (e) {
      if (silent) remember(null);
      else toast.push(errorMessage(e), "error");
      return false;
    } finally {
      setBusy(false);
    }
  }, [api, adopt, toast]);

  useEffect(() => {
    if (!restore) return;
    try {
      const id = sessionStorage.getItem(STORAGE_KEY);
      if (id) void open(id, true);
    } catch { /* ignore */ }
  }, [restore, open]);

  const mutate = useCallback<CharacterCtx["mutate"]>(async (fn) => {
    const id = idRef.current;
    if (!id) return null;
    setBusy(true);
    try {
      const next = await fn(api, id);
      const note = snapshotRef.current ? describeChange(snapshotRef.current, next) : null;
      if (note) setStatus(note);
      setSnapshot(next);
      setRevision((r) => r + 1);
      next.inventory.warnings.forEach((w) => toast.push(w, "warning"));
      return next;
    } catch (e) {
      toast.push(errorMessage(e), "error");
      return null;
    } finally {
      setBusy(false);
    }
  }, [api, toast]);

  const save = useCallback(() => { if (idRef.current) setSaveOpen(true); }, []);
  const closeSave = useCallback(() => setSaveOpen(false), []);

  const commitSave = useCallback(async (meta: { name: string; group: string; favorite: boolean }) => {
    const id = idRef.current;
    const cur = snapshotRef.current;
    if (!id || !cur) return false;
    try {
      let next = cur;
      const name = meta.name.trim() || cur.name;
      if (name !== cur.name || meta.group !== cur.group || meta.favorite !== cur.favorite) {
        next = await api.updateCharacterMeta({ characterId: id, name, group: meta.group, favorite: meta.favorite });
      }
      next = await api.saveCharacter({ characterId: id });
      if (idRef.current !== id) return false;
      setSnapshot(next);
      setSavedByUser(true);
      setStatus(`'${next.name}' Saved`);
      return true;
    } catch (e) {
      setStatus("Error on saving current character");
      toast.push(errorMessage(e), "error");
      return false;
    }
  }, [api, toast]);

  const close = useCallback(async (discard: boolean) => {
    const id = idRef.current;
    if (id) await api.closeCharacter({ characterId: id, discard }).catch(() => undefined);
    idRef.current = null;
    remember(null);
    setSnapshot(null);
    setSavedByUser(false);
    setSaveOpen(false);
    setStatus("");
  }, [api]);

  // A receive or an accepted bundle may have replaced the open character's file; the backend reloads its
  // session from that file, so re-read it here too (HW1 S9: the screen kept the old copy).
  useEffect(() => {
    const reread = () => {
      const id = idRef.current;
      if (!id) return;
      api.getSnapshot({ characterId: id }).then((s) => {
        if (idRef.current !== id) return;
        setSnapshot(s);
        setRevision((r) => r + 1);
      }).catch(() => undefined);
    };
    const offLink = api.subscribe("link:status", (s) => { if (s.role === "receive" && s.phase === "done") reread(); });
    const offBundle = onBundleApplied(reread);
    return () => { offLink(); offBundle(); };
  }, [api]);

  // Autosave draft (silent) after a quiet period.
  useEffect(() => {
    if (!settings.autosave || !snapshot?.dirty) return;
    const id = snapshot.id;
    const t = setTimeout(() => {
      api.saveCharacter({ characterId: id }).then((s) => idRef.current === id && setSnapshot(s)).catch(() => undefined);
    }, AUTOSAVE_MS);
    return () => clearTimeout(t);
  }, [api, settings.autosave, snapshot]);

  const value = useMemo(() => ({ snapshot, revision, busy, open, adopt, mutate, save, saveOpen, closeSave, commitSave, savedByUser, close }), [snapshot, revision, busy, open, adopt, mutate, save, saveOpen, closeSave, commitSave, savedByUser, close]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useCharacter(): CharacterCtx {
  const v = useContext(Ctx);
  if (!v) throw new Error("useCharacter must be used inside <CharacterProvider>");
  return v;
}
