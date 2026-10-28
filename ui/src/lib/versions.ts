import type { Ruleset, SlotOption, SlotOptionBase } from "../api/contract";

/**
 * B-55 "one row per name" (specs/version-rows-and-filters.md sec. 1). Pure helpers shared by the pickers and the mock host:
 * the group key, the default version order, the shown version, the version menu sections and the picker filters.
 */

/** A version as the pickers hold it: a chosen element the option list omits carries only its id, name and type. */
export type VersionLike = Pick<SlotOptionBase, "id" | "name"> & Partial<SlotOptionBase>;

/** The edition a version was written for; "any" = edition-neutral (both rulesets). */
export type EditionKey = Ruleset | "any";

const SINGLE_QUOTES = /[‘’‚‛′ʼ]/g;
const DOUBLE_QUOTES = /[“”„‟″]/g;

/** Sec. 1.1: NFC, trimmed, inner whitespace collapsed, case-folded, typographic quotes folded to ASCII. Nothing else. */
export function normalizeName(name: string): string {
  return name.normalize("NFC").trim().replace(/\s+/g, " ").toLowerCase().replace(SINGLE_QUOTES, "'").replace(DOUBLE_QUOTES, '"');
}

/** Race and Species are one family (likewise Subrace and Subspecies); every other type is its own family. */
export function typeFamily(type: string | undefined): string {
  const t = (type ?? "").toLowerCase();
  if (t === "race" || t === "species") return "race";
  if (t === "subrace" || t === "subspecies") return "subrace";
  return t;
}

/** Sec. 1.1 group key: type family + normalized name. */
export function groupKey(v: Pick<VersionLike, "name" | "type">): string {
  return `${typeFamily(v.type)}\u0000${normalizeName(v.name)}`;
}

/** The edition the version was written for: one ruleset, else edition-neutral (an `edition` mark counts when rulesets are absent). */
export function editionOf(v: VersionLike): EditionKey {
  const rs = v.rulesets ?? [];
  if (rs.length === 1) return rs[0]!;
  if (rs.length === 0 && v.edition) return v.edition;
  return "any";
}

export const yearOf = (ed: Ruleset): "2014" | "2024" => (ed === "dnd2024" ? "2024" : "2014");
export const otherEdition = (ed: Ruleset): Ruleset => (ed === "dnd2024" ? "dnd2014" : "dnd2024");

/** The character's edition when the page does not know it: an option marked for the other edition tells it. */
export function inferCharEdition(versions: VersionLike[], fallback: Ruleset = "dnd2024"): Ruleset {
  const marked = versions.find((v) => v.edition);
  return marked?.edition ? otherEdition(marked.edition) : fallback;
}

const byText = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0);

/**
 * Sec. 1.2 rule 3, the default version order: the character's edition first (edition-neutral counts as the character's),
 * then the other edition; within that imported before bundled, then source name A-Z, then element id.
 */
export function compareVersions(charEd: Ruleset): (a: VersionLike, b: VersionLike) => number {
  const own = (v: VersionLike) => (editionOf(v) === "any" || editionOf(v) === charEd ? 0 : 1);
  return (a, b) => own(a) - own(b)
    || Number(a.bundled === true) - Number(b.bundled === true)
    || byText(a.sourceName ?? "", b.sourceName ?? "")
    || byText(a.id, b.id);
}

export function sortVersions<V extends VersionLike>(versions: readonly V[], charEd: Ruleset): V[] {
  return [...versions].sort(compareVersions(charEd));
}

/** Sec. 1.2: the chosen version, else the last menu pick, else the default. */
export function pickShown<V extends VersionLike>(versions: readonly V[], charEd: Ruleset, chosenId: string | null, menuPickId: string | null): V {
  return versions.find((v) => v.id === chosenId) ?? versions.find((v) => v.id === menuPickId) ?? sortVersions(versions, charEd)[0]!;
}

export interface MenuSection<V> { edition: EditionKey; heading: string; versions: V[] }

/** Sec. 1.4: one section per edition, character's edition, other edition, edition-neutral; each in the default order. */
export function menuSections<V extends VersionLike>(versions: readonly V[], charEd: Ruleset): MenuSection<V>[] {
  const order: EditionKey[] = [charEd, otherEdition(charEd), "any"];
  const sorted = sortVersions(versions, charEd);
  return order
    .map((edition) => ({
      edition,
      heading: edition === "any" ? "Any edition" : `${yearOf(edition)} rules${edition === charEd ? " (your character)" : ""}`,
      versions: sorted.filter((v) => editionOf(v) === edition),
    }))
    .filter((s) => s.versions.length > 0);
}

/** " (2014)" / " (2024)" after the source of a version written for the other edition, unless the source names that year. */
export function editionSuffix(edition: Ruleset | undefined | null, sourceName: string | undefined): string {
  if (!edition) return "";
  const year = yearOf(edition);
  return (sourceName ?? "").includes(year) ? "" : ` (${year})`;
}

/* ---------- filters (sec. 1.5) ---------- */

export type EditionFilter = "all" | Ruleset;

export interface PickerFilter {
  text: string;
  edition: EditionFilter;
  /** Source ids to keep; null keeps every source. */
  sources: readonly string[] | null;
}

export const NO_FILTER: PickerFilter = { text: "", edition: "all", sources: null };

export const isFiltering = (f: PickerFilter): boolean => f.text.trim() !== "" || f.edition !== "all" || f.sources !== null;

/** A version passes the Edition filter when written for that edition or edition-neutral, and the Source filter by its source id. */
export function versionPasses(v: VersionLike, f: PickerFilter): boolean {
  const ed = editionOf(v);
  if (f.edition !== "all" && ed !== "any" && ed !== f.edition) return false;
  return f.sources === null || f.sources.includes(v.source ?? "");
}

/** Search matches the row name (normalized, substring). */
export const nameMatches = (name: string, text: string): boolean => normalizeName(name).includes(normalizeName(text));

/* ---------- rows ---------- */

export interface VersionRow<V extends VersionLike = VersionLike> {
  key: string;
  name: string;
  /** Every version the host offers (plus a chosen one it omits). */
  all: V[];
}

/** The versions of one host row: its `versions`, or the row itself. */
export function versionsOfOption(o: SlotOption): SlotOptionBase[] {
  if (o.versions && o.versions.length > 0) return o.versions;
  const { versions: _v, ...own } = o;
  return [own];
}

/**
 * Builds one row per group key from the host rows (merging any rows that share a key), then adds a chosen element the
 * list omits: into its same-name row when there is one, else as its own row at its alphabetical place.
 */
export function buildRows(options: readonly SlotOption[], chosen: VersionLike | null): VersionRow[] {
  const map = new Map<string, VersionRow>();
  for (const o of options) {
    const key = groupKey(o);
    const prev = map.get(key);
    const vs: VersionLike[] = versionsOfOption(o);
    map.set(key, prev ? { ...prev, all: [...prev.all, ...vs.filter((v) => !prev.all.some((p) => p.id === v.id))] } : { key, name: o.name, all: vs });
  }
  if (chosen && ![...map.values()].some((r) => r.all.some((v) => v.id === chosen.id))) {
    const key = groupKey(chosen);
    const prev = map.get(key);
    map.set(key, prev ? { ...prev, all: [...prev.all, chosen] } : { key, name: chosen.name, all: [chosen] });
  }
  return [...map.values()].sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * One row per group key over a flat list (B-55 W3: the add-item list, whose host answers one entry per element), in the
 * order each key first appears.
 */
export function groupVersions<V extends VersionLike & Pick<SlotOptionBase, "type">>(list: readonly V[]): VersionRow<V>[] {
  const map = new Map<string, VersionRow<V>>();
  for (const v of list) {
    const key = groupKey(v);
    const prev = map.get(key);
    map.set(key, prev ? { ...prev, all: [...prev.all, v] } : { key, name: v.name, all: [v] });
  }
  return [...map.values()];
}

export interface ShownRow<V extends VersionLike = VersionLike> extends VersionRow<V> {
  /** The versions that pass the filters (the chosen one always), the menu's entries. */
  versions: V[];
  shown: V;
  chosen: V | null;
}

/**
 * Applies the filters to the rows (sec. 1.5: a row stays if one version passes; the chosen row always stays, with its
 * chosen version) and picks each row's shown version among what is left (sec. 1.2).
 */
export function visibleRows<V extends VersionLike>(rows: readonly VersionRow<V>[], f: PickerFilter, charEd: Ruleset, chosenId: string | null, menuPicks: Readonly<Record<string, string>>): ShownRow<V>[] {
  const out: ShownRow<V>[] = [];
  for (const r of rows) {
    const chosen = r.all.find((v) => v.id === chosenId) ?? null;
    const passing = nameMatches(r.name, f.text) ? r.all.filter((v) => versionPasses(v, f)) : [];
    const versions = chosen && !passing.includes(chosen) ? [...passing, chosen] : passing;
    if (versions.length === 0) continue;
    out.push({ ...r, versions, chosen, shown: pickShown(versions, charEd, chosenId, menuPicks[r.key] ?? null) });
  }
  return out;
}

export interface SourceEntry { id: string; name: string; edition: EditionKey }

/** The sources that occur in the rows, each with its edition ("any" when its versions span both), by name. */
export function sourcesOf(rows: readonly VersionRow<VersionLike>[]): SourceEntry[] {
  const map = new Map<string, SourceEntry>();
  for (const v of rows.flatMap((r) => r.all)) {
    if (!v.source) continue;
    const ed = editionOf(v);
    const prev = map.get(v.source);
    map.set(v.source, prev ? { ...prev, edition: prev.edition === ed ? ed : "any" } : { id: v.source, name: v.sourceName ?? v.source, edition: ed });
  }
  return [...map.values()].sort((a, b) => a.name.localeCompare(b.name));
}

/** Rows above which the filter bar shows (sec. 1.5: more than 8 rows or more than one source). */
export const FILTER_BAR_MIN_ROWS = 8;

export const showsFilterBar = (rows: readonly VersionRow[]): boolean => rows.length > FILTER_BAR_MIN_ROWS || sourcesOf(rows).length > 1;
