import { useMemo, useState } from "react";
import type { Api, CharacterSnapshot, ItemCategory, ItemSummary, Page } from "../../api/contract";
import { isApiError } from "../../api/contract";
import { Alert, Button, ComboBox, useConfirm } from "../../components";
import { groupVersions, sourcesOf, visibleRows, type EditionFilter, type PickerFilter, type VersionLike } from "../../lib/versions";
import { EditionSegments, SourceFilterButton } from "../builder/PickerFilterBar";
import { VersionMenu } from "../builder/VersionMenu";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useAsync, useDebounced } from "../../state/useAsync";
import { Band, TwoPane } from "../spells/TwoPane";
import { CoinsDialog } from "./CoinsDialog";
import { Glyph } from "./Glyph";
import { ItemGrid, plainRows, type ItemListRow } from "./ItemGrid";
import { InventoryPanel } from "./InventoryPanel";
import { QtyCommand } from "./QtyBox";
import "./equipment.css";

/** Shown until the character's category list has loaded (the classic builder's first category). */
const FIRST_CATEGORY = "Adventuring Gear";
const COIN_LABELS = [["Copper:", "cp"], ["Silver:", "sp"], ["Electrum:", "ep"], ["Gold:", "gp"], ["Platinum:", "pp"]] as const;

/** What the category list depends on: edition, spellcasting classes and source restrictions. */
const categoryKey = (s: CharacterSnapshot | null) =>
  s ? JSON.stringify([s.ruleset, s.spellcasting.classes.map((c) => c.className), s.sourceRestrictions]) : "";

/** Rows asked for per `search_items` call (the backend's largest page). */
export const ITEM_PAGE = 1000;

/** Every row of an item search, page by page: a big category (every "Additional Spell") is never cut off. */
export async function searchAllItems(api: Api, args: Parameters<Api["searchItems"]>[0]): Promise<Page<ItemSummary>> {
  const items: ItemSummary[] = [];
  for (;;) {
    const page = await api.searchItems({ ...args, limit: ITEM_PAGE, offset: items.length });
    items.push(...page.items);
    if (page.items.length === 0 || items.length >= page.total) return { total: page.total, items };
  }
}

/** The character's item categories in the classic builder's order (`list_item_categories`). */
function useItemCategories(snapshot: CharacterSnapshot | null) {
  const api = useApi();
  const id = snapshot?.id ?? "";
  return useAsync(() => (id ? api.listItemCategories({ characterId: id }) : Promise.resolve([] as ItemCategory[])), [api, id, categoryKey(snapshot)]);
}

export function EquipmentTab() {
  const api = useApi();
  const confirm = useConfirm();
  const { snapshot, mutate } = useCharacter();
  const [picked, setCategory] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [filterOn, setFilterOn] = useState(true);
  // B-55 W3: the Edition filter holds for the whole screen, the Source checklist per category (its sources differ).
  const [edition, setEdition] = useState<EditionFilter>("all");
  const [sourcesByCategory, setSourcesByCategory] = useState<Readonly<Record<string, readonly string[] | null>>>({});
  const [menuPicks, setMenuPicks] = useState<Readonly<Record<string, string>>>({});
  const [menu, setMenu] = useState<{ key: string; anchor: HTMLElement } | null>(null);
  const [pickedItem, setPickedItem] = useState<ItemSummary | null>(null);
  const [descId, setDescId] = useState<string | null>(null);
  const [addQty, setAddQty] = useState(1);
  const [buyQty, setBuyQty] = useState(1);
  const [coins, setCoins] = useState(false);
  const q = useDebounced(filterOn ? query : "", 150);
  const cats = useItemCategories(snapshot);
  const listed = cats.data ?? [];
  // A category that is no longer listed (sources restricted, class changed) falls back to the first.
  const category = picked !== null && (!cats.data || listed.some((c) => c.name === picked)) ? picked : listed[0]?.name ?? FIRST_CATEGORY;
  const characterId = snapshot?.id;
  const items = useAsync(() => searchAllItems(api, { category, query: q || undefined, characterId }), [api, category, q, characterId, categoryKey(snapshot)]);
  const rows = useMemo(() => groupVersions(items.data?.items ?? []), [items.data]);
  const charEd = snapshot?.ruleset ?? "dnd2024";
  const filter: PickerFilter = { text: "", edition, sources: sourcesByCategory[category] ?? null };
  const visible = visibleRows(rows, filter, charEd, null, menuPicks);
  const listSources = useMemo(() => sourcesOf(rows), [rows]);
  const pickedId = pickedItem?.id ?? "";
  // A generic magic weapon or armor is added on a base picked in a second list, as in the classic builder.
  const bases = useAsync(() => (pickedId && characterId ? api.listBaseItems({ characterId, itemId: pickedId }) : Promise.resolve([] as ItemSummary[])), [api, pickedId, characterId]);
  const [base, setBase] = useState<ItemSummary | null>(null);
  if (!snapshot) return null;
  const inv = snapshot.inventory;
  const baseList = bases.data ?? [];
  const needsBase = baseList.length > 0;
  const chosenBase = needsBase && base && baseList.some((b) => b.id === base.id) ? base : null;
  const canAdd = !!pickedItem && !bases.loading && (!needsBase || !!chosenBase);

  // The status bar reports the result ("You have added N x 'X' to your inventory."), as in the classic builder.
  const add = async (item: ItemSummary, qty: number, buy: boolean) => {
    const baseItemId = item.id === pickedId && chosenBase ? chosenBase.id : undefined;
    await mutate(async (a, id) => {
      try {
        return await a.addItem({ characterId: id, itemId: item.id, quantity: qty, buy, baseItemId });
      } catch (e) {
        if (buy && isApiError(e) && e.code === "requirement_failed" && !(needsBase && !baseItemId)) {
          const r = await confirm({ title: "Not enough funds", body: "You cannot afford this. Add it without paying?", confirmLabel: "Add anyway" });
          if (r === "confirm") return a.addItem({ characterId: id, itemId: item.id, quantity: qty, buy: false, baseItemId });
        }
        throw e;
      }
    });
  };
  const pick = (i: ItemSummary) => { setPickedItem(i); setDescId(i.id); if (i.id !== pickedId) setBase(null); };
  const setFilter = (f: PickerFilter) => { setEdition(f.edition); setSourcesByCategory((cur) => ({ ...cur, [category]: f.sources })); };
  const clearFilters = () => setFilter({ text: "", edition: "all", sources: null });
  const menuRow = menu ? visible.find((r) => r.key === menu.key) ?? null : null;
  // Sec. 1.2 rule 2: a menu pick becomes the row's shown version (and is selected, as a click on the row would).
  const pickVersion = (row: ItemListRow, v: VersionLike) => {
    menu?.anchor.focus();
    setMenu(null);
    setMenuPicks((cur) => ({ ...cur, [row.key]: v.id }));
    const item = row.versions.find((x) => x.id === v.id);
    if (item) pick(item);
  };
  const pickBase = (b: ItemSummary) => { setBase(b); setDescId(b.id); };
  const label = (c: ItemCategory) => `${c.name} (${c.count})`;
  const left = (
    <div className="eq-layout">
      <nav className="eq-cats" aria-label="Item categories">
        <Band title="Categories" />
        <div className="eq-cats-sub">Category</div>
        <ul>
          {listed.map((c) => <li key={c.name}><button type="button" aria-current={c.name === category} onClick={() => { setCategory(c.name); setPickedItem(null); }}>{label(c)}</button></li>)}
        </ul>
      </nav>
      <div className="eq-items">
        <Band title={category === "Equipment Packs" ? "Equipment Packs" : category} stats={COIN_LABELS.map(([n, k]) => [n, inv.coins[k]] as [string, number])} />
        <div className="eq-cats-narrow" style={{ padding: "6px 0" }}>
          <ComboBox aria-label="Category" value={category} onChange={(e) => setCategory(e.target.value)} options={listed.map((c) => ({ value: c.name, label: label(c) }))} />
        </div>
        {cats.error && <Alert tone="danger">{cats.error}</Alert>}
        {items.error && <Alert tone="danger">{items.error}</Alert>}
        <ItemGrid rows={visible} category={category} charEd={charEd} picked={pickedItem?.id ?? null} onPick={pick} onAdd={(i) => void add(i, addQty, false)}
          menuKey={menu?.key ?? null} onVersions={(row, anchor) => setMenu(menu?.key === row.key ? null : { key: row.key, anchor })}
          emptyText={rows.length > 0 ? <>Nothing matches these filters. <button type="button" className="pick-fclear" onClick={clearFilters}>Clear</button></> : undefined} />
        {menu && menuRow && (
          <VersionMenu anchor={menu.anchor} rowName={menuRow.name} versions={menuRow.versions} charEd={charEd} shownId={menuRow.shown.id} chosenId={null}
            onPick={(v) => pickVersion(menuRow, v)} onClose={(restore) => { setMenu(null); if (restore) menu.anchor.focus(); }} />
        )}
        {bases.error && <Alert tone="danger">{bases.error}</Alert>}
        {needsBase && pickedItem && (
          <div className="eq-bases">
            <Band title={`Base item for ${pickedItem.name}`} />
            <ItemGrid label="Base items" rows={plainRows(baseList, charEd)} category={baseList[0]!.category} charEd={charEd} picked={chosenBase?.id ?? null} onPick={pickBase} onAdd={(b) => { setBase(b); }} />
          </div>
        )}
      </div>
      <div className="eq-tools">
        <span className="eq-cmds">
          <QtyCommand label="Add" value={addQty} onChange={setAddQty} disabled={!canAdd} onRun={() => pickedItem && canAdd && void add(pickedItem, addQty, false)} />
          <QtyCommand label="Buy" value={buyQty} onChange={setBuyQty} disabled={!canAdd} onRun={() => pickedItem && canAdd && void add(pickedItem, buyQty, true)} />
        </span>
        <button type="button" className="eq-filter-tile" aria-label="Filter items" aria-pressed={filterOn} title="Turn the name filter on or off" onClick={() => setFilterOn((v) => !v)}><Glyph name="filter" /></button>
        <span className="eq-filter"><input type="text" aria-label="Filter text" placeholder="filter name and keywords..." value={query} onChange={(e) => setQuery(e.target.value)} /><button type="button" aria-label="Clear" onClick={() => setQuery("")}><Glyph name="x" /></button></span>
        <span className="eq-edition"><EditionSegments value={edition} onChange={setEdition} /></span>
        <span className="eq-source"><SourceFilterButton filter={filter} onChange={setFilter} sources={listSources} /></span>
        <Button className="eq-coins" onClick={() => setCoins(true)}>Manage coins: {inv.totalGp} gp</Button>
      </div>
      <InventoryPanel onSelectElement={setDescId} />
      {coins && <CoinsDialog onClose={() => setCoins(false)} />}
    </div>
  );
  return <TwoPane left={left} descEmpty={!descId} descId={descId} />;
}
