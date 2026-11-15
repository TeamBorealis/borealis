import { useCallback } from "react";
import type { AppSettings, Ruleset } from "../api/contract";
import { copyKey, type PreferredCopies, type VersionLike } from "../lib/versions";
import { useSettings } from "./SettingsContext";

/** The `saveSettings` patch that sets (`id`) or clears (`null`) the "always use this copy" choice of these keys (#51). */
export function preferredCopiesPatch(keys: readonly string[], id: string | null): Partial<AppSettings> {
  // a null value removes the entry from the stored map (see AppSettings.preferredCopies)
  const entries = Object.fromEntries(keys.map((k) => [k, id]));
  return { preferredCopies: entries as Record<string, string> };
}

export interface PreferredCopiesState {
  preferred: PreferredCopies;
  /** Remembers `v` as the copy always used for its name in these editions; `always: false` forgets the choice. */
  setAlways: (v: Pick<VersionLike, "id" | "name" | "type">, editions: readonly Ruleset[], always: boolean) => Promise<void>;
}

/** GitHub #51: the "always use this copy" choices from the app settings, and the way to change one. */
export function usePreferredCopies(): PreferredCopiesState {
  const { settings, update } = useSettings();
  const setAlways = useCallback(async (v: Pick<VersionLike, "id" | "name" | "type">, editions: readonly Ruleset[], always: boolean) => {
    await update(preferredCopiesPatch(editions.map((ed) => copyKey(v, ed)), always ? v.id : null));
  }, [update]);
  return { preferred: settings.preferredCopies ?? {}, setAlways };
}
