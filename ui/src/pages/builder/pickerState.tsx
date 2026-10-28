import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";
import { NO_FILTER, type PickerFilter } from "../../lib/versions";

/** Per-picker UI state (B-55): the filters (sec. 1.5) and each row's last menu pick (sec. 1.2 rule 2). Never saved. */
export interface PickerUiState {
  filter: PickerFilter;
  /** Row key -> the version id last picked in that row's menu. */
  menuPicks: Readonly<Record<string, string>>;
}

export const EMPTY_PICKER_STATE: PickerUiState = { filter: NO_FILTER, menuPicks: {} };

type Update = (fn: (s: PickerUiState) => PickerUiState) => void;
interface Store { get: (key: string) => PickerUiState; update: (key: string, fn: (s: PickerUiState) => PickerUiState) => void }

const StoreCtx = createContext<Store | null>(null);

/** Keeps every picker's state while the page that owns it (the builder, the Magic page) stays open, across its tabs. */
export function PickerStateProvider({ children }: { children: ReactNode }) {
  const [all, setAll] = useState<Readonly<Record<string, PickerUiState>>>({});
  const get = useCallback((key: string) => all[key] ?? EMPTY_PICKER_STATE, [all]);
  const update = useCallback((key: string, fn: (s: PickerUiState) => PickerUiState) => {
    setAll((cur) => ({ ...cur, [key]: fn(cur[key] ?? EMPTY_PICKER_STATE) }));
  }, []);
  const store = useMemo(() => ({ get, update }), [get, update]);
  return <StoreCtx.Provider value={store}>{children}</StoreCtx.Provider>;
}

/** The state of one picker: from the page's provider, else local to the picker. */
export function usePickerUiState(key: string): [PickerUiState, Update] {
  const store = useContext(StoreCtx);
  const [local, setLocal] = useState<PickerUiState>(EMPTY_PICKER_STATE);
  const update = useCallback<Update>((fn) => (store ? store.update(key, fn) : setLocal(fn)), [store, key]);
  return [store ? store.get(key) : local, update];
}
