import { useCallback, useSyncExternalStore } from "react";
import type { Api } from "../api/contract";
import type { EditionFilter } from "../lib/versions";
import { useApi } from "./ApiContext";
import { useCharacter } from "./CharacterContext";

/**
 * TV1 (A4, B-115): the Edition filter of every picker and of the equipment add list. It starts on the character's edition
 * and a change carries to every other list of the same character while the app runs (never saved: U10). Held per host, so
 * each host (and each test's mock) starts fresh.
 */
interface Store {
  values: Map<string, EditionFilter>;
  listeners: Set<() => void>;
}

const stores = new WeakMap<Api, Store>();

function storeOf(api: Api): Store {
  const found = stores.get(api);
  if (found) return found;
  const made: Store = { values: new Map(), listeners: new Set() };
  stores.set(api, made);
  return made;
}

export interface EditionFilterState {
  /** The edition the lists show: the last one picked for this character, else its own edition. */
  edition: EditionFilter;
  /** The character's own edition: where Clear returns to. "all" when no character is open. */
  initial: EditionFilter;
  setEdition: (e: EditionFilter) => void;
}

/** The character's shared Edition filter (TV1 A4). Without an open character it starts on "all". */
export function useEditionFilter(): EditionFilterState {
  const api = useApi();
  const { snapshot } = useCharacter();
  const store = storeOf(api);
  const key = snapshot?.id ?? "";
  const initial: EditionFilter = snapshot?.ruleset ?? "all";
  const subscribe = useCallback((fn: () => void) => {
    store.listeners.add(fn);
    return () => { store.listeners.delete(fn); };
  }, [store]);
  const kept = useSyncExternalStore(subscribe, () => store.values.get(key));
  const setEdition = useCallback((e: EditionFilter) => {
    store.values.set(key, e);
    store.listeners.forEach((fn) => fn());
  }, [store, key]);
  return { edition: kept ?? initial, initial, setEdition };
}
