import type { SlotOption, SlotOptionBase } from "../contract";
import type { Ruleset } from "../types";
import { compareVersions, groupKey, nameMatches } from "../../lib/versions";
import { BUNDLED_SOURCES, getElement } from "./content";
import type { MockElement } from "./data-types";
import { mockEditionMarks } from "./edition-marks";
import { apiError } from "./errors";
import { buildSlotDefs, type SlotDef } from "./slots";
import { findByWireId } from "./slot-ids";
import type { MockCharacter } from "./state";

/** The host's default page size when the caller passes no `limit` (same as the Rust host today). */
export const DEFAULT_PAGE = 200;

/** One offered option as a version: summary fields, edition marks and `bundled` (B-55). */
export function toVersion(e: MockElement, ruleset: Ruleset): SlotOptionBase {
  const { description: _d, facts: _f, grants: _g, ...rest } = e;
  return { ...rest, ...mockEditionMarks(rest, ruleset), ...(BUNDLED_SOURCES.includes(e.source) ? { bundled: true } : {}) };
}

/**
 * B-55 sec. 1.1/4: one row per group key over the offered options. Versions are in the default order (sec. 1.2 rule 3)
 * with the chosen one first; the row's own fields are versions[0]; `versions` only when there are two or more.
 */
export function groupRows(options: readonly SlotOptionBase[], ruleset: Ruleset, chosenId: string | null): SlotOption[] {
  const groups = new Map<string, SlotOptionBase[]>();
  for (const o of options) {
    const key = groupKey(o);
    groups.set(key, [...(groups.get(key) ?? []), o]);
  }
  const order = compareVersions(ruleset);
  return [...groups.values()].map((list) => {
    const versions = [...list].sort((a, b) => Number(b.id === chosenId) - Number(a.id === chosenId) || order(a, b));
    const head = versions[0]!;
    return versions.length > 1 ? { ...head, versions } : { ...head };
  });
}

/** Rows whose name matches the host search (`query`): the normalized row name only, like the Rust host. */
export const queryRows = (rows: readonly SlotOption[], query: string | undefined): SlotOption[] =>
  rows.filter((r) => !query || nameMatches(r.name, query));

const titleOf = (d: SlotDef) => `${d.title}\u0000${d.ordinal}`;

function requireSlot(c: MockCharacter, slotId: string): SlotDef {
  const def = findByWireId(buildSlotDefs(c), slotId);
  if (!def) throw apiError("not_found", `Unknown slot ${slotId}`);
  if (def.locked) throw apiError("invalid_argument", "This selection is locked.");
  return def;
}

/**
 * B-55 sec. 3: switches the slot's pick to another version of the same row and re-applies the old pick's dependent
 * selections where they still fit (same slot title and ordinal; the same element, else one of the same name). The rest
 * are dropped and their titles reported in slot order. Throws (changing nothing) for a non-version.
 */
export function switchVersionChoices(c: MockCharacter, slotId: string, elementId: string): { choices: Record<string, string>; dropped: string[] } {
  const def = requireSlot(c, slotId);
  const curId = c.choices[def.id];
  const cur = curId ? getElement(curId) : undefined;
  if (!cur) throw apiError("invalid_argument", "Nothing is chosen in that slot.");
  const next = def.candidates.find((e) => e.id === elementId);
  if (!next || groupKey(next) !== groupKey(cur)) throw apiError("requirement_failed", `That is not another version of ${cur.name}.`);
  if (next.id === cur.id) return { choices: c.choices, dropped: [] };
  const oldDefs = buildSlotDefs(c).filter((d) => d.id !== def.id && c.choices[d.id]);
  const base: Record<string, string> = { ...c.choices, [def.id]: next.id };
  const newDefs = buildSlotDefs({ ...c, choices: base });
  const dropped: string[] = [];
  const choices = oldDefs.reduce<Record<string, string>>((acc, old) => {
    const pick = c.choices[old.id]!;
    const slot = newDefs.find((d) => d.id === old.id) ?? newDefs.find((d) => titleOf(d) === titleOf(old));
    const pickEl = getElement(pick);
    const fit = slot?.candidates.find((e) => e.id === pick) ?? (pickEl ? slot?.candidates.find((e) => groupKey(e) === groupKey(pickEl)) : undefined);
    const { [old.id]: _old, ...rest } = acc;
    if (slot && fit) return { ...rest, [slot.id]: fit.id };
    if (!dropped.includes(old.title)) dropped.push(old.title);
    return rest;
  }, base);
  return { choices, dropped };
}
