/**
 * Unsaved SPELL LISTS checks kept while the workspace is left (P3, spec 10.1: leaving keeps what is pending, nothing is
 * lost and nothing is asked). Per backend and pack, for this run of the app: the list chosen and its checks. The
 * workspace opens SPELL LISTS on them when it comes back to the pack; Save or Discard there forgets them.
 */
export interface KeptChecks {
  /** The list's chooser value (a list tag, or `expanded:<subclass id>`). */
  readonly list: string;
  /** Spell id -> on the list (true) or off it (false). */
  readonly pending: ReadonlyMap<string, boolean>;
}

const kept = new WeakMap<object, Map<string, KeptChecks>>();

const of = (api: object): Map<string, KeptChecks> => {
  const found = kept.get(api);
  if (found) return found;
  const made = new Map<string, KeptChecks>();
  kept.set(api, made);
  return made;
};

/** The checks kept for `packId`, if any. */
export function keptChecks(api: object, packId: string): KeptChecks | null {
  return of(api).get(packId) ?? null;
}

/** Keep the pack's unsaved checks (none: forget them). */
export function keepChecks(api: object, packId: string, list: string, pending: ReadonlyMap<string, boolean>): void {
  if (list && pending.size > 0) of(api).set(packId, { list, pending });
  else of(api).delete(packId);
}
