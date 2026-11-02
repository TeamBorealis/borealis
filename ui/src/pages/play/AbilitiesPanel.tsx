import type { AbilityKey, CharacterSnapshot, Proficiency } from "../../api/contract";
import { signed } from "../../components";
import { Expander } from "../../components/controls";
import { Tag } from "../ai/Panes";
import { D20Button } from "./rolls/RollButton";

const ABILITIES: { key: AbilityKey; name: string }[] = [
  { key: "str", name: "Strength" }, { key: "dex", name: "Dexterity" }, { key: "con", name: "Constitution" },
  { key: "int", name: "Intelligence" }, { key: "wis", name: "Wisdom" }, { key: "cha", name: "Charisma" },
];
const ABBR: Record<string, string> = { str: "STR", dex: "DEX", con: "CON", int: "INT", wis: "WIS", cha: "CHA" };
const PROF_TEXT: Record<Proficiency, string> = { none: "not proficient", proficient: "proficient", expertise: "expertise" };

/** Proficiency as the Resources card's pips: empty = none, one filled = proficient, two filled = expertise. */
export function ProfMark({ level }: { level: Proficiency }) {
  const full = level === "expertise" ? 2 : level === "proficient" ? 1 : 0;
  return (
    <span className="pr-prof" title={PROF_TEXT[level]}>
      <span className="v5-pips" aria-hidden="true">
        {Array.from({ length: Math.max(1, full) }, (_, i) => <span key={i} className={i < full ? "v5-pip full" : "v5-pip"} />)}
      </span>
      <span className="sr-only">{PROF_TEXT[level]}</span>
    </span>
  );
}

function AbilityRows({ s }: { s: CharacterSnapshot }) {
  const a = s.abilities;
  return (
    <ul className="v5-list" aria-label="Ability scores">
      {ABILITIES.map(({ key, name }) => {
        const save = a.save[key];
        return (
          <li key={key} className="v5-line pr-line">
            <span className="v5-row static grow"><span className="grow">{name}</span><span className="meta">{a.final[key]} ({signed(a.modifier[key])})</span></span>
            <span className="v5-ctrls pr-rolls">
              <D20Button what={`${name} check`} modifier={a.check[key]} prefix="Check" />
              <ProfMark level={save.proficient ? "proficient" : "none"} />
              <D20Button what={`${name} save`} modifier={save.bonus} prefix="Save" note={save.proficient ? "proficient" : undefined} />
            </span>
          </li>
        );
      })}
    </ul>
  );
}

function SkillRows({ s }: { s: CharacterSnapshot }) {
  return (
    <ul className="v5-list" aria-label="Skills">
      {s.stats.skills.map((k) => (
        <li key={k.key} className="v5-line pr-line">
          <ProfMark level={k.proficiency} />
          <span className="v5-row static grow"><span className="grow">{k.name}</span><span className="meta">{ABBR[k.ability] ?? k.ability}</span></span>
          <span className="v5-ctrls pr-rolls">
            <D20Button what={`${k.name} check`} modifier={k.bonus} note={k.proficiency === "none" ? undefined : PROF_TEXT[k.proficiency]} />
          </span>
        </li>
      ))}
    </ul>
  );
}

/** Abilities sub-tab (GitHub #14): initiative and passive Perception, the six abilities with check and save, the skills. */
export function AbilitiesPanel({ s }: { s: CharacterSnapshot }) {
  const adv = s.play.initiativeAdvantage;
  return (<>
    <Expander title="Initiative">
      <ul className="v5-list" aria-label="Initiative and passive Perception">
        <li className="v5-line pr-line">
          <span className="v5-row static grow"><span className="grow">Initiative</span>{adv && <Tag tone="ok">Advantage</Tag>}</span>
          <span className="v5-ctrls pr-rolls"><D20Button what="Initiative" modifier={s.stats.initiative} defaultMode={adv ? "advantage" : "normal"} /></span>
        </li>
        <li className="v5-line pr-line">
          <span className="v5-row static grow"><span className="grow">Passive Perception</span></span>
          <span className="v5-count" role="status" aria-label={`Passive Perception ${s.stats.passivePerception}`}>{s.stats.passivePerception}</span>
        </li>
      </ul>
    </Expander>
    <Expander title="Ability Scores"><AbilityRows s={s} /></Expander>
    <Expander title="Skills" headerExtra={<span>{s.stats.skills.length}</span>}><SkillRows s={s} /></Expander>
  </>);
}
