import { useId } from "react";
import type { PlayConcentrationSpell, PlaySlotUse, PlayState } from "../../api/contract";
import { Button, Icon } from "../../components";
import { ComboBox, Expander } from "../../components/controls";
import { PipButtons, UsesLeft } from "./PipButtons";
import { usePlayMutate } from "./playQueue";
import "./spell-tracker.css";

/** One press on a slot pool: one more slot of `level` (or of the Pact Magic pool) used, or one given back. */
export interface SlotPress { pact: boolean; level: number; spend: boolean }

function SlotRow({ label, meta, what, use, onPress }: { label: string; meta: string; what: string; use: PlaySlotUse; onPress: (spend: boolean) => void }) {
  const state = `${use.max - use.used} of ${use.max} left`;
  return (
    <li className="v5-line st-line">
      <span className="v5-row static grow"><span className="grow">{label}</span><span className="meta">{meta}</span></span>
      <PipButtons label={`${label} slots`} max={use.max} used={use.used} spendName={`Use a ${what}, ${state}`} restoreName={`Restore a ${what}, ${state}`}
        onSpend={() => onPress(true)} onRestore={() => onPress(false)} />
      <UsesLeft name={`${label} slots`} max={use.max} used={use.used} />
    </li>
  );
}

/** One row per spell level with slots (back on a long rest), then the Pact Magic slots (back on a short rest). */
export function SlotRows({ slots, pact, onPress }: { slots: PlaySlotUse[]; pact: PlaySlotUse | null; onPress: (p: SlotPress) => void }) {
  return (<>
    {slots.map((s) => <SlotRow key={s.level} label={`Level ${s.level}`} meta="Long Rest" what={`level ${s.level} slot`} use={s}
      onPress={(spend) => onPress({ pact: false, level: s.level, spend })} />)}
    {pact && <SlotRow label="Pact Magic" meta={`Level ${pact.level} · Short Rest`} what="Pact Magic slot" use={pact}
      onPress={(spend) => onPress({ pact: true, level: pact.level, spend })} />}
  </>);
}

const spellLabel = (s: PlayConcentrationSpell) => `${s.name} (${s.level === 0 ? "cantrip" : `level ${s.level}`})`;

function ConcentrationRow({ spells, current }: { spells: PlayConcentrationSpell[]; current: PlayConcentrationSpell | null }) {
  const run = usePlayMutate();
  const id = useId();
  const options = [{ value: "", label: "Not concentrating" }, ...spells.map((s) => ({ value: s.id, label: spellLabel(s) }))];
  return (
    <li className="v5-line st-line st-conc-line">
      <label className="v5-row static grow" htmlFor={id}><span className="grow">Concentration</span></label>
      <span className="v5-ctrls">
        <ComboBox id={id} aria-label="Concentrate on" className="st-conc-pick" value={current?.id ?? ""} options={options}
          onChange={(e) => { const spellId = e.target.value || null; void run((api, cid) => api.setConcentration({ characterId: cid, spellId })); }} />
      </span>
    </li>
  );
}

/** Spell slot pips per level, the Pact Magic row and the concentration picker; absent for characters that cast no spells. */
export function SpellSlotsCard({ play }: { play: PlayState }) {
  const run = usePlayMutate();
  const hasConcentration = play.concentrationSpells.length > 0 || play.concentration !== null;
  if (play.spellSlots.length === 0 && !play.pactSlots && !hasConcentration) return null;
  const press = ({ pact, level, spend }: SlotPress) => void run((api, characterId) => (spend
    ? api.spendSpellSlot({ characterId, level, pact })
    : api.restoreSpellSlot({ characterId, level, pact })));
  return (
    <Expander title="Spell Slots">
      <ul className="v5-list" aria-label="Spell slots">
        <SlotRows slots={play.spellSlots} pact={play.pactSlots} onPress={press} />
        {hasConcentration && <ConcentrationRow spells={play.concentrationSpells} current={play.concentration} />}
      </ul>
    </Expander>
  );
}

/** The spell being concentrated on, pinned at the top of Play; one tap ends it. */
export function ConcentrationBanner({ concentration }: { concentration: PlayConcentrationSpell | null }) {
  const run = usePlayMutate();
  if (!concentration) return null;
  const end = () => void run((api, id) => api.setConcentration({ characterId: id, spellId: null }));
  return (
    <section className="st-conc" aria-label="Concentration">
      <Icon name="star" />
      <span className="st-conc-text">Concentrating on <strong>{concentration.name}</strong></span>
      <Button small aria-label={`End concentration on ${concentration.name}`} onClick={end}>End</Button>
    </section>
  );
}
