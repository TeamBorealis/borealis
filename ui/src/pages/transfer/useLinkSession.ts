import { useCallback, useEffect, useRef, useState } from "react";
import type { Api, LinkStatus } from "../../api/contract";
import { isNewer } from "./messages";

const TICK_MS = 1000;
/** Events of sessions not adopted yet (an event can beat the command's reply). */
const MAX_EARLY = 8;

export interface LinkSession {
  status: LinkStatus | null;
  /** Seconds left on the shown QR / code (counts down locally). */
  remaining: number | null;
  /** Adopt a command reply (start, connect, accept, ...). */
  adopt: (s: LinkStatus) => void;
  reset: () => void;
}

/** One Send/Receive session: replies and `link:status` events for it, newest phase wins. */
export function useLinkSession(api: Api): LinkSession {
  const [status, setStatus] = useState<LinkStatus | null>(null);
  const [remaining, setRemaining] = useState<number | null>(null);
  const idRef = useRef<string | null>(null);
  const early = useRef(new Map<string, LinkStatus>());

  const take = useCallback((next: LinkStatus) => setStatus((cur) => (isNewer(cur, next) ? next : cur)), []);
  const adopt = useCallback((next: LinkStatus) => {
    idRef.current = next.sessionId;
    const seen = early.current.get(next.sessionId);
    early.current.clear();
    setStatus((cur) => {
      const best = seen && isNewer(next, seen) ? seen : next;
      return isNewer(cur, best) ? best : cur;
    });
  }, []);
  const reset = useCallback(() => { idRef.current = null; early.current.clear(); setStatus(null); setRemaining(null); }, []);

  useEffect(() => api.subscribe("link:status", (s) => {
    if (s.sessionId === idRef.current) { take(s); return; }
    const map = early.current;
    map.set(s.sessionId, s);
    if (map.size > MAX_EARLY) map.delete(map.keys().next().value as string);
  }), [api, take]);

  const expires = status?.phase === "waiting" ? status.expiresInSecs : null;
  useEffect(() => {
    if (expires === null || expires === undefined) { setRemaining(null); return; }
    const end = Date.now() + expires * 1000;
    setRemaining(expires);
    const t = setInterval(() => setRemaining(Math.max(0, Math.round((end - Date.now()) / 1000))), TICK_MS);
    return () => clearInterval(t);
  }, [expires, status?.sessionId]);

  return { status, remaining, adopt, reset };
}
