import { useState } from "react";
import { TabPanel, Tabs, type TabDef } from "../../components";
import { RequireCharacter } from "../../components/RequireCharacter";
import { usePhone } from "../../lib/phone";
import { useCharacter } from "../../state/CharacterContext";
import { EquipmentTab, type GearView } from "./EquipmentTab";
import { LoadoutTab } from "./LoadoutTab";
import { TreasureTab } from "./TreasureTab";
import "./equipment.css";

/**
 * Classic builder sub-tabs: Equipment (catalogue and inventory) and Additional treasure. GitHub #33 (B-195, suggested by
 * Egorobi): on phones the inventory sat under the whole catalogue, so it is a sub-tab of its own between the two there.
 * Desktop and tablet shells keep the two classic sub-tabs (U7).
 */
export function equipmentTabs(phone: boolean): TabDef[] {
  return [
    { id: "equipment", label: "Equipment" },
    ...(phone ? [{ id: "inventory", label: "Inventory" }] : []),
    { id: "treasure", label: "Additional treasure" },
  ];
}

/** On a phone, a character with items opens on its inventory (the screen used at the table); an empty one on the catalogue. */
const firstTab = (phone: boolean, itemCount: number): string => (phone && itemCount > 0 ? "inventory" : "equipment");

const gearView = (phone: boolean, tab: string): GearView => (!phone ? "all" : tab === "inventory" ? "inventory" : "catalogue");

function Body() {
  const phone = usePhone();
  const { snapshot } = useCharacter();
  const [tab, setTab] = useState(() => firstTab(phone, snapshot?.inventory.items.length ?? 0));
  const tabs = equipmentTabs(phone);
  // Leaving the phone layout (a tablet turned to the wide shell) folds the Inventory sub-tab back into Equipment.
  const active = tabs.some((t) => t.id === tab) ? tab : "equipment";
  return (
    <>
      <Tabs inBand label="Equipment sections" idPrefix="equip" active={active} onChange={setTab} tabs={tabs} />
      <h1 className="sr-only">Equipment</h1>
      <TabPanel active={active} idPrefix="equip">
        {active === "treasure" ? <div className="eq-page"><TreasureTab /><LoadoutTab /></div> : <EquipmentTab view={gearView(phone, active)} />}
      </TabPanel>
    </>
  );
}

export function EquipmentPage() {
  return <RequireCharacter><Body /></RequireCharacter>;
}
