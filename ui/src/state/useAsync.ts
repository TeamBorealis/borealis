import { type DependencyList, useCallback, useEffect, useRef, useState } from "react";
import { isApiError } from "../api/contract";

export interface AsyncState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  reload: () => void;
  setData: (d: T) => void;
}

export const errorMessage = (e: unknown): string => (isApiError(e) ? e.message : e instanceof Error ? e.message : String(e));

/** Loads data when deps change; ignores stale responses. */
export function useAsync<T>(load: () => Promise<T>, deps: DependencyList): AsyncState<T> {
  const [data, setDataState] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);
  const seq = useRef(0);
  useEffect(() => {
    const mine = ++seq.current;
    setLoading(true);
    load().then(
      (d) => { if (mine === seq.current) { setDataState(d); setError(null); setLoading(false); } },
      (e) => { if (mine === seq.current) { setError(errorMessage(e)); setLoading(false); } },
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);
  const reload = useCallback(() => setTick((t) => t + 1), []);
  return { data, error, loading, reload, setData: setDataState };
}

export function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}
