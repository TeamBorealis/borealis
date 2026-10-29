import { useState } from "react";
import { ExplainStat, Stat, StatRow, TabPanel, Tabs, signed } from "../../components";
import { RequireCharacter } from "../../components/RequireCharacter";
import { useCharacter } from "../../state/CharacterContext";
import { PaneSplit } from "../ai/Panes";
import { ExhaustionCard, PlayDetail, type PlaySelection, ResourcesCard, TogglesCard } from "./PlayCards";
import { ConcentrationBanner, SpellSlotsCard } from "./SpellSlotsCard";
import { TrackersCard } from "./TrackersCard";
import { DefensesCard, FeatureDetail, FeaturesCard, SensesCard, SituationalCard } from "./TraitsPanel";

function Body() {
  const { snapshot } = useCharacter();
  const [tab, setTab] = useState("play");
  const [sel, setSel] = useState<PlaySelection>(null);
  const [featId, setFeatId] = useState<string | null>(null);
  if (!snapshot) return null;
  const { stats, play } = snapshot;
  const tiles: { stat: string; label: string; value: string }[] = [
    { stat: "ac", label: "AC", value: String(stats.armorClass) },
    { stat: "initiative", label: "Initiative", value: signed(stats.initiative) },
    { stat: "speed", label: "Speed", value: `${play.speeds.walk} ft` },
    { stat: "hp", label: "Max HP", value: String(stats.hpMax) },
    { stat: "proficiency", label: "Proficiency", value: signed(stats.proficiencyBonus) },
  ];
  const statRow = (
    <StatRow>
      {tiles.map((t) => (
        <ExplainStat key={t.stat} stat={t.stat} label={t.label} value={t.value}>
          <Stat label={t.label} value={t.value} sub="tap to explain" valueTestId={`explain-${t.stat}-value`} />
        </ExplainStat>
      ))}
    </StatRow>
  );
  // GitHub #2: concentration pinned first, spell slots before the resources (their rests live in the Resources header).
  const playList = (<><ConcentrationBanner concentration={play.concentration} />{statRow}<SpellSlotsCard play={play} />
    <ResourcesCard resources={play.resources} selected={sel} onSelect={setSel} /><TrackersCard trackers={play.trackers} />
    <TogglesCard toggles={play.toggles} selected={sel} onSelect={setSel} /><ExhaustionCard exhaustion={play.exhaustion} inspiration={play.inspiration} /></>);
  const traitList = (<>{statRow}<FeaturesCard features={play.features} selectedId={featId} onSelect={setFeatId} />
    <DefensesCard play={play} /><SensesCard play={play} /><SituationalCard play={play} /></>);
  return (
    <div className="v5-page">
      <h1 className="sr-only">Play</h1>
      <Tabs inBand tabs={[{ id: "play", label: "Play" }, { id: "traits", label: "Traits" }]} active={tab} onChange={setTab} label="Play sections" idPrefix="play" />
      <TabPanel active={tab} idPrefix="play">
        {tab === "play"
          ? <PaneSplit label="Play" list={playList} desc={<PlayDetail selected={sel} resources={play.resources} toggles={play.toggles} />} />
          : <PaneSplit label="Traits" list={traitList} desc={<FeatureDetail feature={play.features.find((f) => f.id === featId)} />} />}
      </TabPanel>
    </div>
  );
}

export function PlayPage() {
  return <RequireCharacter><Body /></RequireCharacter>;
}
