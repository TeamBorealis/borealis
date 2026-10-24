import { useCallback, useEffect, useState } from "react";
import type { SemanticStatus } from "../api/contract";
import { useApi } from "./ApiContext";
import { useSettings } from "./SettingsContext";

/** Semantic model status for Settings > Search and the search flyout: refetched when the search settings change, kept
 * current by `semantic:download`. Reading it never makes a network request. */
export function useSemanticStatus(): { status: SemanticStatus | null; setStatus: (s: SemanticStatus) => void; refresh: () => Promise<void> } {
  const api = useApi();
  const { settings } = useSettings();
  const [status, setStatus] = useState<SemanticStatus | null>(null);
  const search = settings.search;
  const refresh = useCallback(async () => {
    try { setStatus(await api.semanticStatus()); } catch { /* status is optional UI; keyword search does not need it */ }
  }, [api]);
  useEffect(() => { void refresh(); }, [refresh, search?.semantic, search?.tierOverride]);
  useEffect(() => api.subscribe("semantic:download", (download) => {
    setStatus((s) => (s ? { ...s, download, modelPresent: s.modelPresent || download.state === "done" } : s));
    if (download.state === "done" || download.state === "idle") void refresh();
  }), [api, refresh]);
  return { status, setStatus, refresh };
}

const MB = 1_000_000;

/** "123 of 310 MB". */
export function megabytes(done: number, total: number): string {
  return `${Math.floor(done / MB)} of ${Math.round(total / MB)} MB`;
}
