import { useCallback, useEffect, useState } from "react";
import type { Api, VaultBacklink, VaultChange, VaultSearchHit, VaultTag, VaultTree } from "../../api/contract";
import { errorMessage } from "../../state/useAsync";
import { useAppLifecycle } from "./useAppLifecycle";

/** How long search waits after the last keystroke. */
const SEARCH_DELAY_MS = 250;

export interface VaultData {
  tree: VaultTree | null;
  error: string | null;
  /** Bumped on every change reported from disk (embeds and lists read again). */
  version: number;
  reload: () => Promise<void>;
  tags: VaultTag[];
}

/** Read everything again: what a phone does when the app comes back, since it has no watcher. */
export const RESCAN: VaultChange = { paths: [], all: true, config: true };

/**
 * The vault tree and tag list. On desktop the backend's watcher keeps them fresh (`vault:changed`). Phones have none (B-221):
 * the vault is read again when Notes opens and whenever the app comes back to the foreground, and the open note is checked
 * then, exactly as for a watcher's change (`onChange` with everything changed). `onBackground` runs when the app leaves.
 */
export function useVaultData(api: Api, onChange: (c: VaultChange) => void, phone: { mobile: boolean; onBackground: () => void } = { mobile: false, onBackground: () => undefined }): VaultData {
  const [tree, setTree] = useState<VaultTree | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);
  const [tags, setTags] = useState<VaultTag[]>([]);

  const reload = useCallback(async () => {
    try {
      const next = await api.vaultTree();
      setTree(next);
      setError(null);
      setTags(next.dir && !next.unavailable ? await api.vaultTags() : []);
    } catch (e) { setError(errorMessage(e)); }
  }, [api]);

  useEffect(() => { void reload(); }, [reload]);

  const live = !!tree?.dir && !tree.unavailable;
  useAppLifecycle(api, phone.mobile, () => {
    setVersion((v) => v + 1);
    void reload();
    onChange(RESCAN);
  }, phone.onBackground);
  useEffect(() => {
    if (!live || phone.mobile) return;
    void api.vaultWatch({ enabled: true }).catch(() => undefined);
    const off = api.subscribe("vault:changed", (c) => {
      setVersion((v) => v + 1);
      void reload();
      onChange(c);
    });
    return () => { off(); void api.vaultWatch({ enabled: false }).catch(() => undefined); };
  }, [api, live, reload, onChange, phone.mobile]);

  return { tree, error, version, reload, tags };
}

/** The open note's backlinks (null without a note), read again when the vault or the note changes. */
export function useBacklinks(api: Api, path: string | null, version: number, hash: string | null): VaultBacklink[] | null {
  const [links, setLinks] = useState<VaultBacklink[] | null>(null);
  useEffect(() => {
    let alive = true;
    if (!path) { setLinks(null); return; }
    api.vaultBacklinks({ path }).then((l) => alive && setLinks(l), () => alive && setLinks([]));
    return () => { alive = false; };
  }, [api, path, version, hash]);
  return links;
}

/** Search results for a query or a tag (null when neither is set). */
export function useVaultSearch(api: Api, query: string, tag: string | null, version: number): VaultSearchHit[] | null {
  const [hits, setHits] = useState<VaultSearchHit[] | null>(null);
  useEffect(() => {
    let alive = true;
    const q = query.trim();
    if (!q && !tag) { setHits(null); return; }
    const t = setTimeout(() => {
      api.vaultSearch({ query: q, ...(tag ? { tag } : {}) }).then((h) => alive && setHits(h), () => alive && setHits([]));
    }, tag ? 0 : SEARCH_DELAY_MS);
    return () => { alive = false; clearTimeout(t); };
  }, [api, query, tag, version]);
  return hits;
}
