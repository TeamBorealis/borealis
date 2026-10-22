import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { useShell } from "../../components/shell/ShellContext";
import type { ElementSummary } from "../../api/contract";
import { MetroProgressBar, ScoreInput, TextBox, useConfirm } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useSettings } from "../../state/SettingsContext";
import { FA5_LEVEL_DOWN_ALT, FA5_LEVEL_UP_ALT } from "../../components/controls/openIcons";
import { PaneShell } from "./DescriptionPane";
import { MulticlassExpander } from "./MulticlassList";

/** Font Awesome Free 5 level-up-alt / level-down-alt (CC BY 4.0, see components/controls/openIcons.ts). */
function BarArrow({ dir }: { dir: "up" | "down" }) {
  const g = dir === "up" ? FA5_LEVEL_UP_ALT : FA5_LEVEL_DOWN_ALT;
  return <svg className="icon" viewBox={g.viewBox} style={dir === "up" ? { width: 7, height: 11, marginLeft: 5, marginRight: 0 } : { width: 7, height: 11, marginLeft: 2, marginRight: 2 }} aria-hidden="true" focusable="false"><path d={g.d} /></svg>;
}

function XpField({ xp, onCommit }: { xp: number; onCommit: (v: number) => void }) {
  const [v, setV] = useState(String(xp));
  useEffect(() => setV(String(xp)), [xp]);
  return (
    <label className="lvl-xp"><span>Experience points</span>
      <TextBox type="number" inputMode="numeric" min={0} value={v} onChange={(e) => setV(e.target.value)}
        onBlur={() => { const n = Number(v); if (Number.isFinite(n) && n >= 0 && n !== xp) onCommit(n); }}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
    </label>
  );
}

export function LevelsPane() {
  return (
    <PaneShell title="Level up and HP" imageIcon={false} intro="Gain a level in your current class, or start a new one."
      readText="Level up. Gain a level in your current class, or start a new one. Hit points: each level adds a hit die roll or the average, plus your Constitution modifier.">
      <h3 className="desc-h">Leveling up</h3>
      <p className="desc-line">Experience points only prompt you: a level is never added automatically. Press Level up on a class block when you are ready, then make the new choices that appear on the other Build tabs.</p>
      <h3 className="desc-h">Hit points</h3>
      <p className="desc-line">Your first level uses the largest value of the hit die. Later levels use the average, or a roll you enter here when the average setting is off. Constitution adds to every level.</p>
      <h3 className="desc-h">Multiclassing</h3>
      <p className="desc-line">A new class needs the listed ability scores. Each class keeps its own level, hit die and features.</p>
    </PaneShell>
  );
}

/** Short class descriptions for the class cards (compendium class entries, by id). */
function useClassBlurbs(ruleset: string): Record<string, string> {
  const api = useApi();
  const [blurbs, setBlurbs] = useState<Record<string, string>>({});
  useEffect(() => {
    let live = true;
    void api.searchCompendium({ query: { query: "", types: ["class"], ruleset: ruleset as ElementSummary["rulesets"][number], limit: 100, offset: 0 } })
      .then((p) => { if (live) setBlurbs(Object.fromEntries(p.items.map((c) => [c.id, c.shortDescription]))); })
      .catch(() => undefined);
    return () => { live = false; };
  }, [api, ruleset]);
  return blurbs;
}

/** Classic builder "LEVEL UP & MULTICLASSING" content: XP bar, level line with grey remove-level, a card per class and the multiclass expander anchored to the bottom. The flyout has no XP input (experience lives in Manage); the page variant keeps it. */
export function LevelPanel({ embedded = false }: { embedded?: boolean }) {
  const { snapshot, mutate } = useCharacter();
  const { settings } = useSettings();
  const confirm = useConfirm();
  const nav = useNavigate();
  const { setLevelUpOpen } = useShell();
  const blurbs = useClassBlurbs(snapshot?.ruleset ?? "dnd2024");
  if (!snapshot) return null;
  const lv = snapshot.level;
  const removeLevel = async () => {
    const last = lv.classes[lv.classes.length - 1];
    if (settings.confirmRemoveLevel) {
      const r = await confirm({ title: "Remove last level", body: `Remove the most recent level (${last?.name ?? "the empty multiclass slot"})? Its selections will be dropped.`, confirmLabel: "Remove level", danger: true });
      if (r !== "confirm") return;
    }
    await mutate((api, id) => api.levelDown({ characterId: id }));
  };
  const span = lv.nextLevelXp !== null ? lv.nextLevelXp - lv.currentLevelXp : 1;
  return (
    <div className="lvl">
      {!embedded && <h2 className="lvl-title">Level up &amp; multiclassing</h2>}
      <div className="lvl-xpbar">
        <span>{lv.currentLevelXp} XP</span>
        <MetroProgressBar label="Progress to next level" value={lv.nextLevelXp !== null ? lv.xp - lv.currentLevelXp : 1} max={span} />
        <span>{lv.nextLevelXp !== null ? `${lv.nextLevelXp} XP` : "MAX"}</span>
      </div>
      <p className="lvl-status">
        <span>Your character is level {lv.total}</span>
        <i className="lvl-dot" aria-hidden="true" />
        <button type="button" className="lvl-link remove" disabled={!lv.canLevelDown} onClick={() => void removeLevel()}><BarArrow dir="down" /> Remove the last level</button>
      </p>
      {!embedded && <XpField xp={lv.xp} onCommit={(xp) => void mutate((api, id) => api.setXp({ characterId: id, xp }))} />}
      {lv.levelUpAvailable && <p className="lvl-note" role="status"><strong>Level up available.</strong> Your experience has reached the next threshold.</p>}
      {lv.classes.map((c) => (
        <section key={c.slotId} className="lvl-class" aria-label={c.name ? `${c.name} ${c.level}` : `Empty ${c.level}`}>
          <header>
            <h3>{c.name ? `${c.name} (${c.level})` : `EMPTY (${c.level})`}</h3>
            <button type="button" className="lvl-link up" aria-label={`Level up ${c.name ?? "class"}`} disabled={!lv.canLevelUp || (!c.name && !c.isMain)}
              onClick={() => void mutate((api, id) => api.levelUp({ characterId: id, classSlotId: c.slotId }))}>Level up <BarArrow dir="up" /></button>
          </header>
          <p className="lvl-sub">{c.name ? ((c.classId && blurbs[c.classId]) || `d${c.hitDie} hit die${c.subclass ? ` - ${c.subclass.name}` : ""}`) : "Select a multiclass from the Multiclass (Level N) selection picker."}</p>
          {c.name && !snapshot.options.averageHp && (
            <div className="hp-rolls" aria-label={`Hit point rolls (d${c.hitDie})`}>
              {c.hpRolls.map((r, i) => (
                <div key={i} className="lvl-roll">
                  <span className="small muted">Lv {i + 1}</span>
                  {c.isMain && i === 0 ? <strong>{r} (max)</strong> : (
                    <ScoreInput label={`${c.name} level ${i + 1} HP roll`} value={r} min={1} max={c.hitDie}
                      onChange={(v) => void mutate((api, id) => api.setHpRoll({ characterId: id, classSlotId: c.slotId, level: i + 1, value: v }))} />
                  )}
                </div>
              ))}
            </div>
          )}
        </section>
      ))}
      <MulticlassExpander snapshot={snapshot} onAdded={() => { setLevelUpOpen(false); nav("/builder/class"); }} />
    </div>
  );
}
