import { useState } from "react";
import type { CharacterSnapshot, ItemCategory, ItemSummary } from "../../api/contract";
import { isApiError } from "../../api/contract";
import { Alert, Button, ComboBox, useConfirm } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useAsync, useDebounced } from "../../state/useAsync";
import { Band, TwoPane } from "../spells/TwoPane";
import { CoinsDialog } from "./CoinsDialog";
import { Glyph } from "./Glyph";
import { ItemGrid } from "./ItemGrid";
import { InventoryPanel } from "./InventoryPanel";
import { QtyCommand } from "./QtyBox";
import "./equipment.css";

/** Shown until the character's category list has loaded (the classic builder's first category). */
const FIRST_CATEGORY = "Adventuring Gear";
const COIN_LABELS = [["Copper:", "cp"], ["Silver:", "sp"], ["Electrum:", "ep"], ["Gold:", "gp"], ["Platinum:", "pp"]] as const;

/** What the category list depends on: edition, spellcasting classes and source restrictions. */
const categoryKey = (s: CharacterSnapshot | null) =>
  s ? JSON.stringify([s.ruleset, s.spellcasting.classes.map((c) => c.className), s.sourceRestrictions]) : "";

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
  const [source, setSource] = useState("");
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
  const sources = useAsync(() => api.listSources(), [api]);
  const items = useAsync(() => api.searchItems({ category, query: q || undefined, source: source || undefined, limit: 200, characterId }), [api, category, q, source, characterId]);
  if (!snapshot) return null;
  const inv = snapshot.inventory;

  // The status bar reports the result ("You have added N x 'X' to your inventory."), as in the classic builder.
  const add = async (item: ItemSummary, qty: number, buy: boolean) => {
    await mutate(async (a, id) => {
      try {
        return await a.addItem({ characterId: id, itemId: item.id, quantity: qty, buy });
      } catch (e) {
        if (buy && isApiError(e) && e.code === "requirement_failed") {
          const r = await confirm({ title: "Not enough funds", body: "You cannot afford this. Add it without paying?", confirmLabel: "Add anyway" });
          if (r === "confirm") return a.addItem({ characterId: id, itemId: item.id, quantity: qty, buy: false });
        }
        throw e;
      }
    });
  };
  const pick = (i: ItemSummary) => { setPickedItem(i); setDescId(i.id); };
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
        <ItemGrid items={items.data?.items ?? []} category={category} picked={pickedItem?.id ?? null} onPick={pick} onAdd={(i) => void add(i, addQty, false)} />
      </div>
      <div className="eq-tools">
        <span className="eq-cmds">
          <QtyCommand label="Add" value={addQty} onChange={setAddQty} disabled={!pickedItem} onRun={() => pickedItem && void add(pickedItem, addQty, false)} />
          <QtyCommand label="Buy" value={buyQty} onChange={setBuyQty} disabled={!pickedItem} onRun={() => pickedItem && void add(pickedItem, buyQty, true)} />
        </span>
        <button type="button" className="eq-filter-tile" aria-label="Filter items" aria-pressed={filterOn} title="Turn the name filter on or off" onClick={() => setFilterOn((v) => !v)}><Glyph name="filter" /></button>
        <span className="eq-filter"><input type="text" aria-label="Filter text" placeholder="filter name and keywords..." value={query} onChange={(e) => setQuery(e.target.value)} /><button type="button" aria-label="Clear" onClick={() => setQuery("")}><Glyph name="x" /></button></span>
        <span className="eq-source"><ComboBox aria-label="Source filter" value={source} onChange={(e) => setSource(e.target.value)} options={[{ value: "", label: "--" }, ...(sources.data ?? []).map((s) => ({ value: s.id, label: s.name }))]} /></span>
        <Button className="eq-coins" onClick={() => setCoins(true)}>Manage coins: {inv.totalGp} gp</Button>
      </div>
      <InventoryPanel onSelectElement={setDescId} />
      {coins && <CoinsDialog onClose={() => setCoins(false)} />}
    </div>
  );
  return <TwoPane left={left} descEmpty={!descId} descId={descId} />;
}
