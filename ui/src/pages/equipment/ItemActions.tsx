import { CheckBox } from "../../components/controls";
import { useState } from "react";
import type { InventoryItem } from "../../api/contract";
import { Button, Chip, Modal, TextArea, TextField, useConfirm } from "../../components";
import { useToast } from "../../components/Toast";
import { haptic } from "../../lib/haptics";
import { useCharacter } from "../../state/CharacterContext";
import { useSettings } from "../../state/SettingsContext";
import { attuneBlocked, restrictionText, weaponsFiring } from "./attunement";
import { RequirementsOverride } from "./RequirementsOverride";
import { RolledChargesEditor } from "./RolledCharges";

/** Equips an item; for weapons missing from the attacks list it offers to add one (per setting). */
export function useEquipItem() {
  const { mutate } = useCharacter();
  const { settings } = useSettings();
  const confirm = useConfirm();
  return async (item: InventoryItem, slot: "auto" | "primary" | "secondary" | "versatile"): Promise<void> => {
    const next = await mutate((api, id) => api.equipItem({ characterId: id, inventoryId: item.id, slot }));
    if (!next || !item.weapon || !settings.confirmEquipWeaponNotInAttacks || next.attacks.some((a) => a.name === item.displayName)) return;
    const r = await confirm({ title: "Add to attacks?", body: `${item.displayName} is not in your attacks list. Add it with automatic bonuses?`, confirmLabel: "Add attack" });
    if (r !== "confirm") return;
    await mutate((api, id) => api.addToAttacks({ characterId: id, inventoryId: item.id }));
  };
}

/** Aurora's "Add to Attacks & Spellcasting" (GitHub #28): the weapon's row stays in the attacks while it is not held. */
export function useAddToAttacks() {
  const { mutate } = useCharacter();
  const toast = useToast();
  return async (item: InventoryItem): Promise<void> => {
    const next = await mutate((api, id) => api.addToAttacks({ characterId: id, inventoryId: item.id }));
    if (next) toast.push(`${item.displayName} added to your attacks.`, "info");
  };
}

export function ItemActionsDialog({ item, index, count, onClose, startEditing = false }: { item: InventoryItem; index: number; count: number; onClose: () => void; startEditing?: boolean }) {
  const { snapshot, mutate } = useCharacter();
  const [editing, setEditing] = useState(startEditing);
  const addToAttacks = useAddToAttacks();
  // opened on the edit form (the inventory menu's Edit Item Details), leaving it closes the dialog
  const leaveEditing = () => (startEditing ? onClose() : setEditing(false));
  const [name, setName] = useState(item.displayName);
  const [notes, setNotes] = useState(item.notes);
  const [listed, setListed] = useState(item.listInInventory);
  const [card, setCard] = useState(item.showCard ?? false);
  const { settings } = useSettings();
  const equipItem = useEquipItem();
  if (!snapshot) return null;
  const names = snapshot.inventory.storageNames;

  const equip = async (slot: "auto" | "primary" | "secondary" | "versatile") => { await equipItem(item, slot); onClose(); };
  const run = (fn: Parameters<typeof mutate>[0]) => void mutate(fn).then(onClose);

  if (editing) {
    return (
      <Modal title={`Edit ${item.name}`} onClose={leaveEditing}
        footer={<><Button onClick={leaveEditing}>Cancel</Button><Button variant="primary" onClick={() => run((api, id) => api.updateItemDetails({ characterId: id, inventoryId: item.id, patch: { displayName: name, notes, listInInventory: listed, showCard: card } }))}>Save</Button></>}>
        <div className="stack">
          <TextField label="Display name" value={name} onChange={(e) => setName(e.target.value)} />
          <TextArea label="Notes" value={notes} onChange={(e) => setNotes(e.target.value)} />
          <label className="check"><CheckBox checked={listed} onChange={(e) => setListed(e.target.checked)} /><span>List in inventory on the sheet</span></label>
          <label className="check"><CheckBox checked={card} disabled={!settings.sheet.cards.item} onChange={(e) => setCard(e.target.checked)} /><span>Include in item cards</span></label>
        </div>
      </Modal>
    );
  }
  const restriction = restrictionText(item);
  const firedBy = weaponsFiring(item, snapshot.attacks);
  return (
    <Modal title={item.displayName} onClose={onClose}>
      <div className="stack">
        <div className="row">
          {item.equipped && <Chip tone="success">Equipped: {item.equipped}</Chip>}
          {item.attuned && <Chip tone="gold">Attuned</Chip>}
          {item.storage > 0 && <Chip>Stored: {names[item.storage - 1]}</Chip>}
          <Chip>x{item.quantity}</Chip>
        </div>
        {restriction && <p className={item.attunementBarred ? "error-text" : "muted"}>{restriction}{attuneBlocked(item) ? ". You do not meet this requirement." : ""}</p>}
        {firedBy.length > 0 && <p className="muted">Fired from {firedBy.join(", ")}: its bonus is added to the attack and damage.</p>}
        {item.rolledCharges?.map((r) => <RolledChargesEditor key={r.resourceId} item={item} charge={r} />)}
        <RequirementsOverride item={item} />
        <div className="row">
          {item.equippable && !item.equipped && <Button variant="primary" onClick={() => void equip("auto")}>Equip</Button>}
          {item.isWeapon && !item.equipped && <><Button onClick={() => void equip("primary")}>Primary hand</Button><Button onClick={() => void equip("secondary")}>Off hand</Button><Button onClick={() => void equip("versatile")}>Two-handed</Button></>}
          {item.equipped && <Button onClick={() => run((api, id) => api.unequipItem({ characterId: id, inventoryId: item.id }))}>Unequip</Button>}
          {item.requiresAttunement && <Button disabled={attuneBlocked(item)} onClick={() => run((api, id) => api.setAttuned({ characterId: id, inventoryId: item.id, attuned: !item.attuned }))}>{item.attuned ? "End attunement" : "Attune"}</Button>}
        </div>
        <div className="row">
          {item.storage !== 0 && <Button onClick={() => run((api, id) => api.storeItem({ characterId: id, inventoryId: item.id, storage: 0 }))}>Retrieve</Button>}
          {item.storage !== 1 && <Button onClick={() => run((api, id) => api.storeItem({ characterId: id, inventoryId: item.id, storage: 1 }))}>Store in {names[0]}</Button>}
          {item.storage !== 2 && <Button onClick={() => run((api, id) => api.storeItem({ characterId: id, inventoryId: item.id, storage: 2 }))}>Store in {names[1]}</Button>}
        </div>
        <div className="row">
          {item.isPack && <Button onClick={() => run((api, id) => api.extractPack({ characterId: id, inventoryId: item.id }))}>Extract contents</Button>}
          {item.isWeapon && <Button onClick={() => void addToAttacks(item).then(onClose)}>Add to attacks</Button>}
          <Button onClick={() => setEditing(true)}>Edit details</Button>
          <Button disabled={index === 0} icon="up" onClick={() => void mutate((api, id) => api.moveItem({ characterId: id, inventoryId: item.id, direction: "up" }))}>Move up</Button>
          <Button disabled={index === count - 1} icon="down" onClick={() => void mutate((api, id) => api.moveItem({ characterId: id, inventoryId: item.id, direction: "down" }))}>Move down</Button>
        </div>
        <hr />
        <div className="row">
          <Button variant="danger" onClick={() => { haptic("delete"); run((api, id) => api.removeItem({ characterId: id, inventoryId: item.id, all: false })); }}>Remove one</Button>
          {item.quantity > 1 && <Button variant="danger" onClick={() => { haptic("delete"); run((api, id) => api.removeItem({ characterId: id, inventoryId: item.id, all: true })); }}>Remove all</Button>}
        </div>
      </div>
    </Modal>
  );
}
