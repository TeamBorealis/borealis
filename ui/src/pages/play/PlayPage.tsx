import { useState } from "react";
import type { CharacterSnapshot } from "../../api/contract";
import { ExplainStat, Stat, StatRow, TabPanel, Tabs, signed } from "../../components";
import { RequireCharacter } from "../../components/RequireCharacter";
import { useCharacter } from "../../state/CharacterContext";
import { AbilitiesPanel } from "./AbilitiesPanel";
import { AttackDetail, AttacksPanel } from "./AttacksPanel";
import { HitDiceCard } from "./DiceCards";
import { HitPointsCard } from "./HitPointsCard";
import { ExhaustionCard, PlayDetail, type PlaySelection, ResourcesCard, TogglesCard } from "./PlayCards";
import { PlaySplit } from "./PlaySplit";
import { RestBar } from "./RestBar";
import { RollProvider } from "./rolls/RollContext";
import { RollHistoryButton } from "./rolls/RollResult";
import { ConcentrationBanner, SpellSlotsCard } from "./SpellSlotsCard";
import { SpellDetail, SpellsPanel } from "./SpellsPanel";
import { TrackersCard } from "./TrackersCard";
import { DefensesCard, FeatureDetail, FeaturesCard, SensesCard, SituationalCard } from "./TraitsPanel";
import "./play-sheet.css";

/**
 * GitHub #14 / #1 (U16(b)): a table session needs only this tab. Sub-tabs, not more cards, keep each list short on a
 * phone and give each its own description pane: Play (rests, hit points, hit dice, slots, resources), Abilities (initiative, checks,
 * saves, skills), Attacks, Spells (casters only) and Traits.
 */
function tabsFor(s: CharacterSnapshot) {
  return [
    { id: "play", label: "Play" },
    { id: "abilities", label: "Abilities" },
    { id: "attacks", label: "Attacks" },
    ...(s.spellcasting.classes.length > 0 ? [{ id: "spells", label: "Spells" }] : []),
    { id: "traits", label: "Traits" },
  ];
}

/** Recent rolls, and a note when Exhaustion's D20 Test penalty is in every roll. */
function RollBar({ s }: { s: CharacterSnapshot }) {
  const penalty = s.play.exhaustion.d20Penalty;
  return (
    <div className="pr-bar" role="group" aria-label="Rolls">
      {penalty < 0 && <span className="v5-note pr-bar-note">Exhaustion {s.play.exhaustion.level}: {penalty} is in every d20 roll.</span>}
      <RollHistoryButton characterId={s.id} />
    </div>
  );
}

function StatTiles({ s }: { s: CharacterSnapshot }) {
  const tiles = [
    { stat: "ac", label: "AC", value: String(s.stats.armorClass) },
    { stat: "initiative", label: "Initiative", value: signed(s.stats.initiative) },
    { stat: "speed", label: "Speed", value: `${s.play.speeds.walk} ft` },
    { stat: "hp", label: "Max HP", value: String(s.stats.hpMax) },
    { stat: "proficiency", label: "Proficiency", value: signed(s.stats.proficiencyBonus) },
  ];
  return (
    <StatRow>
      {tiles.map((t) => (
        <ExplainStat key={t.stat} stat={t.stat} label={t.label} value={t.value}>
          <Stat label={t.label} value={t.value} sub="tap to explain" valueTestId={`explain-${t.stat}-value`} />
        </ExplainStat>
      ))}
    </StatRow>
  );
}

function selectionName(s: CharacterSnapshot, sel: PlaySelection): string | null {
  if (!sel) return null;
  const list = sel.kind === "resource" ? s.play.resources : s.play.toggles;
  return list.find((x) => x.id === sel.id)?.name ?? null;
}

function Body({ s }: { s: CharacterSnapshot }) {
  const { play } = s;
  const [tab, setTab] = useState("play");
  const [sel, setSel] = useState<PlaySelection>(null);
  const [featId, setFeatId] = useState<string | null>(null);
  const [attackId, setAttackId] = useState<string | null>(null);
  const [spellId, setSpellId] = useState<string | null>(null);
  const tabs = tabsFor(s);
  const active = tabs.some((t) => t.id === tab) ? tab : "play";
  const statRow = <StatTiles s={s} />;
  const attack = s.attacks.find((a) => a.id === attackId);
  const spell = s.spellcasting.classes.flatMap((c) => c.spells).find((x) => x.id === spellId);
  const feature = play.features.find((f) => f.id === featId);
  const panes: Record<string, () => JSX.Element> = {
    // GitHub #2: the rests for the whole page first, then concentration pinned, then the stats and the cards the rests affect:
    // hit points and hit dice next to the rests (B-194), then slots, resources and trackers.
    play: () => <PlaySplit label="Play" title={selectionName(s, sel)} onClose={() => setSel(null)}
      detail={<PlayDetail selected={sel} resources={play.resources} toggles={play.toggles} />}
      list={(<><RestBar /><ConcentrationBanner concentration={play.concentration} />{statRow}<HitPointsCard hp={play.hitPoints} />
        <HitDiceCard hitDice={play.hitDice} /><SpellSlotsCard play={play} />
        <ResourcesCard resources={play.resources} toggles={play.toggles} selected={sel} onSelect={setSel} /><TrackersCard trackers={play.trackers} />
        <TogglesCard toggles={play.toggles} resources={play.resources} selected={sel} onSelect={setSel} /><ExhaustionCard exhaustion={play.exhaustion} inspiration={play.inspiration} /></>)} />,
    abilities: () => <PlaySplit label="Abilities" title={null} onClose={() => undefined}
      detail={<><h1 className="v5-desc-title">Abilities</h1><p className="v5-lead">Tap a value to roll it. Long-press, right-click or the arrow beside it rolls with advantage or disadvantage.</p></>}
      list={<div className="pr-cols"><RollBar s={s} /><AbilitiesPanel s={s} /></div>} />,
    attacks: () => <PlaySplit label="Attacks" title={attack?.name ?? null} onClose={() => setAttackId(null)}
      detail={<AttackDetail s={s} attack={attack} />} list={<div className="pr-cols pr-one"><RollBar s={s} /><AttacksPanel s={s} selectedId={attackId} onSelect={setAttackId} /></div>} />,
    spells: () => <PlaySplit label="Spells" title={spell?.name ?? null} onClose={() => setSpellId(null)}
      detail={<SpellDetail s={s} spellId={spellId} />} list={<div className={`pr-cols ${s.spellcasting.classes.length > 1 ? "" : "pr-one"}`}><RollBar s={s} /><SpellsPanel s={s} selectedId={spellId} onSelect={setSpellId} /></div>} />,
    traits: () => <PlaySplit label="Traits" title={feature?.name ?? null} onClose={() => setFeatId(null)} detail={<FeatureDetail feature={feature} />}
      list={<>{statRow}<FeaturesCard features={play.features} selectedId={featId} onSelect={setFeatId} />
        <DefensesCard play={play} /><SensesCard play={play} /><SituationalCard play={play} /></>} />,
  };
  return (
    <div className="v5-page">
      <h1 className="sr-only">Play</h1>
      <Tabs inBand tabs={tabs} active={active} onChange={setTab} label="Play sections" idPrefix="play" />
      <TabPanel active={active} idPrefix="play">{panes[active]!()}</TabPanel>
    </div>
  );
}

function Loaded() {
  const { snapshot } = useCharacter();
  if (!snapshot) return null;
  return <RollProvider characterId={snapshot.id}><Body s={snapshot} /></RollProvider>;
}

export function PlayPage() {
  return <RequireCharacter><Loaded /></RequireCharacter>;
}
