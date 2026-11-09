import { useEffect, useRef, useState, type ReactNode } from "react";
import { Navigate, useNavigate, useParams } from "react-router";
import type { AddedKind, BuilderTab, RulesetProfile } from "../../api/contract";
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
import { speciesTerm } from "../../lib/terminology";
import { scrollToStart } from "../../lib/scrollReveal";
import { SlotList } from "./SlotList";
import { AddedPicks } from "./AddedPicks";
import { GrantedFeats, grantedFeats } from "./GrantedFeats";
import { AbilityOptionPane, describedAbilityOption } from "./abilityOption";
import { NoticesBanner } from "./NoticesBanner";
import { CharacterMissingContentDialog, ShowMissingContentButton } from "./MissingContentAction";
import { PickerStateProvider } from "./pickerState";

type TabId = BuilderTab;
/** B3: the Build tabs with an Add action for extras the DM grants (spells are added on Magic). */
const ADDED_KIND: Partial<Record<BuilderTab, AddedKind>> = { feats: "feat", languages: "language", proficiencies: "proficiency" };
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

/** Phones (B-48 O2, TV1 A1/A2): the same intro, describing taps, the row's circle and the Choose button instead of a double click. */
const PHONE_RACE_INTRO = { title: "Heritage", text: "Choose where your character comes from. Tap an entry in the list to read about it under the entry; choose it with its circle, the Choose button or a double tap." };

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
  const [described, setDescribed] = useState<{ tab: string; id: string; isElement: boolean; choice?: PickChoice; slotId?: string } | null>(null);
  const [missingOpen, setMissingOpen] = useState(false);
  const phone = usePhone();
  const leftRef = useRef<HTMLDivElement>(null);
  const next = snapshot?.progress.nextSlotId ?? null;
  const flowTab = snapshot?.slots.find((s) => s.id === next)?.tab;
  // Selection flow (setting): jump to the first pending pick after every change.
  useEffect(() => {
    if (!settings.selectionFlow || !next || !flowTab || flowTab === "spells") return;
    const t = setTimeout(() => nav(`/builder/${flowTab}?slot=${encodeURIComponent(next)}`), 250);
    return () => clearTimeout(t);
  }, [settings.selectionFlow, next, flowTab, nav]);
  if (!snapshot) return null;
  // B14: the ruleset's word is known from the character, so the first paint never waits for the profile (no "Race" flash).
  const speciesWord = profile?.terminology.species ?? speciesTerm(snapshot.ruleset);
  // Companions added by hand (GitHub #13) live on Manage > Companion only; the builder tab is for the granted one.
  const hasCompanion = snapshot.companions.some((c) => c.granted) || snapshot.slots.some((s) => s.tab === "companion");
  const tabs: TabDef[] = [
    { id: "race", label: speciesWord },
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
  const addedKind = ADDED_KIND[active];
  // Tester B8 (TV2, U24): Feats also lists the feats the build grants outright, read-only, on phones and desktop.
  const granted = active === "feats" ? grantedFeats(snapshot) : [];
  const slots = snapshot.slots.filter((s) => s.tab === active && SLOT_TABS.includes(s.tab) && !s.locked); // locked slots are hidden (E31 SO-3)
  // List entries (e.g. a background's ability increase) are not elements: the pane keeps the last element instead.
  const elementChoices = slots.filter((s) => s.chosen && s.chosen.type !== "list");
  const defaultId = elementChoices[0]?.chosen?.id ?? null;
  const own = described?.tab === active ? described : null;
  const shownId = own ? own.id : defaultId;
  const paneId = own?.isElement ? own.id : defaultId;
  const onDescribe: DescribeFn = (id, isElement = true, choice, at) => {
    // A2 / TV1 (phones): a second tap on the row whose description is open under it closes it.
    const reTap = phone && at?.toggle && own?.isElement && own.id === id && own.slotId === at.slotId;
    setDescribed(reTap ? null : { tab: active, id, isElement, choice, slotId: at?.slotId });
  };
  // A2 / TV1 (phones): a row's description opens inline under it; the pane below the lists then stays away (one card).
  const inline = phone && own?.isElement && own.slotId ? { slotId: own.slotId, id: own.id } : null;
  // O2 (phones): the described row's "Choose <name>" button, while its slot does not already hold it.
  const choice = phone && own?.choice && snapshot.slots.some((s) => s.id === own.choice!.slotId && s.chosen?.id !== own.id) ? own.choice : null;
  const intro = phone && active === "race" ? PHONE_RACE_INTRO : INTRO[active] ?? INTRO.race!;
  const species = phone ? speciesWord : speciesWord.toUpperCase();
  const fallback: ReactNode = <PaneShell title={active === "race" ? species : intro.title} imageIcon={false} intro={intro.text} readText={intro.text} />;
  // A11: a plain sub-tab tap starts the new tab at its top (the page scroller keeps the old offset otherwise).
  const openTab = (id: string) => {
    if (id !== active) scrollToStart(leftRef.current);
    nav(`/builder/${id}`);
  };
  // Tester B5 (TV2, U24): a described ability option (a list entry) shows its resulting scores instead, on every platform.
  const option = describedAbilityOption(own, slots);
  const pane = active === "abilities" ? <AbilitiesPane />
    : option ? <AbilityOptionPane abilities={snapshot.abilities} slot={option.slot} text={option.text} />
    : <DescriptionPane elementId={paneId} fallback={fallback} />;
  return (
    <div className="bp">
      <h1 className="sr-only">Build</h1>
      <div className="bp-left" ref={leftRef}>
        {snapshot.level.levelUpAvailable && (
          <Alert tone="success"><strong>Level up available.</strong> <button type="button" className="bp-linkbtn" onClick={() => setLevelUpOpen(true)}>Open Level Up</button> to advance.</Alert>
        )}
        <NoticesBanner notices={snapshot.notices}
          action={snapshot.missingContent.length > 0 && <ShowMissingContentButton onClick={() => setMissingOpen(true)} />} />
        {missingOpen && <CharacterMissingContentDialog onClose={() => setMissingOpen(false)} />}
        <Tabs inBand tabs={tabs} active={active} onChange={openTab} label="Build sections" idPrefix="build" />
        <TabPanel active={active} idPrefix="build">
          {granted.length > 0 && <GrantedFeats feats={granted} onDescribe={(id) => onDescribe(id)} />}
          {active === "abilities" ? <AbilitiesTab describedId={shownId} onDescribe={onDescribe} />
            : !(granted.length > 0 && slots.length === 0) && <SlotList slots={slots} describedId={shownId} onDescribe={onDescribe} inline={inline} />}
          {/* B3 (GitHub #35): extras the DM grants, after the section's own picks */}
          {addedKind && <AddedPicks kind={addedKind} onDescribe={(id) => onDescribe(id)} />}
        </TabPanel>
        <div className="sr-only" aria-live="polite">{snapshot.progress.remaining} choices remaining</div>
      </div>
      {!inline && <div className="bp-right">{pane}</div>}
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
