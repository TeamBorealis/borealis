import { useEffect, useState, useSyncExternalStore } from "react";
import { type TrialApi, TrialSession, type TrialState } from "./trialSession";

const IDLE: TrialState = { view: null, busy: false, error: null, notice: null, revision: 0 };
const noSubscribe = () => () => undefined;
const idle = () => IDLE;

/**
 * A Try it session for one draft while the flyout is open: started when `draftId` is set, ended on close, on a draft
 * switch and on Start over (`generation` goes up). Each start is its own session, so a late answer of an ended one is
 * dropped (and its trial ended).
 */
export function useTrial(api: TrialApi, draftId: string | null, generation: number): { session: TrialSession | null; state: TrialState } {
  const [session, setSession] = useState<TrialSession | null>(null);
  useEffect(() => {
    if (!draftId) return undefined;
    const s = new TrialSession(api, draftId);
    setSession(s);
    void s.start();
    return () => { void s.end(); };
  }, [api, draftId, generation]);
  const state = useSyncExternalStore(session?.subscribe ?? noSubscribe, session?.getSnapshot ?? idle, session?.getSnapshot ?? idle);
  return { session, state };
}
