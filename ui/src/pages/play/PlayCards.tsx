import type { PlayExhaustion, PlayResource, PlayToggle } from "../../api/contract";
import { Button, EmptyState, IconButton, Switch } from "../../components";
import { Expander } from "../../components/controls";
import { Tag } from "../ai/Panes";
import { useCharacter } from "../../state/CharacterContext";
import { RolledChargesEditor, chargesLabel, rolledChargeOf } from "../equipment/RolledCharges";
import { ResourceDieButton } from "./DiceCards";

/** When a use comes back besides the rests ("Recharges when you roll Initiative"). */
const TRIGGER_TEXT: Record<string, string> = { initiative: "when you roll Initiative", turn_start: "at the start of your turn", dawn: "at dawn" };
const REST_RECHARGES = new Set(["short_rest", "long_rest", "none"]);

/**
 * B-141: how a pool's uses come back, from the engine's rest rules ("1 back on a Short Rest, all on a Long Rest" for Rage),
 * never just the rest names: a Short Rest gives back only what `shortRestRegain` says.
 */
export function rechargeText(r: PlayResource): string {
  const short = r.shortRestRegain;
  const rests = short !== null && short >= r.max ? (r.longRestRegainAll ? "All back on a Short or Long Rest" : "All back on a Short Rest")
    : short !== null && short > 0 ? `${short} back on a Short Rest${r.longRestRegainAll ? ", all on a Long Rest" : ""}`
      : r.longRestRegainAll ? "All back on a Long Rest" : "";
  const others = r.recharge.filter((x) => !REST_RECHARGES.has(x)).map((x) => TRIGGER_TEXT[x] ?? x.replace(/_/g, " "));
  if (others.length === 0) return rests || "No recharge";
  return rests ? `${rests}, or ${others.join(" or ")}` : `Recharges ${others.join(" or ")}`;
}

/** The grey text on a resource row: how its uses come back, after its source when that is not just the name again (B-141). */
export function resourceMeta(r: PlayResource, rolledMax: string | null): string {
  return [r.source !== r.name ? r.source : null, rechargeText(r), rolledMax ? `max ${rolledMax}` : null].filter(Boolean).join(" · ");
}

function Pips({ r }: { r: PlayResource }) {
  if (r.max > 12) return <span className="v5-count" aria-hidden="true">{r.current}/{r.max}</span>;
  return (
    <span className="v5-pips" aria-hidden="true">
      {Array.from({ length: r.max }, (_, i) => <span key={i} className={i < r.current ? "v5-pip full" : "v5-pip"} />)}
    </span>
  );
}

export type PlaySelection = { kind: "resource" | "toggle"; id: string } | null;
interface Pickable { selected: PlaySelection; onSelect: (s: PlaySelection) => void }

/**
 * B-178: a state with a use pool of the same id (Rage) is switched on that pool's row, so spending and switching are in one
 * place (switching it on spends a use, B-140). Such a state is not listed again under States.
 */
export const stateOfPool = (toggles: PlayToggle[], r: PlayResource): PlayToggle | undefined => toggles.find((t) => t.id === r.id);
const hasPool = (resources: PlayResource[], t: PlayToggle): boolean => resources.some((r) => r.id === t.id);

/** A state's tags on its row: switched on but without effect (worn armor), or in effect. */
function StateTags({ t }: { t: PlayToggle }) {
  return (<>{t.on && t.blockedBy && <Tag tone="bad">No effect</Tag>}{t.active && <Tag tone="ok">Active</Tag>}</>);
}

function StateSwitch({ t }: { t: PlayToggle }) {
  const { mutate } = useCharacter();
  return <Switch label={t.name} checked={t.on} onChange={(e) => void mutate((api, id) => api.setToggle({ characterId: id, toggleId: t.id, on: e.target.checked }))} />;
}

function StateEffects({ t }: { t: PlayToggle }) {
  return (<>{t.effects.length > 0 && <><h2>Effects</h2><ul>{t.effects.map((e) => <li key={e}>{e}</li>)}</ul></>}
    {t.on && t.blockedBy && <p className="error-text">No effect: {t.blockedBy}</p>}</>);
}

function ResourceRow({ r, state, selected, onSelect }: { r: PlayResource; state?: PlayToggle } & Pickable) {
  const { snapshot, mutate } = useCharacter();
  const rolled = snapshot ? rolledChargeOf(snapshot, r.id) : null;
  const spend = () => void mutate((api, id) => api.spendResource({ characterId: id, resourceId: r.id }));
  const restore = () => void mutate((api, id) => api.restoreResource({ characterId: id, resourceId: r.id }));
  const on = selected?.kind === "resource" && selected.id === r.id;
  return (
    <li className={`v5-line ${on ? "is-selected" : ""}`}>
      <button type="button" className="v5-row grow" aria-current={on} onClick={() => onSelect({ kind: "resource", id: r.id })}>
        <span className="grow">{r.name}</span>
        <span className="meta">{resourceMeta(r, rolled ? chargesLabel(rolled.charge) : null)}</span>
        {state && <StateTags t={state} />}
      </button>
      <Pips r={r} />
      <span className="v5-count" role="status" aria-label={`${r.name}: ${r.current} of ${r.max} uses left`}>{r.current}/{r.max}</span>
      <span className="v5-ctrls">
        <ResourceDieButton r={r} />
        <Button small disabled={r.current <= 0} aria-label={`Spend ${r.name}`} onClick={spend}>Spend</Button>
        <Button small disabled={r.current >= r.max} aria-label={`Restore ${r.name}`} onClick={restore}>Restore</Button>
        {state && <StateSwitch t={state} />}
      </span>
    </li>
  );
}

/** Limited-use pools. The rests are in the Play page's rest bar (RestBar), above every card they affect. */
export function ResourcesCard({ resources, toggles, selected, onSelect }: { resources: PlayResource[]; toggles: PlayToggle[] } & Pickable) {
  return (
    <Expander title="Resources">
      {resources.length === 0 ? <EmptyState title="No limited-use resources" /> : <ul className="v5-list" aria-label="Resources">{resources.map((r) => <ResourceRow key={r.id} r={r} state={stateOfPool(toggles, r)} selected={selected} onSelect={onSelect} />)}</ul>}
    </Expander>
  );
}

/** Speed effects of a state ("Walk speed x2", "Fly speed 2x walk speed"), shown on its row. */
export const speedEffects = (t: PlayToggle): string[] => t.effects.filter((e) => /\bspeed\b/i.test(e));

/** The states without a same-id use pool (those are switched on the pool's row, B-178). */
export function TogglesCard({ toggles, resources, selected, onSelect }: { toggles: PlayToggle[]; resources: PlayResource[] } & Pickable) {
  const own = toggles.filter((t) => !hasPool(resources, t));
  if (own.length === 0) return null;
  return (
    <Expander title="States">
      <ul className="v5-list" aria-label="States">
        {own.map((t) => {
          const on = selected?.kind === "toggle" && selected.id === t.id;
          return (
            <li key={t.id} className={`v5-line ${on ? "is-selected" : ""}`}>
              <button type="button" className="v5-row grow" aria-current={on} onClick={() => onSelect({ kind: "toggle", id: t.id })}>
                <span className="grow">{t.name}</span>
                {speedEffects(t).length > 0 && <span className="meta">{speedEffects(t).join(", ")}</span>}
                <StateTags t={t} />
              </button>
              <span className="v5-ctrls"><StateSwitch t={t} /></span>
            </li>
          );
        })}
      </ul>
    </Expander>
  );
}

/** Description pane body for the selected resource or state. */
export function PlayDetail({ selected, resources, toggles }: { selected: PlaySelection; resources: PlayResource[]; toggles: PlayToggle[] }) {
  const { snapshot } = useCharacter();
  const r = selected?.kind === "resource" ? resources.find((x) => x.id === selected.id) : undefined;
  const rolled = r && snapshot ? rolledChargeOf(snapshot, r.id) : null;
  const t = selected?.kind === "toggle" ? toggles.find((x) => x.id === selected.id) : undefined;
  const state = r ? stateOfPool(toggles, r) : undefined;
  if (r) return (<><h1 className="v5-desc-title">{r.name}</h1>{r.source !== r.name && <p className="v5-lead">{r.source}</p>}
    <h2>Uses</h2><p>{r.current} of {r.max} left. {rechargeText(r)}.</p>
    {(r.otherRegain?.length ?? 0) > 0 && <ul>{r.otherRegain?.map((x) => <li key={x}>{x}</li>)}</ul>}
    {rolled && <><h2>Maximum</h2><RolledChargesEditor item={rolled.item} charge={rolled.charge} /></>}
    {state && <><h2>State</h2><p>{[state.activation, state.duration].filter(Boolean).join(" · ")}{state.activation || state.duration ? ". " : ""}Switching it on spends one use.</p><StateEffects t={state} /></>}</>);
  if (t) return (<><h1 className="v5-desc-title">{t.name}</h1><p className="v5-lead">{[t.activation, t.duration].filter(Boolean).join(" · ") || "State"}</p>
    <StateEffects t={t} /></>);
  return (<><h1 className="v5-desc-title">Play</h1><p className="v5-lead">Select a resource or state to read its details.</p></>);
}

export function exhaustionSummary(e: PlayExhaustion): string {
  if (e.dead) return "Exhaustion 6: the character dies.";
  if (e.level === 0) return "No exhaustion.";
  const parts = [`${e.d20Penalty} to D20 Tests`, `${e.speedPenalty} ft. speed`];
  return `Level ${e.level}: ${parts.join(", ")}.`;
}

export function ExhaustionCard({ exhaustion, inspiration }: { exhaustion: PlayExhaustion; inspiration: boolean }) {
  const { mutate } = useCharacter();
  const set = (level: number) => void mutate((api, id) => api.setExhaustion({ characterId: id, level }));
  return (
    <Expander title="Conditions">
      <div className="v5-inline">
        <div className="v5-foot">
          <span id="exh-label" className="field-label">Exhaustion</span>
          <IconButton icon="minus" label="Lower exhaustion" variant="default" disabled={exhaustion.level <= 0} onClick={() => set(exhaustion.level - 1)} />
          <span className="v5-exh" role="status" aria-labelledby="exh-label">{exhaustion.level}</span>
          <IconButton icon="plus" label="Raise exhaustion" variant="default" disabled={exhaustion.level >= exhaustion.maxLevels} onClick={() => set(exhaustion.level + 1)} />
          <span className="v5-note">of {exhaustion.maxLevels}</span>
        </div>
        <p className={exhaustion.dead ? "error-text" : "v5-note"} data-testid="exhaustion-summary" style={{ margin: 0 }}>{exhaustionSummary(exhaustion)}</p>
        <div className="v5-foot">
          <Switch label="Heroic Inspiration" checked={inspiration} onChange={(e) => void mutate((api, id) => api.setHeroicInspiration({ characterId: id, has: e.target.checked }))} />
          <span>Heroic Inspiration</span>
        </div>
      </div>
    </Expander>
  );
}
