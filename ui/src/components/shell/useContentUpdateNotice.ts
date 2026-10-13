import { useEffect } from "react";
import type { ContentUpdateStatus } from "../../api/contract";
import { useApi } from "../../state/ApiContext";
import { getStatus, setStatus } from "../../state/status";

const POLL_MS = 3_000;
/** The startup check waits 10 s, then fetches with 30 s timeouts: stop looking after about two minutes. */
const MAX_POLLS = 40;
const CHECKING = "Checking for content updates...";

/** Status-bar text for a finished check, or null when there is nothing to tell (the classic builder keeps its idle line). */
export function describeContentUpdate(s: ContentUpdateStatus): string | null {
  if (s.state === "failed") return s.message;
  if (s.state === "done" && s.updated > 0) return "Your content files have been updated and the new content is loaded.";
  if (s.state === "done" && s.failed.length > 0) return s.message;
  return null;
}

/** Surfaces the startup auto-update the way the classic builder does: "Checking..." while it runs, then the result in the status bar. */
export function useContentUpdateNotice(): void {
  const api = useApi();
  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let polls = 0;
    const look = async () => {
      let s: ContentUpdateStatus;
      try { s = await api.contentUpdateStatus(); } catch { return; }
      if (!alive) return;
      if (s.state === "running") {
        setStatus(CHECKING);
      } else if (s.state === "pending" || s.state === "idle") {
        if (++polls < MAX_POLLS) timer = setTimeout(() => void look(), POLL_MS);
        return;
      } else {
        const text = describeContentUpdate(s);
        if (text) setStatus(text);
        else if (getStatus() === CHECKING) setStatus("");
        return;
      }
      if (++polls < MAX_POLLS) timer = setTimeout(() => void look(), POLL_MS);
    };
    void look();
    const off = api.subscribe("content:progress", (p) => { if (p.phase === "checking" && p.message) setStatus(p.message); else if (p.phase === "done") void look(); });
    return () => { alive = false; clearTimeout(timer); off(); };
  }, [api]);
}
