import { useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { TabPanel, Tabs, type TabDef } from "../../components";
import { RequireCharacter } from "../../components/RequireCharacter";
import { usePhone } from "../../lib/phone";
import { useCharacter } from "../../state/CharacterContext";
import { EquipmentTab, type GearView } from "./EquipmentTab";
import { LoadoutTab, PackageChooser } from "./LoadoutTab";
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

/** `/equipment?tab=equipment` opens that sub-tab (Next selection to the starting equipment, tester B3). */
export const GEAR_TAB_PARAM = "tab";

const gearView = (phone: boolean, tab: string): GearView => (!phone ? "all" : tab === "inventory" ? "inventory" : "catalogue");

function Body() {
  const phone = usePhone();
  const { snapshot } = useCharacter();
  const [params, setParams] = useSearchParams();
  const wanted = params.get(GEAR_TAB_PARAM);
  const [tab, setTab] = useState(() => wanted ?? firstTab(phone, snapshot?.inventory.items.length ?? 0));
  // A later Next selection while Gear is open names the sub-tab again; it is consumed once followed.
  useEffect(() => {
    if (!wanted) return;
    setTab(wanted);
    const p = new URLSearchParams(params);
    p.delete(GEAR_TAB_PARAM);
    setParams(p, { replace: true });
  }, [wanted, params, setParams]);
  const tabs = equipmentTabs(phone);
  // Leaving the phone layout (a tablet turned to the wide shell) folds the Inventory sub-tab back into Equipment.
  const active = tabs.some((t) => t.id === tab) ? tab : "equipment";
  const packages = snapshot?.play.equipmentPackages ?? [];
  return (
    <>
      <Tabs inBand label="Equipment sections" idPrefix="equip" active={active} onChange={setTab} tabs={tabs} />
      <h1 className="sr-only">Equipment</h1>
      <TabPanel active={active} idPrefix="equip">
        {active === "treasure" ? <div className="eq-page"><TreasureTab /><LoadoutTab /></div> : (
          <>
            {/* Tester B3 (B-129): on a phone the starting equipment sits on top of the catalogue, where a new character equips. */}
            {phone && active === "equipment" && packages.length > 0 && <div className="eq-start-wrap"><PackageChooser packages={packages} /></div>}
            <EquipmentTab view={gearView(phone, active)} />
          </>
        )}
      </TabPanel>
    </>
  );
}

export function EquipmentPage() {
  return <RequireCharacter><Body /></RequireCharacter>;
}
