import type { PlayHitPoints } from "../../api/contract";
import { IconButton } from "../../components";
import { Expander } from "../../components/controls";
import { AmountControl } from "./AmountControl";
import { usePlayMutate } from "./playQueue";
import "./hit-points.css";

/** Largest damage, healing or temporary hit points one change takes (the host's limit too: borealis-engine hit_points.rs). */
export const MAX_HP_CHANGE = 9999;

/**
 * Current and temporary hit points (B-194, Egorobi on Discord). Laid out as the Conditions card's exhaustion row: the
 * value between -1 and +1, then a typed amount with Damage, Heal and Temp. Damage comes off temporary hit points first;
 * hit points stop at 0 and at the maximum; temporary hit points don't stack, so Temp replaces them.
 */
export function HitPointsCard({ hp }: { hp: PlayHitPoints }) {
  const run = usePlayMutate();
  const damage = (amount: number) => void run((api, id) => api.takeDamage({ characterId: id, amount }));
  const heal = (amount: number) => void run((api, id) => api.regainHitPoints({ characterId: id, amount }));
  const setTemp = (amount: number) => void run((api, id) => api.setTempHitPoints({ characterId: id, amount }));
  const untouchable = hp.current <= 0 && hp.temp <= 0;
  return (
    <Expander title="Hit Points">
      <div className="v5-inline">
        <div className="v5-foot hp-foot">
          <span className="field-label">Current</span>
          <IconButton icon="minus" label="Take 1 damage" variant="default" disabled={untouchable} onClick={() => damage(1)} />
          <span className="v5-exh" role="status" aria-label={`Hit points: ${hp.current} of ${hp.max}`}>{hp.current}</span>
          <IconButton icon="plus" label="Regain 1 hit point" variant="default" disabled={hp.current >= hp.max} onClick={() => heal(1)} />
          <span className="v5-note">of {hp.max}</span>
        </div>
        <div className="v5-foot hp-foot">
          <span className="field-label">Temporary</span>
          <span className="v5-exh" role="status" aria-label={`Temporary hit points: ${hp.temp}`}>{hp.temp}</span>
          {hp.temp > 0 && <IconButton icon="x" label="End temporary hit points" variant="default" onClick={() => setTemp(0)} />}
        </div>
        <div className="v5-foot">
          <AmountControl label="Hit point amount" max={MAX_HP_CHANGE} actions={[
            { label: "Damage", name: "Take damage", run: damage, disabled: untouchable },
            { label: "Heal", name: "Regain hit points", run: heal, disabled: hp.current >= hp.max },
            { label: "Temp", name: "Set temporary hit points", run: setTemp, allowZero: true },
          ]} />
        </div>
      </div>
    </Expander>
  );
}
