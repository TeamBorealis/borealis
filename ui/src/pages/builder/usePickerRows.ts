import { useCallback, useMemo } from "react";
import type { Api, Page, SelectionSlot, SlotOption, SlotOptionsArgs } from "../../api/contract";
import { useToast } from "../../components/Toast";
import {
  buildRows, editionOf, inferCharEdition, isFiltering, showsFilterBar, sourcesOf, visibleRows, yearOf,
  type PickerFilter, type ShownRow, type SourceEntry, type VersionLike, type VersionRow,
} from "../../lib/versions";
import type { Ruleset } from "../../api/contract";
import { useApi } from "../../state/ApiContext";
import { useAsync, type AsyncState } from "../../state/useAsync";
import { useCharacter } from "../../state/CharacterContext";
import { usePickerUiState } from "./pickerState";

/** Upper bound on pages for one list (a host that never reports its end cannot loop forever). */
const MAX_PAGES = 1000;

/**
 * B-55 sec. 1.6: the whole option list. Pages by `offset` until `total` rows are in, whatever page size the host uses
 * (no `limit` is sent, so a host that answers everything at once needs one call).
 */
export async function fetchAllSlotOptions(api: Pick<Api, "getSlotOptions">, args: Omit<SlotOptionsArgs, "limit" | "offset">): Promise<Page<SlotOption>> {
  const items: SlotOption[] = [];
  let total = 0;
  for (let page = 0; page < MAX_PAGES; page++) {
    const res = await api.getSlotOptions({ ...args, offset: items.length });
    total = res.total;
    items.push(...res.items);
    if (res.items.length === 0 || items.length >= total) break;
  }
  return { total: Math.max(total, items.length), items };
}

export interface PickerRows {
  options: AsyncState<Page<SlotOption> | null>;
  /** Every row of the list, unfiltered. */
  rows: VersionRow[];
  /** The rows the filters keep, each with its shown version. */
  visible: ShownRow[];
  charEd: Ruleset;
  filter: PickerFilter;
  setFilter: (f: PickerFilter) => void;
  /** Sec. 1.2 rule 2: remember a menu pick as the row's shown version. */
  remember: (rowKey: string, versionId: string) => void;
  sources: SourceEntry[];
  showBar: boolean;
  filtering: boolean;
  /** At least one visible row has two or more versions: the version column is drawn on every row. */
  versionColumn: boolean;
}

/** Loads a slot's options (all pages) and applies the B-55 row model: grouping, filters, shown versions. */
export function usePickerRows(characterId: string, slot: SelectionSlot, enabled: boolean): PickerRows {
  const api = useApi();
  const { snapshot } = useCharacter();
  const chosenSig = slot.chosen?.id ?? "";
  const options = useAsync(
    () => (enabled ? fetchAllSlotOptions(api, { characterId, slotId: slot.id, query: "" }) : Promise.resolve(null)),
    [api, characterId, slot.id, enabled, chosenSig],
  );
  const [ui, update] = usePickerUiState(slot.id);
  const rows = useMemo(() => buildRows(options.data?.items ?? [], slot.chosen), [options.data, slot.chosen]);
  const charEd = snapshot?.ruleset ?? inferCharEdition(rows.flatMap((r) => r.all));
  const chosenId = slot.chosen?.id ?? null;
  const visible = useMemo(() => visibleRows(rows, ui.filter, charEd, chosenId, ui.menuPicks), [rows, ui.filter, charEd, chosenId, ui.menuPicks]);
  const setFilter = useCallback((filter: PickerFilter) => update((s) => ({ ...s, filter })), [update]);
  const remember = useCallback((rowKey: string, versionId: string) => update((s) => ({ ...s, menuPicks: { ...s.menuPicks, [rowKey]: versionId } })), [update]);
  const sources = useMemo(() => sourcesOf(rows), [rows]);
  return {
    options, rows, visible, charEd, filter: ui.filter, setFilter, remember, sources,
    showBar: showsFilterBar(rows), filtering: isFiltering(ui.filter), versionColumn: visible.some((r) => r.versions.length > 1),
  };
}

/** Sec. 1.4 toast: "Switched Dragonborn to <source> (2014 rules)." plus " Choose again: <titles>." when sub-choices dropped. */
export function switchToast(rowName: string, v: VersionLike, dropped: readonly string[]): string {
  const ed = editionOf(v);
  const rules = ed === "any" ? "" : ` (${yearOf(ed)} rules)`;
  const again = dropped.length > 0 ? ` Choose again: ${dropped.join(", ")}.` : "";
  return `Switched ${rowName} to ${v.sourceName ?? v.name}${rules}.${again}`;
}

/** Switches a chosen row to another of its versions (switchVersion) and reports it in a toast. */
export function useSwitchVersion(): (slotId: string, rowName: string, v: VersionLike) => Promise<void> {
  const { mutate } = useCharacter();
  const toast = useToast();
  return useCallback(async (slotId, rowName, v) => {
    let dropped: string[] = [];
    const next = await mutate(async (api, id) => {
      const res = await api.switchVersion({ characterId: id, slotId, elementId: v.id });
      dropped = res.dropped;
      return res.snapshot;
    });
    if (next) toast.push(switchToast(rowName, v, dropped));
  }, [mutate, toast]);
}
