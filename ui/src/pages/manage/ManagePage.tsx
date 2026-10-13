import { useState } from "react";
import { TabPanel, Tabs } from "../../components";
import { RequireCharacter } from "../../components/RequireCharacter";
import { useCharacter } from "../../state/CharacterContext";
import { AttacksTab } from "./AttacksTab";
import { CharacterTab } from "./CharacterTab";
import { CompanionTab } from "./CompanionTab";
import { AlliesTab, BackstoryTab, NotesTab } from "./TextTabs";
import "./manage.css";

function Body() {
  const { snapshot, mutate } = useCharacter();
  const [tab, setTab] = useState("character");
  if (!snapshot) return null;
  /* The classic builder's set. TREASURE moved to Equipment, OPTIONS and RULES to Settings: their components stay exported (OptionsTab, RulesTab, TreasureTab). */
  const tabs = [
    { id: "character", label: "Character" }, { id: "backstory", label: "Backstory" }, { id: "notes", label: "Notes" }, { id: "allies", label: "Allies & Organizations" },
    ...(snapshot.companion ? [{ id: "companion", label: "Companion" }] : []), { id: "attacks", label: "Attacks & Spellcasting" },
  ];
  return (
    <>
      <Tabs inBand tabs={tabs} active={tab} onChange={setTab} label="Manage sections" idPrefix="manage" />
      <h1 className="sr-only">Manage</h1>
      <TabPanel active={tab} idPrefix="manage">
        {tab === "character" && <CharacterTab />}
        {tab === "backstory" && <BackstoryTab onTrinket={(t) => void mutate((api, id) => api.updateDetails({ characterId: id, patch: { trinket: t } }))} />}
        {tab === "notes" && <NotesTab />}
        {tab === "allies" && <AlliesTab />}
        {tab === "attacks" && <AttacksTab />}
        {tab === "companion" && <CompanionTab />}
      </TabPanel>
    </>
  );
}

export function ManagePage() {
  return <RequireCharacter><Body /></RequireCharacter>;
}
