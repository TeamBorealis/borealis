import type { CompanionState } from "../../api/contract";
import { ABILITY_KEYS } from "../../api/contract";
import { Expander } from "../controls/Expander";

const signed = (n: number) => (n >= 0 ? `+${n}` : `${n}`);
const NO_BREAKDOWN = "n/a";

/** Aurora's score-with-modifier string: "14 (+2)". */
const scoreWithModifier = (score: number) => `${score} (${signed(Math.floor((score - 10) / 2))})`;

/** The creature's own name (the template), not the name the user gave it. */
export const companionDisplayName = (c: CompanionState): string => c.creature?.name ?? c.name;

/** Rows of "Additional Statistics & Breakdown": only statistics that exist, in Aurora's order. */
function breakdownRows(c: CompanionState): [string, string][] {
  const speeds: [string, number][] = [["Speed", c.speed], ["Fly Speed", c.flySpeed], ["Climb Speed", c.climbSpeed], ["Swim Speed", c.swimSpeed], ["Burrow Speed", c.burrowSpeed]];
  return [
    ...speeds.filter(([, v]) => v > 0).map(([name, v]): [string, string] => [name, `${v}ft`]),
    ["AC", c.armorClassNote ?? String(c.armorClass)],
    ["Initiative", signed(c.initiative)],
    ["HP Max", c.hpNote ?? String(c.hp)],
  ];
}

function Features({ title, items }: { title: string; items: CompanionState["traits"] }) {
  return (
    <Expander title={title} defaultOpen={false}>
      {items.map((f, i) => <p key={i} className="qv-feat"><em>{f.name}.</em> {f.text}</p>)}
      {items.length === 0 && <p className="qv-note">None.</p>}
    </Expander>
  );
}

/**
 * The information slider's companion context: the creature's name, build line, the headline strip
 * (Proficiency, Initiative, AC, Speed, HP), the six scores with modifiers and the Skills, Traits, Actions,
 * Reactions and Additional Statistics & Breakdown sections. AC and HP print as text when the stat block gives a formula.
 */
export function CompanionQuickView({ companion: c }: { companion: CompanionState }) {
  const strip: [string, string][] = [
    ["Proficiency", signed(c.proficiency)], ["Initiative", signed(c.initiative)], ["AC", c.armorClassNote ?? String(c.armorClass)],
    ["Speed", `${c.speed}ft`], ["HP", c.hpNote ?? String(c.hp)],
  ];
  return (
    <div className="qv qv-companion" aria-label="Companion">
      <h3 className="qv-build">{c.build}</h3>
      <p className="qv-stats">{strip.map(([k, v]) => <span key={k}>{k}: <b>{v}</b></span>)}</p>
      <div className="qv-abil">
        {ABILITY_KEYS.map((k) => <div key={k}><b>{k}</b><span>{scoreWithModifier(c.abilities[k])}</span></div>)}
      </div>
      <Expander title="Skills" defaultOpen={false}>
        <ul className="qv-list">
          {c.skills.map((s) => <li key={s.name}><em>{signed(s.bonus)}</em><span>{s.name} <small>({s.ability})</small></span></li>)}
        </ul>
        <p className="qv-passive"><em>{c.passivePerception}</em><span>Passive Perception</span></p>
      </Expander>
      <Features title="Traits" items={c.traits} />
      <Features title="Actions" items={c.actions} />
      <Features title="Reactions" items={c.reactions} />
      <Expander title="Additional Statistics & Breakdown" defaultOpen={false}>
        <ul className="qv-list qv-breakdown">
          {breakdownRows(c).map(([name, value]) => <li key={name}><b>{value}</b><span>{name}</span><i>{NO_BREAKDOWN}</i></li>)}
        </ul>
      </Expander>
    </div>
  );
}
