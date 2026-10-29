import { type ReactNode, useState } from "react";
import type { EquipSlot, InventoryItem } from "../../api/contract";
import { Alert } from "../../components";
import { haptic } from "../../lib/haptics";
import { useCharacter } from "../../state/CharacterContext";
import { Band } from "../spells/TwoPane";
import { Glyph, type GlyphName } from "./Glyph";
import { ItemActionsDialog, useEquipItem } from "./ItemActions";
import { RequirementsOverride } from "./RequirementsOverride";
import { attuneBlocked, attunementWarnings, restrictionText } from "./attunement";
import { chargesLabel } from "./RolledCharges";

const SLOT_LABEL: Record<EquipSlot, string> = { worn: "Worn", primary: "Primary Hand", secondary: "Secondary Hand", "two-handed": "Two-Handed" };
const slotLabel = (i: InventoryItem): string => (i.equipped === "worn" && /^armor$/i.test(i.category) ? "Armor" : i.equipped ? SLOT_LABEL[i.equipped] : "");

function Act({ icon, label, disabled, onClick }: { icon: GlyphName; label: string; disabled?: boolean; onClick: () => void }) {
  return <button type="button" disabled={disabled} onClick={onClick}><Glyph name={icon} />{label}</button>;
}

const Stat = ({ value, rest }: { value: ReactNode; rest?: string }) => <>{value}{rest ? <span className="eq-stat-rest">{rest}</span> : null}</>;

/** Classic builder InventorySection: equipped check, attunement link, name, equip location; flat icon actions below the box. */
export function InventoryPanel({ onSelectElement }: { onSelectElement: (itemId: string) => void }) {
  const { snapshot, mutate } = useCharacter();
  const equip = useEquipItem();
  const [selId, setSelId] = useState<string | null>(null);
  const [details, setDetails] = useState(false);
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
                <tr key={i.id} className={`${i.id === selId ? "sel" : ""} ${i.equipped ? "on" : ""}`} onMouseDown={() => pick(i)} onDoubleClick={() => { pick(i); setDetails(true); }}>
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
                    <button type="button" className="ap-name" aria-label={`Select ${i.displayName}`} aria-pressed={i.id === selId} onClick={() => pick(i)}>{i.displayName}</button>
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
      <div className="eq-inv-actions" role="toolbar" aria-label="Inventory actions">
        {sel?.isPack && <Act icon="extract" label={`Extract ${sel.displayName}`} onClick={() => void mutate((api, id) => api.extractPack({ characterId: id, inventoryId: sel.id }))} />}
        <Act icon="details" label="Details" disabled={!sel} onClick={() => setDetails(true)} />
        <Act icon="up" label="Up" disabled={!sel || selIdx <= 0} onClick={() => sel && void mutate((api, id) => api.moveItem({ characterId: id, inventoryId: sel.id, direction: "up" }))} />
        <Act icon="down" label="Down" disabled={!sel || selIdx >= inv.items.length - 1} onClick={() => sel && void mutate((api, id) => api.moveItem({ characterId: id, inventoryId: sel.id, direction: "down" }))} />
        <Act icon="del" label="Delete" disabled={!sel} onClick={() => { if (sel) { haptic("delete"); void mutate((api, id) => api.removeItem({ characterId: id, inventoryId: sel.id, all: false })); } }} />
        {showDeleteAll && <Act icon="del" label="Delete all" disabled={!sel} onClick={() => { if (sel) { haptic("delete"); void mutate((api, id) => api.removeItem({ characterId: id, inventoryId: sel.id, all: true })); setSelId(null); } }} />}
      </div>
      {details && sel && <ItemActionsDialog item={sel} index={selIdx} count={inv.items.length} onClose={() => setDetails(false)} />}
    </section>
  );
}
