import type { AppSettings, ContentSource } from "../../api/contract";

/** The two Sources page toggles: restrict every playtest book, or every book not marked official. */
export type SourceKind = "playtest" | "thirdParty";

export const KIND_SETTING: Record<SourceKind, "restrictPlaytest" | "restrictThirdParty"> = {
  playtest: "restrictPlaytest",
  thirdParty: "restrictThirdParty",
};

export const KIND_LABEL: Record<SourceKind, string> = { playtest: "playtest", thirdParty: "third-party" };

export type SourceToggles = Pick<AppSettings, "restrictPlaytest" | "restrictThirdParty">;

/** Is `s` a book of this kind? Older backends send no `thirdParty`; such books count as official. */
export function isKind(s: Pick<ContentSource, "playtest" | "thirdParty">, kind: SourceKind): boolean {
  return kind === "playtest" ? s.playtest : s.thirdParty === true;
}

/** Ids of the books of `kind`. */
export function idsOfKind(sources: readonly ContentSource[], kind: SourceKind): string[] {
  return sources.filter((s) => isKind(s, kind)).map((s) => s.id);
}

/** Books of `kind` that `allowed` leaves out. */
export function restrictedCount(sources: readonly ContentSource[], kind: SourceKind, allowed: ReadonlySet<string>): number {
  return sources.filter((s) => isKind(s, kind) && !allowed.has(s.id)).length;
}

/** "23 playtest sources restricted" (singular for one). */
export function restrictedNote(count: number, kind: SourceKind): string {
  return `${count} ${KIND_LABEL[kind]} source${count === 1 ? "" : "s"} restricted`;
}

/**
 * A restriction (null = every source) with the books of each switched-on kind left out. The result is null again when
 * it allows every installed source, as the Sources page stores it.
 */
export function withToggles(restriction: readonly string[] | null, sources: readonly ContentSource[], toggles: Partial<SourceToggles>): string[] | null {
  const allowed = new Set(restriction ?? sources.map((s) => s.id));
  for (const kind of ["playtest", "thirdParty"] as const) {
    if (!toggles[KIND_SETTING[kind]]) continue;
    for (const id of idsOfKind(sources, kind)) allowed.delete(id);
  }
  const installed = sources.map((s) => s.id);
  return installed.every((id) => allowed.has(id)) && allowed.size === installed.length ? null : [...allowed];
}
