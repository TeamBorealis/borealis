import { useState } from "react";
import { TabPanel, Tabs } from "../../components";
import { RequireCharacter } from "../../components/RequireCharacter";
import { EquipmentTab } from "./EquipmentTab";
import { LoadoutTab } from "./LoadoutTab";
import { TreasureTab } from "./TreasureTab";
import "./equipment.css";

function Body() {
  const [tab, setTab] = useState("equipment");
  return (
    <>
      <Tabs inBand label="Equipment sections" idPrefix="equip" active={tab} onChange={setTab} tabs={[{ id: "equipment", label: "Equipment" }, { id: "treasure", label: "Additional treasure" }]} />
      <h1 className="sr-only">Equipment</h1>
      <TabPanel active={tab} idPrefix="equip">
        {tab === "equipment" ? <EquipmentTab /> : <div className="eq-page"><TreasureTab /><LoadoutTab /></div>}
      </TabPanel>
    </>
  );
}

export function EquipmentPage() {
  return <RequireCharacter><Body /></RequireCharacter>;
}
