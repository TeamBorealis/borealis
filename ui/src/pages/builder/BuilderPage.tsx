import { useEffect, useState, type ReactNode } from "react";
import { Navigate, useNavigate, useParams } from "react-router";
import type { BuilderTab, RulesetProfile } from "../../api/contract";
import { Alert, Button, TabPanel, Tabs, type TabDef } from "../../components";
import { RequireCharacter } from "../../components/RequireCharacter";
import { useShell } from "../../components/shell/ShellContext";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useSettings } from "../../state/SettingsContext";
import { useAsync } from "../../state/useAsync";
import "./builder.css";
import { AbilitiesPane, AbilitiesTab } from "./AbilitiesTab";
import { DescriptionPane, PaneShell } from "./DescriptionPane";
import type { DescribeFn, PickChoice } from "./PickerGroup";
import { usePhone } from "../../lib/phone";
import { SlotList } from "./SlotList";
import { PickerStateProvider } from "./pickerState";

type TabId = BuilderTab;
const SLOT_TABS: BuilderTab[] = ["race", "class", "background", "abilities", "feats", "languages", "proficiencies", "companion"];

/** Page a slot is picked on: spell slots live on Magic (`/spells`), every other tab on its own Build tab. */
export function tabForSlot(snapshotTab: BuilderTab): TabId {
  return snapshotTab;
}

const INTRO: Record<string, { title: string; text: string }> = {
  race: { title: "Heritage", text: "Choose where your character comes from. Select an entry in the list to read about it; choose it by double-clicking or pressing Enter." },
  class: { title: "Class", text: "Your class sets your hit die, proficiencies and features. Select an entry to read about it before choosing." },
  background: { title: "Background", text: "Your background describes what your character did before adventuring, and grants skills, tools and a feature." },
  feats: { title: "Feats", text: "Feats are optional talents. Selections appear here when your build allows them." },
  languages: { title: "Languages", text: "Languages your character speaks, reads and writes." },
  proficiencies: { title: "Proficiencies", text: "Skills, tools, weapons and armor your character is trained with." },
  companion: { title: "Companion", text: "A companion creature that travels with your character." },
};

/** Phones (B-48 O2): the same intro, describing taps and the Choose button instead of a double click. */
const PHONE_RACE_INTRO = { title: "Heritage", text: "Choose where your character comes from. Tap an entry in the list to read about it; choose it with the Choose button or a double tap." };

/** The old Levels & HP route: leveling and multiclassing live in the Level Up flyout, so open it and show Class. */
function LevelsRedirect() {
  const { setLevelUpOpen } = useShell();
  useEffect(() => setLevelUpOpen(true), [setLevelUpOpen]);
  return <Navigate to="/builder/class" replace />;
}

function BuilderBody({ profile }: { profile: RulesetProfile | null }) {
  const { snapshot } = useCharacter();
  const { settings } = useSettings();
  const { setLevelUpOpen } = useShell();
  const { tab = "race" } = useParams();
  const nav = useNavigate();
  const [described, setDescribed] = useState<{ tab: string; id: string; isElement: boolean; choice?: PickChoice } | null>(null);
  const phone = usePhone();
  const next = snapshot?.progress.nextSlotId ?? null;
  const flowTab = snapshot?.slots.find((s) => s.id === next)?.tab;
  // Selection flow (setting): jump to the first pending pick after every change.
  useEffect(() => {
    if (!settings.selectionFlow || !next || !flowTab || flowTab === "spells") return;
    const t = setTimeout(() => nav(`/builder/${flowTab}?slot=${encodeURIComponent(next)}`), 250);
    return () => clearTimeout(t);
  }, [settings.selectionFlow, next, flowTab, nav]);
  if (!snapshot) return null;
  const hasCompanion = snapshot.companion !== null || snapshot.slots.some((s) => s.tab === "companion");
  const tabs: TabDef[] = [
    { id: "race", label: profile?.terminology.species ?? "Race" },
    { id: "class", label: "Class" },
    { id: "background", label: "Background" },
    { id: "abilities", label: "Ability Scores" },
    { id: "languages", label: "Languages" },
    { id: "proficiencies", label: "Proficiencies" },
    { id: "feats", label: "Feats" },
    ...(hasCompanion ? [{ id: "companion", label: "Companion" }] : []),
  ];
  if (tab === "levels") return <LevelsRedirect />;
  if (!tabs.some((t) => t.id === tab)) return <Navigate to="/builder/race" replace />;
  const active = tab as TabId;
  const slots = snapshot.slots.filter((s) => s.tab === active && SLOT_TABS.includes(s.tab) && !s.locked); // locked slots are hidden (E31 SO-3)
  // List entries (e.g. a background's ability increase) are not elements: the pane keeps the last element instead.
  const elementChoices = slots.filter((s) => s.chosen && s.chosen.type !== "list");
  const defaultId = elementChoices[0]?.chosen?.id ?? null;
  const own = described?.tab === active ? described : null;
  const shownId = own ? own.id : defaultId;
  const paneId = own?.isElement ? own.id : defaultId;
  const onDescribe: DescribeFn = (id, isElement = true, choice) => setDescribed({ tab: active, id, isElement, choice });
  // O2 (phones): the described row's "Choose <name>" button, while its slot does not already hold it.
  const choice = phone && own?.choice && snapshot.slots.some((s) => s.id === own.choice!.slotId && s.chosen?.id !== own.id) ? own.choice : null;
  const intro = phone && active === "race" ? PHONE_RACE_INTRO : INTRO[active] ?? INTRO.race!;
  const speciesWord = profile?.terminology.species ?? "Race";
  const species = phone ? speciesWord : speciesWord.toUpperCase();
  const fallback: ReactNode = <PaneShell title={active === "race" ? species : intro.title} imageIcon={false} intro={intro.text} readText={intro.text} />;
  const pane = active === "abilities" ? <AbilitiesPane /> : <DescriptionPane elementId={paneId} fallback={fallback} />;
  return (
    <div className="bp">
      <h1 className="sr-only">Build</h1>
      <div className="bp-left">
        {snapshot.level.levelUpAvailable && (
          <Alert tone="success"><strong>Level up available.</strong> <button type="button" className="bp-linkbtn" onClick={() => setLevelUpOpen(true)}>Open Level Up</button> to advance.</Alert>
        )}
        {snapshot.notices.map((n) => <Alert key={n} tone="info">{n}</Alert>)}
        <Tabs inBand tabs={tabs} active={active} onChange={(id) => nav(`/builder/${id}`)} label="Build sections" idPrefix="build" />
        <TabPanel active={active} idPrefix="build">
          {active === "abilities" ? <AbilitiesTab describedId={shownId} onDescribe={onDescribe} />
            : <SlotList slots={slots} describedId={shownId} onDescribe={onDescribe} />}
        </TabPanel>
        <div className="sr-only" aria-live="polite">{snapshot.progress.remaining} choices remaining</div>
      </div>
      <div className="bp-right">{pane}</div>
      {/* O2 (phones): under the list and its description; it stays in reach at the bottom while the page scrolls. */}
      {choice && <div className="bp-choose"><Button variant="primary" className="bp-choose-btn" onClick={choice.choose}>Choose {choice.name}</Button></div>}
    </div>
  );
}

export function BuilderPage() {
  const api = useApi();
  const { snapshot } = useCharacter();
  const ruleset = snapshot?.ruleset;
  const profile = useAsync(() => (ruleset ? api.getRulesetProfile({ ruleset }) : Promise.resolve(null)), [api, ruleset]);
  return <RequireCharacter><PickerStateProvider><BuilderBody profile={profile.data} /></PickerStateProvider></RequireCharacter>;
}
