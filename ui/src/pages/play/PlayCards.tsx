import type { PlayExhaustion, PlayResource, PlayToggle } from "../../api/contract";
import { Button, EmptyState, IconButton, Switch } from "../../components";
import { Expander } from "../../components/controls";
import { Tag } from "../ai/Panes";
import { haptic } from "../../lib/haptics";
import { useCharacter } from "../../state/CharacterContext";
import { RolledChargesEditor, chargesLabel, rolledChargeOf } from "../equipment/RolledCharges";

const RECHARGE_LABEL: Record<string, string> = { short_rest: "Short Rest", long_rest: "Long Rest", initiative: "Initiative", dawn: "Dawn" };
export const rechargeText = (r: PlayResource): string => r.recharge.map((x) => RECHARGE_LABEL[x] ?? x.replace(/_/g, " ")).join(" / ");

/** Id prefix of the use pools the engine makes for spells cast without a slot (`free_cast:<granter>:<spell>`). */
const FREE_CAST_PREFIX = "free_cast:";

/** The grey text on a resource row: its source, plus the recharge period for a free cast ("per long rest") or the rolled maximum. */
export function resourceMeta(r: PlayResource, rolledMax: string | null): string {
  if (rolledMax) return `${r.source} · max ${rolledMax}`;
  if (!r.id.startsWith(FREE_CAST_PREFIX)) return r.source;
  const period = r.recharge[0]?.replace(/_/g, " ");
  return `${r.source} · ${period && period !== "none" ? `per ${period}` : "no recharge"}`;
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

function ResourceRow({ r, selected, onSelect }: { r: PlayResource } & Pickable) {
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
      </button>
      <Pips r={r} />
      <span className="v5-count" role="status" aria-label={`${r.name}: ${r.current} of ${r.max} uses left`}>{r.current}/{r.max}</span>
      <span className="v5-ctrls">
        <Button small disabled={r.current <= 0} aria-label={`Spend ${r.name}`} onClick={spend}>Spend</Button>
        <Button small disabled={r.current >= r.max} aria-label={`Restore ${r.name}`} onClick={restore}>Restore</Button>
      </span>
    </li>
  );
}

export function ResourcesCard({ resources, selected, onSelect }: { resources: PlayResource[] } & Pickable) {
  const { mutate } = useCharacter();
  return (
    <Expander title="Resources" headerExtra={<>
      <Button small icon="sparkles" onClick={() => { haptic("rest"); void mutate((api, id) => api.shortRest({ characterId: id })); }}>Short Rest</Button>
      <Button small variant="primary" icon="star" onClick={() => { haptic("rest"); void mutate((api, id) => api.longRest({ characterId: id })); }}>Long Rest</Button>
    </>}>
      {resources.length === 0 ? <EmptyState title="No limited-use resources" /> : <ul className="v5-list" aria-label="Resources">{resources.map((r) => <ResourceRow key={r.id} r={r} selected={selected} onSelect={onSelect} />)}</ul>}
    </Expander>
  );
}

/** Speed effects of a state ("Walk speed x2", "Fly speed 2x walk speed"), shown on its row. */
export const speedEffects = (t: PlayToggle): string[] => t.effects.filter((e) => /\bspeed\b/i.test(e));

export function TogglesCard({ toggles, selected, onSelect }: { toggles: PlayToggle[] } & Pickable) {
  const { mutate } = useCharacter();
  if (toggles.length === 0) return null;
  return (
    <Expander title="States">
      <ul className="v5-list" aria-label="States">
        {toggles.map((t) => {
          const on = selected?.kind === "toggle" && selected.id === t.id;
          return (
            <li key={t.id} className={`v5-line ${on ? "is-selected" : ""}`}>
              <button type="button" className="v5-row grow" aria-current={on} onClick={() => onSelect({ kind: "toggle", id: t.id })}>
                <span className="grow">{t.name}</span>
                {speedEffects(t).length > 0 && <span className="meta">{speedEffects(t).join(", ")}</span>}
                {t.on && t.blockedBy && <Tag tone="bad">No effect</Tag>}
                {t.active && <Tag tone="ok">Active</Tag>}
              </button>
              <span className="v5-ctrls"><Switch label={t.name} checked={t.on} onChange={(e) => void mutate((api, id) => api.setToggle({ characterId: id, toggleId: t.id, on: e.target.checked }))} /></span>
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
  if (r) return (<><h1 className="v5-desc-title">{r.name}</h1><p className="v5-lead">{r.source}</p>
    <h2>Uses</h2><p>{r.current} of {r.max} left{r.recharge.length ? `. Resets on ${rechargeText(r)}.` : "."}</p>
    {rolled && <><h2>Maximum</h2><RolledChargesEditor item={rolled.item} charge={rolled.charge} /></>}</>);
  if (t) return (<><h1 className="v5-desc-title">{t.name}</h1><p className="v5-lead">{[t.activation, t.duration].filter(Boolean).join(" · ") || "State"}</p>
    {t.effects.length > 0 && <><h2>Effects</h2><ul>{t.effects.map((e) => <li key={e}>{e}</li>)}</ul></>}
    {t.on && t.blockedBy && <p className="error-text">No effect: {t.blockedBy}</p>}</>);
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
