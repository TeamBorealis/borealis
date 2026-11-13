import type { Api, HomebrewKeptChecks } from "../../api/contract";

/**
 * Unsaved SPELL LISTS checks kept while the workspace is left (P3, spec 10.1: leaving keeps what is pending, nothing is
 * lost and nothing is asked). Per backend and pack: the list chosen and its checks. They are kept on the device like a
 * draft (`homebrewKeepSpellListChecks`, INW5c), so a restart keeps them too; this module holds what the backend has in
 * memory for the app's run, so the workspace opens SPELL LISTS on them at once when it comes back to the pack. Save or
 * Discard there forgets them.
 */
export interface KeptChecks {
  /** The list's chooser value (a list tag, or `expanded:<subclass id>`). */
  readonly list: string;
  /** Spell id -> on the list (true) or off it (false). */
  readonly pending: ReadonlyMap<string, boolean>;
}

/** Per backend: each pack read from (or written to) the device this run, with its checks or none. */
const known = new WeakMap<object, Map<string, KeptChecks | null>>();

const of = (api: object): Map<string, KeptChecks | null> => {
  const found = known.get(api);
  if (found) return found;
  const made = new Map<string, KeptChecks | null>();
  known.set(api, made);
  return made;
};

const fromWire = (k: HomebrewKeptChecks | null): KeptChecks | null =>
  k && k.list && k.pending.length ? { list: k.list, pending: new Map(k.pending.map((c) => [c.id, c.on])) } : null;

const same = (a: KeptChecks | null, b: KeptChecks | null): boolean =>
  a === b || (!!a && !!b && a.list === b.list && a.pending.size === b.pending.size && [...a.pending].every(([id, on]) => b.pending.get(id) === on));

/** The checks kept for `packId` that this run knows of (read from the device or kept since), if any. */
export function keptChecks(api: object, packId: string): KeptChecks | null {
  return of(api).get(packId) ?? null;
}

/** Has this run read the pack's kept checks from the device (or kept some since)? */
export function keptKnown(api: object, packId: string): boolean {
  return of(api).has(packId);
}

/** The checks kept for `packId`, read from the device the first time this run asks. */
export async function loadKeptChecks(api: Api, packId: string): Promise<KeptChecks | null> {
  const memory = of(api);
  if (memory.has(packId)) return memory.get(packId) ?? null;
  const read = fromWire(await api.homebrewKeptSpellListChecks({ packId }));
  // a keep made while the device was read wins
  if (!memory.has(packId)) memory.set(packId, read);
  return memory.get(packId) ?? null;
}

/**
 * Keep the pack's unsaved checks (none: forget them), in memory and on the device. Before the device was read this run,
 * an empty keep is not written: it would forget checks this run never saw.
 */
export function keepChecks(api: Api, packId: string, list: string, pending: ReadonlyMap<string, boolean>): Promise<void> {
  const memory = of(api);
  const next: KeptChecks | null = list && pending.size > 0 ? { list, pending } : null;
  const unread = !memory.has(packId);
  if ((unread && !next) || (!unread && same(memory.get(packId) ?? null, next))) return Promise.resolve();
  memory.set(packId, next);
  const kept: HomebrewKeptChecks = { list: next?.list ?? "", pending: next ? [...next.pending].map(([id, on]) => ({ id, on })) : [] };
  return api.homebrewKeepSpellListChecks({ packId, kept });
}

/** Forget the pack's kept checks (Save, Discard), in memory and on the device, whether or not this run read them. */
export function forgetChecks(api: Api, packId: string): Promise<void> {
  of(api).set(packId, null);
  return api.homebrewKeepSpellListChecks({ packId, kept: { list: "", pending: [] } });
}
