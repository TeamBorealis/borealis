import { useId } from "react";
import type { PlayConcentrationSpell, PlaySlotUse, PlayState } from "../../api/contract";
import { Button, Icon } from "../../components";
import { ComboBox, Expander } from "../../components/controls";
import { useCharacter } from "../../state/CharacterContext";
import { PipButtons, UsesLeft } from "./PipButtons";
import "./spell-tracker.css";

/** A change of a slot pool: `used` slots of `level`, or of the Pact Magic pool. */
export interface SlotSet { pact: boolean; level: number; used: number }

function SlotRow({ label, meta, use, onSet }: { label: string; meta: string; use: PlaySlotUse; onSet: (used: number) => void }) {
  return (
    <li className="v5-line st-line">
      <span className="v5-row static grow"><span className="grow">{label}</span><span className="meta">{meta}</span></span>
      <PipButtons max={use.max} used={use.used} onSet={onSet} pipName={(n, max, used) => `${label} slot ${n} of ${max}, ${used ? "used" : "available"}`} />
      <UsesLeft name={`${label} slots`} max={use.max} used={use.used} />
    </li>
  );
}

/** One row per spell level with slots (back on a long rest), then the Pact Magic slots (back on a short rest). */
export function SlotRows({ slots, pact, onSet }: { slots: PlaySlotUse[]; pact: PlaySlotUse | null; onSet: (s: SlotSet) => void }) {
  return (<>
    {slots.map((s) => <SlotRow key={s.level} label={`Level ${s.level}`} meta="Long Rest" use={s} onSet={(used) => onSet({ pact: false, level: s.level, used })} />)}
    {pact && <SlotRow label="Pact Magic" meta={`Level ${pact.level} · Short Rest`} use={pact} onSet={(used) => onSet({ pact: true, level: pact.level, used })} />}
  </>);
}

const spellLabel = (s: PlayConcentrationSpell) => `${s.name} (${s.level === 0 ? "cantrip" : `level ${s.level}`})`;

function ConcentrationRow({ spells, current }: { spells: PlayConcentrationSpell[]; current: PlayConcentrationSpell | null }) {
  const { mutate } = useCharacter();
  const id = useId();
  const options = [{ value: "", label: "Not concentrating" }, ...spells.map((s) => ({ value: s.id, label: spellLabel(s) }))];
  return (
    <li className="v5-line st-line st-conc-line">
      <label className="v5-row static grow" htmlFor={id}><span className="grow">Concentration</span></label>
      <span className="v5-ctrls">
        <ComboBox id={id} aria-label="Concentrate on" className="st-conc-pick" value={current?.id ?? ""} options={options}
          onChange={(e) => { const spellId = e.target.value || null; void mutate((api, cid) => api.setConcentration({ characterId: cid, spellId })); }} />
      </span>
    </li>
  );
}

/** Spell slot pips per level, the Pact Magic row and the concentration picker; absent for characters that cast no spells. */
export function SpellSlotsCard({ play }: { play: PlayState }) {
  const { mutate } = useCharacter();
  const hasConcentration = play.concentrationSpells.length > 0 || play.concentration !== null;
  if (play.spellSlots.length === 0 && !play.pactSlots && !hasConcentration) return null;
  const set = ({ pact, level, used }: SlotSet) =>
    void mutate((api, id) => api.setSpellSlotsUsed({ characterId: id, level, used, pact }));
  return (
    <Expander title="Spell Slots">
      <ul className="v5-list" aria-label="Spell slots">
        <SlotRows slots={play.spellSlots} pact={play.pactSlots} onSet={set} />
        {hasConcentration && <ConcentrationRow spells={play.concentrationSpells} current={play.concentration} />}
      </ul>
    </Expander>
  );
}

/** The spell being concentrated on, pinned at the top of Play; one tap ends it. */
export function ConcentrationBanner({ concentration }: { concentration: PlayConcentrationSpell | null }) {
  const { mutate } = useCharacter();
  if (!concentration) return null;
  const end = () => void mutate((api, id) => api.setConcentration({ characterId: id, spellId: null }));
  return (
    <section className="st-conc" aria-label="Concentration">
      <Icon name="star" />
      <span className="st-conc-text">Concentrating on <strong>{concentration.name}</strong></span>
      <Button small aria-label={`End concentration on ${concentration.name}`} onClick={end}>End</Button>
    </section>
  );
}
