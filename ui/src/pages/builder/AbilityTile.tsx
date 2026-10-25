import { useRef } from "react";
import { ABILITY_KEYS, type AbilityKey, type AbilityState } from "../../api/contract";
import { ComboBox, Icon, IconButton } from "../../components";
import { useHoldRepeat } from "../../components/useHoldRepeat";
import { ABILITY_LABEL } from "../../lib/format";

/** "Mountain Dwarf +2" -> "Mountain Dwarf (2)": classic builder lists each bonus source with its value in parentheses. */
export const sourceLine = (s: string): string => s.replace(/\s*\+?(-?\d+)$/, " ($1)");

const signed = (n: number) => (n >= 0 ? `+${n}` : `${n}`);

export interface AbilityTileProps {
  ability: AbilityKey; state: AbilityState; onSet: (v: number) => void; onRoll: () => void; onAssign: (v: number | null) => void;
}

/** AbilitiesSpinner: framed score tile (name / score / modifier pill) + chevron pair around the base value + bonus sources. */
function AbilitySpinner({ ability, state, onSet, onRoll, onAssign }: AbilityTileProps) {
  const { method, base, final, modifier, pointBuy, standardArray, overCap, rolls, bonusSources } = state;
  const label = ABILITY_LABEL[ability];
  const min = pointBuy?.min ?? 3;
  const max = pointBuy?.max ?? 30;
  const cur = base[ability];
  const step = pointBuy?.costs[cur + 1];
  const canUp = cur < max && (!pointBuy || pointBuy.unrestricted || (step !== undefined && pointBuy.remaining >= step - (pointBuy.costs[cur] ?? 0)));
  const canDown = cur > min;
  const chevrons = method === "point_buy" || method === "manual";
  const last = [...rolls].reverse().find((r) => r.ability === ability);
  const rollable = method === "roll_3d6" || method === "roll_4d6";
  const latest = useRef({ cur, canUp, canDown, onSet });
  latest.current = { cur, canUp, canDown, onSet };
  const hold = useHoldRepeat();
  const stepBy = (d: number) => () => {
    const l = latest.current;
    if (d > 0 ? !l.canUp : !l.canDown) return;
    l.onSet(l.cur + d);
  };
  return (
    <div className="abil" role="group" aria-label={label}>
      <div className="abil-main">
        <div className={`abil-frame ${overCap.includes(ability) ? "over" : ""}`}>
          <span className="abil-name">{label}</span>
          <span className="abil-score" aria-label={`${label} final score ${final[ability]}`}>{final[ability]}</span>
          <span className="abil-mod" aria-label={`modifier ${signed(modifier[ability])}`}>{signed(modifier[ability])}</span>
        </div>
        <div className="abil-spin">
          {chevrons && <button type="button" className="abil-chev" aria-label={`Increase ${label}`} disabled={!canUp} {...hold(stepBy(1))}><Icon name="up" /></button>}
          <span className="abil-base" title="Base score">{cur}</span>
          {chevrons && <button type="button" className="abil-chev" aria-label={`Decrease ${label}`} disabled={!canDown} {...hold(stepBy(-1))}><Icon name="down" /></button>}
        </div>
      </div>
      <div className="abil-src">
        {bonusSources[ability].map((s) => <span key={s}>{sourceLine(s)}</span>)}
        {overCap.includes(ability) && <strong className="danger-text">Over the {state.cap} cap</strong>}
      </div>
      {rollable && (
        <div className="abil-extra">
          <IconButton icon="dice" label={`Roll ${label}`} onClick={onRoll} />
          {last && <span className="small muted">{last.dice.join(" + ")} = {last.total}</span>}
        </div>
      )}
      {method === "standard_array" && standardArray && (
        <div className="abil-extra">
          <ComboBox aria-label={`${label} array value`} value={String(standardArray.assigned[ability] ?? "")}
            onChange={(e) => onAssign(e.target.value === "" ? null : Number(e.target.value))}
            options={[{ value: "", label: "Unassigned" }, ...standardArray.values.map((v) => ({ value: String(v), label: `${v}${standardArray.assigned[ability] !== v && !standardArray.unassigned.includes(v) ? " (swap)" : ""}` }))]} />
        </div>
      )}
    </div>
  );
}

export function AbilityGrid(props: Omit<AbilityTileProps, "ability" | "onSet" | "onRoll" | "onAssign"> & {
  onSet: (a: AbilityKey, v: number) => void; onRoll: (a: AbilityKey) => void; onAssign: (a: AbilityKey, v: number | null) => void;
}) {
  return (
    <div className="abil-row">
      {ABILITY_KEYS.map((a) => (
        <AbilitySpinner key={a} ability={a} state={props.state} onSet={(v) => props.onSet(a, v)} onRoll={() => props.onRoll(a)} onAssign={(v) => props.onAssign(a, v)} />
      ))}
    </div>
  );
}
