import type { CharacterSnapshot } from "../../api/contract";
import { ABILITY_KEYS } from "../../api/contract";
import { Flyout } from "../controls/Flyout";
import { Expander } from "../controls/Expander";
import { Portrait } from "../Portrait";
import { MetroProgressBar } from "../controls/Controls";
import { ABILITY_LABEL } from "../../lib/format";
import { useSettings } from "../../state/SettingsContext";

const signed = (n: number) => (n >= 0 ? `+${n}` : `${n}`);
const chosen = (s: CharacterSnapshot, tab: string) => s.slots.find((x) => x.tab === tab && x.chosen)?.chosen?.name ?? "";

/** "LEVEL 1 MOUNTAIN DWARF FIGHTER": level, species and every class. */
export const buildLine = (s: CharacterSnapshot): string =>
  ["Level", s.level.total, chosen(s, "race"), s.level.classes.map((c) => c.name).filter(Boolean).join(" / ")].filter((x) => x !== "").join(" ");

/**
 * The classic builder's character quick view: the name in gold, XP bar, portrait, build line, background / gender / alignment, the five headline stats,
 * the six scores, a two-column SKILLS checklist and collapsed SAVING THROWS / CONDITIONS / ADDITIONAL STATISTICS expanders. Opened from the profile name.
 */
export function QuickViewFlyout({ snapshot, onClose }: { snapshot: CharacterSnapshot; onClose: () => void }) {
  const { settings } = useSettings();
  const { level, stats, abilities, details, play } = snapshot;
  const span = level.nextLevelXp !== null ? level.nextLevelXp - level.currentLevelXp : 1;
  const sub = [chosen(snapshot, "background"), details.gender, details.alignment].filter(Boolean);
  const half = Math.ceil(stats.skills.length / 2);
  const skills = [stats.skills.slice(0, half), stats.skills.slice(half)];
  const headline: [string, string][] = [
    ["Proficiency", signed(stats.proficiencyBonus)], ["Initiative", signed(stats.initiative)], ["AC", String(stats.armorClass)],
    ["Speed", `${stats.speed}ft`], ["HP", String(stats.hpMax)],
  ];
  return (
    <Flyout title={<span className="qv-name">{snapshot.name}</span>} onClose={onClose}>
      <div className="qv">
        <div className="qv-xp">
          <span>{level.currentLevelXp} XP</span>
          <MetroProgressBar label="Progress to next level" value={level.nextLevelXp !== null ? level.xp - level.currentLevelXp : 1} max={span} />
          <span>{level.nextLevelXp !== null ? `${level.nextLevelXp} XP` : "MAX"}</span>
        </div>
        <Portrait src={snapshot.portrait} name={snapshot.name} className="qv-portrait" />
        <h3 className="qv-build">{buildLine(snapshot)}</h3>
        {sub.length > 0 && <p className="qv-sub">{sub.join(" • ")}</p>}
        <p className="qv-stats">{headline.map(([k, v]) => <span key={k}>{k}: <b>{v}</b></span>)}</p>
        <div className="qv-abil">
          {ABILITY_KEYS.map((k) => <div key={k}><b>{k}</b><span>{abilities.final[k]} ({signed(abilities.modifier[k])})</span></div>)}
        </div>
        <Expander title="Skills" defaultOpen={settings.skillsPanelExpanded}>
          <div className="qv-skills">
            {skills.map((col, i) => (
              <ul key={i}>
                {col.map((s) => (
                  <li key={s.key}>
                    <i className={s.proficiency === "none" ? "box" : "box on"} aria-label={s.proficiency === "none" ? "not proficient" : s.proficiency} />
                    <em>{signed(s.bonus)}</em><span>{s.name} <small>({s.ability.toUpperCase()})</small></span>
                  </li>
                ))}
              </ul>
            ))}
          </div>
          <p className="qv-passive"><i className="box" aria-hidden="true" /><em>{signed(stats.passivePerception)}</em><span>Passive Perception</span></p>
        </Expander>
        <Expander title="Saving throws" defaultOpen={settings.savingThrowsExpanded}>
          <ul className="qv-list">
            {ABILITY_KEYS.map((k) => <li key={k}><i className={abilities.save[k].proficient ? "box on" : "box"} aria-hidden="true" /><em>{signed(abilities.save[k].bonus)}</em><span>{ABILITY_LABEL[k]}</span></li>)}
          </ul>
        </Expander>
        <Expander title="Conditions" defaultOpen={false}>
          <p className="qv-note">{play.exhaustion.level > 0 ? `Exhaustion level ${play.exhaustion.level}` : "No active conditions."}</p>
        </Expander>
        <Expander title="Additional statistics" defaultOpen={false}>
          <ul className="qv-list">
            <li><span>Passive Perception: {stats.passivePerception}</span></li>
            {stats.languages.length > 0 && <li><span>Languages: {stats.languages.join(", ")}</span></li>}
            {play.senses.map((s, i) => <li key={i}><span>{s.name}{s.range > 0 ? ` ${s.range} ft` : ""}</span></li>)}
          </ul>
        </Expander>
      </div>
    </Flyout>
  );
}
