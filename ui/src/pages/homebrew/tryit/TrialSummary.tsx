import type { ReactNode } from "react";
import type { CharacterSnapshot } from "../../../api/contract";
import { ABILITY_KEYS } from "../../../api/contract";

const ORDINALS = ["1st", "2nd", "3rd", "4th", "5th", "6th", "7th", "8th", "9th"];
const signed = (n: number): string => (n >= 0 ? `+${n}` : String(n));
const levelWord = (n: number): string => (n === 0 ? "cantrip" : ORDINALS[n - 1] ?? `${n}th`);

/** Is a path (or a list entry `<list>:<name>`) among the draft's changes (`HomebrewTrialView.changed`)? */
export type ChangedFn = (path: string) => boolean;

export function changedSet(changed: readonly string[]): ChangedFn {
  const set = new Set(changed.map((c) => c.toLowerCase()));
  return (path) => set.has(path.toLowerCase());
}

/** A value the draft changed: gold, with a mark and words for screen readers (spec 17: never colour alone). */
function Val({ on, children }: { on: boolean; children: ReactNode }) {
  if (!on) return <>{children}</>;
  return <span className="hb-try-changed">{children}<span className="hb-try-mark" aria-hidden="true">*</span><span className="sr-only"> (changed by this element)</span></span>;
}

function Panel({ title, children }: { title: string; children: ReactNode }) {
  // H-ajc: h3 under the flyout's h2 (axe `heading-order`); the look is the class's
  return <section className="hb-try-panel" aria-label={title}><h3 className="v4-fl">{title}</h3>{children}</section>;
}

/** One line per entry of a list, each marked when the draft added or changed it. */
function Lines({ list, entries, changed, empty }: { list: string; entries: { key: string; name: string; text?: string }[]; changed: ChangedFn; empty: string }) {
  if (entries.length === 0) return <p className="hb-try-none">{empty}</p>;
  return (
    <ul className="hb-try-lines">
      {entries.map((e) => (
        <li key={e.key}><Val on={changed(`${list}:${e.name}`)}>{e.name}</Val>{e.text && <span className="hb-try-dim"> {e.text}</span>}</li>
      ))}
    </ul>
  );
}

function StatTiles({ s, changed }: { s: CharacterSnapshot; changed: ChangedFn }) {
  const tiles: [string, string, string][] = [
    ["Hit points", String(s.stats.hpMax), "stats.hpMax"],
    ["Armor class", String(s.stats.armorClass), "stats.armorClass"],
    ["Speed", `${s.stats.speed} ft.`, "stats.speed"],
    ["Initiative", signed(s.stats.initiative), "stats.initiative"],
    ["Proficiency", signed(s.stats.proficiencyBonus), "stats.proficiencyBonus"],
    ["Passive perc.", String(s.stats.passivePerception), "stats.passivePerception"],
  ];
  return (
    <div className="hb-try-tiles" role="list" aria-label="Combat numbers">
      {tiles.map(([label, value, path]) => (
        <div key={path} role="listitem" className="hb-try-tile"><b>{label}</b><span><Val on={changed(path)}>{value}</Val></span></div>
      ))}
    </div>
  );
}

function Abilities({ s, changed }: { s: CharacterSnapshot; changed: ChangedFn }) {
  return (
    <div className="v4-abil hb-try-abil" role="list" aria-label="Abilities">
      {ABILITY_KEYS.map((k) => {
        const save = s.abilities.save[k];
        return (
          <div key={k} role="listitem" className="v4-tile-abil hb-try-abil-tile" aria-label={`${k.toUpperCase()} ${s.abilities.final[k]}, save ${signed(save.bonus)}${save.proficient ? ", proficient" : ""}`}>
            <b>{k}</b>
            <span><Val on={changed(`abilities.final.${k}`)}>{s.abilities.final[k]}</Val></span>
            <i><Val on={changed(`abilities.save.${k}`)}>{signed(save.bonus)}</Val>{save.proficient ? " P" : ""}</i>
          </div>
        );
      })}
    </div>
  );
}

const spellsOf = (s: CharacterSnapshot) =>
  s.spellcasting.classes.flatMap((c) => c.spells.map((sp) => ({ key: `${c.classSlotId}-${sp.id}`, name: sp.name, text: `(${levelWord(sp.level)}, ${c.className})` })));

/**
 * The trial character's read-only summary (spec 9 "Output", 13.9: summary panels, AuroraPanel dashed border), built from
 * the same snapshot the app uses. Values the draft changed are marked.
 */
export function TrialSummary({ snapshot: s, changed }: { snapshot: CharacterSnapshot; changed: ChangedFn }) {
  const skills = s.stats.skills.filter((k) => k.proficiency !== "none");
  const slots = s.spellcasting.slots.filter((r) => r.total > 0);
  const features = [...s.play.features].sort((a, b) => a.level - b.level);
  const proficiencies = s.stats.proficiencies.flatMap((g) => g.values.map((v) => ({ key: `${g.label}-${v}`, name: v, text: `(${g.label})` })));
  return (
    <div className="hb-try-summary">
      <StatTiles s={s} changed={changed} />
      <Abilities s={s} changed={changed} />
      <Panel title="Skills">
        <Lines list="stats.skills" changed={changed} empty="No skill proficiencies."
          entries={skills.map((k) => ({ key: k.key, name: k.name, text: `${signed(k.bonus)}${k.proficiency === "expertise" ? " (expertise)" : ""}` }))} />
      </Panel>
      <Panel title="Proficiencies and languages">
        <Lines list="stats.proficiencies" changed={changed} empty="No proficiencies." entries={proficiencies} />
        <Lines list="stats.languages" changed={changed} empty="No languages." entries={s.stats.languages.map((l) => ({ key: l, name: l }))} />
      </Panel>
      <Panel title="Features">
        <Lines list="play.features" changed={changed} empty="No features."
          entries={features.map((f) => ({ key: `${f.id}-${f.level}`, name: f.name, text: `(level ${f.level}${f.grantedBy ? `, ${f.grantedBy}` : ""})` }))} />
      </Panel>
      <Panel title="Spell slots">
        {slots.length === 0 ? <p className="hb-try-none">No spell slots.</p> : (
          <p className="hb-try-slots">{slots.map((r) => (
            <span key={r.level}><Val on={changed(`spellcasting.slots.${r.level}`)}>{ORDINALS[r.level - 1]} {r.total}</Val></span>
          ))}</p>
        )}
      </Panel>
      <Panel title="Spells">
        <Lines list="spellcasting.spells" changed={changed} empty="No spells." entries={spellsOf(s)} />
      </Panel>
      <Panel title="Attacks">
        <Lines list="attacks" changed={changed} empty="No attacks."
          entries={s.attacks.map((a) => ({ key: a.id || a.name, name: a.name, text: [a.attackBonus, a.damage].filter(Boolean).join(", ") }))} />
      </Panel>
      <Panel title="Resources">
        <Lines list="play.resources" changed={changed} empty="No resources."
          entries={s.play.resources.map((r) => ({ key: r.id, name: r.name, text: `(${r.max})` }))} />
      </Panel>
      {s.companion && (
        <Panel title="Companion">
          <Lines list="companions" changed={changed} empty="" entries={[{ key: s.companion.id, name: s.companion.name || s.companion.creature?.name || "Companion", text: `AC ${s.companion.armorClassNote ?? s.companion.armorClass}, HP ${s.companion.hpNote ?? s.companion.hp}` }]} />
        </Panel>
      )}
    </div>
  );
}
