import { CheckBox } from "../../components/controls";
import { useState } from "react";
import type { InventoryItem } from "../../api/contract";
import { Button, Chip, Modal, TextArea, TextField, useConfirm } from "../../components";
import { useCharacter } from "../../state/CharacterContext";
import { useSettings } from "../../state/SettingsContext";
import { attuneBlocked, restrictionText, weaponsFiring } from "./attunement";

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
    const w = item.weapon;
    await mutate((api, id) => api.upsertAttack({ characterId: id, attack: {
      name: item.displayName, range: w.properties.includes("Thrown") ? "20/60 ft" : "Melee", attackBonus: "", damage: w.damage,
      description: w.damageType, ability: w.ability, auto: true, showInCombat: true, card: false,
    } }));
  };
}

export function ItemActionsDialog({ item, index, count, onClose }: { item: InventoryItem; index: number; count: number; onClose: () => void }) {
  const { snapshot, mutate } = useCharacter();
  const [editing, setEditing] = useState(false);
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
      <Modal title={`Edit ${item.name}`} onClose={() => setEditing(false)}
        footer={<><Button onClick={() => setEditing(false)}>Cancel</Button><Button variant="primary" onClick={() => run((api, id) => api.updateItemDetails({ characterId: id, inventoryId: item.id, patch: { displayName: name, notes, listInInventory: listed, showCard: card } }))}>Save</Button></>}>
        <div className="stack">
          <TextField label="Display name" value={name} onChange={(e) => setName(e.target.value)} />
          <TextArea label="Notes" value={notes} onChange={(e) => setNotes(e.target.value)} />
          <label className="check"><CheckBox checked={listed} onChange={(e) => setListed(e.target.checked)} /><span>List in inventory on the sheet</span></label>
          <label className="check"><CheckBox checked={card} disabled={!settings.sheet.cards.item} onChange={(e) => setCard(e.target.checked)} /><span>Include in item cards</span></label>
        </div>
      </Modal>
    );
  }
  const canWear = item.isWeapon || item.category === "Armor" || item.category === "Magic Items";
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
        <div className="row">
          {canWear && !item.equipped && <Button variant="primary" onClick={() => void equip("auto")}>Equip</Button>}
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
          <Button onClick={() => setEditing(true)}>Edit details</Button>
          <Button disabled={index === 0} icon="up" onClick={() => void mutate((api, id) => api.moveItem({ characterId: id, inventoryId: item.id, direction: "up" }))}>Move up</Button>
          <Button disabled={index === count - 1} icon="down" onClick={() => void mutate((api, id) => api.moveItem({ characterId: id, inventoryId: item.id, direction: "down" }))}>Move down</Button>
        </div>
        <hr />
        <div className="row">
          <Button variant="danger" onClick={() => run((api, id) => api.removeItem({ characterId: id, inventoryId: item.id, all: false }))}>Remove one</Button>
          {item.quantity > 1 && <Button variant="danger" onClick={() => run((api, id) => api.removeItem({ characterId: id, inventoryId: item.id, all: true }))}>Remove all</Button>}
        </div>
      </div>
    </Modal>
  );
}
