import { type ReactNode, useState } from "react";
import type { EquipSlot, InventoryItem } from "../../api/contract";
import { Alert } from "../../components";
import { haptic } from "../../lib/haptics";
import { usePhone } from "../../lib/phone";
import { useCharacter } from "../../state/CharacterContext";
import { Band } from "../spells/TwoPane";
import { Glyph, type GlyphName } from "./Glyph";
import { PressMenu } from "../play/rolls/PressMenu";
import { usePress } from "../play/rolls/usePress";
import { ItemActionsDialog, useAddToAttacks, useEquipItem } from "./ItemActions";
import { itemMenuItems } from "./itemMenu";
import { RequirementsOverride } from "./RequirementsOverride";
import { attuneBlocked, attunementWarnings, restrictionText } from "./attunement";
import { chargesLabel } from "./RolledCharges";

const SLOT_LABEL: Record<EquipSlot, string> = { worn: "Worn", primary: "Primary Hand", secondary: "Secondary Hand", "two-handed": "Two-Handed" };
const slotLabel = (i: InventoryItem): string => (i.equipped === "worn" && /^armor$/i.test(i.category) ? "Armor" : i.equipped ? SLOT_LABEL[i.equipped] : "");

function Act({ icon, label, disabled, onClick }: { icon: GlyphName; label: string; disabled?: boolean; onClick: () => void }) {
  return <button type="button" disabled={disabled} onClick={onClick}><Glyph name={icon} />{label}</button>;
}

/** The row's name: a click selects it; a long press (touch), right-click or the menu key opens the item menu. */
function ItemName({ item, selected, onPick, onMenu }: { item: InventoryItem; selected: boolean; onPick: () => void; onMenu: (anchor: HTMLElement) => void }) {
  const press = usePress(onPick, onMenu);
  return <button type="button" className="ap-name" aria-label={`Select ${item.displayName}`} aria-pressed={selected} aria-haspopup="menu" {...press}>{item.displayName}</button>;
}

const Stat = ({ value, rest }: { value: ReactNode; rest?: string }) => <>{value}{rest ? <span className="eq-stat-rest">{rest}</span> : null}</>;

/** Classic builder InventorySection: equipped check, attunement link, name, equip location; flat icon actions below the box. */
export function InventoryPanel({ onSelectElement }: { onSelectElement: (itemId: string) => void }) {
  const { snapshot, mutate } = useCharacter();
  const equip = useEquipItem();
  const phone = usePhone();
  const [selId, setSelId] = useState<string | null>(null);
  const [details, setDetails] = useState<"view" | "edit" | null>(null);
  const [menu, setMenu] = useState<{ item: InventoryItem; anchor: HTMLElement } | null>(null);
  const addToAttacks = useAddToAttacks();
  if (!snapshot) return null;
  const inv = snapshot.inventory;
  // Every line, as in the classic builder: "include in inventory" only drops a line from the sheet, and a hidden
  // proxy ("Additional Feat, Alert") must be here to be equipped.
  const items = inv.items;
  const sel: InventoryItem | undefined = inv.items.find((i) => i.id === selId);
  const selIdx = sel ? inv.items.indexOf(sel) : -1;
  const toggleEquip = (i: InventoryItem) => (i.equipped ? void mutate((api, id) => api.unequipItem({ characterId: id, inventoryId: i.id })) : void equip(i, "auto"));
  const pick = (i: InventoryItem) => { setSelId(i.id); onSelectElement(i.itemId); };
  const showDeleteAll = !sel || sel.quantity > 1;
  // Aurora's inventory context menu (GitHub #28): right-click, long press or the menu key on a row
  const openMenu = (i: InventoryItem, anchor: HTMLElement) => { pick(i); setMenu({ item: i, anchor }); };
  const menuItems = (i: InventoryItem) => itemMenuItems(i, {
    details: () => setDetails("edit"),
    extract: () => void mutate((api, id) => api.extractPack({ characterId: id, inventoryId: i.id })),
    addToAttacks: () => void addToAttacks(i),
    equip: (slot) => void equip(i, slot === "worn" || slot === "two-handed" ? "auto" : slot),
    unequip: () => void mutate((api, id) => api.unequipItem({ characterId: id, inventoryId: i.id })),
    toggleAttunement: () => void mutate((api, id) => api.setAttuned({ characterId: id, inventoryId: i.id, attuned: !i.attuned })),
    remove: (all) => { haptic("delete"); void mutate((api, id) => api.removeItem({ characterId: id, inventoryId: i.id, all })); },
  });
  return (
    <section className="eq-inv" aria-label="Inventory">
      <div className="eq-inv-box">
        <Band title="Inventory" stats={[["Attuned items:", <Stat key="a" value={inv.attunement.used} rest={` / ${inv.attunement.max}`} />], ["Weight:", <Stat key="w" value={inv.carriedWeight.toFixed(1)} rest=" lb." />]]} />
        <div className="ap-scroll eq-inv-scroll">
          <table className="ap-grid eq-grid eq-invgrid">
            <thead>
              <tr>
                <th scope="col" className="eq-ic gold"><Glyph name="check" label="Equipped" /></th>
                <th scope="col" className="eq-ic gold"><Glyph name="link" label="Attuned" /></th>
                <th scope="col">Name</th>
                <th scope="col" className="eq-loc"><Glyph name="pin" />Equip location</th>
              </tr>
            </thead>
            <tbody>
              {items.map((i) => (
                <tr key={i.id} className={`${i.id === selId ? "sel" : ""} ${i.equipped ? "on" : ""}`} onMouseDown={() => pick(i)} onDoubleClick={() => { pick(i); setDetails("view"); }}
                  onContextMenu={(e) => { e.preventDefault(); openMenu(i, e.currentTarget.querySelector<HTMLElement>(".ap-name") ?? e.currentTarget); }}>
                  <td className="eq-ic">
                    {(i.equippable || i.equipped) && (
                      <button type="button" className={`eq-eqbtn ${i.equipped ? "on" : ""}`} aria-pressed={!!i.equipped} aria-label={`${i.equipped ? "Unequip" : "Equip"} ${i.displayName}`} onClick={() => toggleEquip(i)}><Glyph name="check" /></button>
                    )}
                  </td>
                  <td className="eq-ic">
                    {i.requiresAttunement && (
                      <button type="button" className={`eq-eqbtn eq-link ${i.attuned ? "on" : ""}`} aria-pressed={i.attuned} aria-label={`${i.attuned ? "End attunement to" : "Attune to"} ${i.displayName}`}
                        disabled={attuneBlocked(i)} title={restrictionText(i) ?? undefined}
                        onClick={() => void mutate((api, id) => api.setAttuned({ characterId: id, inventoryId: i.id, attuned: !i.attuned }))}><Glyph name="link" /></button>
                    )}
                  </td>
                  <td className="eq-invname">
                    <ItemName item={i} selected={i.id === selId} onPick={() => pick(i)} onMenu={(anchor) => openMenu(i, anchor)} />
                    {i.quantity > 1 && <span className="muted"> x{i.quantity}</span>}
                    {i.storage > 0 && <span className="eq-stored">Stored: {inv.storageNames[i.storage - 1]}</span>}
                    {i.attunementBarred && <span className="eq-stored eq-barred">{restrictionText(i)}</span>}
                    {i.rolledCharges?.map((r) => <span key={r.resourceId} className="eq-stored" title={r.name}>Charges: {chargesLabel(r)}</span>)}
                    <RequirementsOverride item={i} />
                  </td>
                  <td className="eq-slot">{slotLabel(i)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
      {[...new Set([...inv.warnings, ...attunementWarnings(snapshot)])].map((w) => <Alert key={w} tone="warning">{w}</Alert>)}
      {/* PH2: on phones the toolbar stays pinned above the bottom tabs while the inventory is on screen. */}
      <div className={phone ? "eq-inv-actions eq-pin" : "eq-inv-actions"} role="toolbar" aria-label="Inventory actions">
        {sel?.isPack && <Act icon="extract" label={`Extract ${sel.displayName}`} onClick={() => void mutate((api, id) => api.extractPack({ characterId: id, inventoryId: sel.id }))} />}
        <Act icon="details" label="Details" disabled={!sel} onClick={() => setDetails("view")} />
        <Act icon="up" label="Up" disabled={!sel || selIdx <= 0} onClick={() => sel && void mutate((api, id) => api.moveItem({ characterId: id, inventoryId: sel.id, direction: "up" }))} />
        <Act icon="down" label="Down" disabled={!sel || selIdx >= inv.items.length - 1} onClick={() => sel && void mutate((api, id) => api.moveItem({ characterId: id, inventoryId: sel.id, direction: "down" }))} />
        <Act icon="del" label="Delete" disabled={!sel} onClick={() => { if (sel) { haptic("delete"); void mutate((api, id) => api.removeItem({ characterId: id, inventoryId: sel.id, all: false })); } }} />
        {showDeleteAll && <Act icon="del" label="Delete all" disabled={!sel} onClick={() => { if (sel) { haptic("delete"); void mutate((api, id) => api.removeItem({ characterId: id, inventoryId: sel.id, all: true })); setSelId(null); } }} />}
      </div>
      {details && sel && <ItemActionsDialog item={sel} index={selIdx} count={inv.items.length} startEditing={details === "edit"} onClose={() => setDetails(null)} />}
      {menu && <PressMenu anchor={menu.anchor} label={menu.item.displayName} items={menuItems(menu.item)} onClose={() => setMenu(null)} />}
    </section>
  );
}
