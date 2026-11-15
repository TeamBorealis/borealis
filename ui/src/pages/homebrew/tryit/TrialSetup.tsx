import { useState } from "react";
import type { HomebrewKindDescriptor, HomebrewTrialView, Ruleset } from "../../../api/contract";
import { ABILITY_KEYS } from "../../../api/contract";
import { ComboBox, NumericUpDown } from "../../../components";
import { type TrialSlot, draftItem, placementText, takenAtLevel } from "./placement";

const EDITIONS = [{ value: "dnd2024", label: "2024" }, { value: "dnd2014", label: "2014" }];
const MAX_LEVEL = 20;

/** Two tiles swap their scores (the New character flyout's standard array tiles). */
export function swapScores(scores: readonly number[], a: number, b: number): number[] {
  return scores.map((v, i) => (i === a ? scores[b]! : i === b ? scores[a]! : v));
}

export interface TrialSetupProps {
  kind: HomebrewKindDescriptor;
  draftName: string;
  slot: TrialSlot;
  view: HomebrewTrialView;
  /** The level on screen (the one asked for while the answer is on its way). */
  level: number;
  asSecondClass: boolean;
  onLevel: (level: number) => void;
  onEdition: (edition: Ruleset) => void;
  onScores: (scores: number[]) => void;
  onSecondClass: (on: boolean) => void;
}

/**
 * SETUP (spec 9, 13.9): the draft in its slot, edition, LEVEL (slider + NumericUpDown) and the ability scores as the New
 * character flyout's tiles (press one, then another, to swap). Class, subclass, species and background are the test
 * character's own choices, changed with the Build pickers under CHOICES.
 */
export function TrialSetup({ kind, draftName, slot, view, level, asSecondClass, onLevel, onEdition, onScores, onSecondClass }: TrialSetupProps) {
  const [grab, setGrab] = useState<number | null>(null);
  const s = view.snapshot;
  const scores = ABILITY_KEYS.map((k) => s.abilities.base[k]);
  const takenAt = slot === "feat" ? takenAtLevel(s, draftName) : null;
  const tap = (i: number) => {
    if (grab === null) { setGrab(i); return; }
    setGrab(null);
    if (grab !== i) onScores(swapScores(scores, grab, i));
  };
  return (
    <div className="hb-try-setup">
      <div className="v4-opt hb-try-draft">
        <span className="v4-fl">Your {kind.label.toLowerCase()}</span>
        <strong className="hb-try-draft-name">{draftName}</strong>
        <p>{placementText(slot, asSecondClass, slot === "item" ? draftItem(s, draftName) : undefined)}{takenAt !== null ? ` Taken at level ${takenAt}.` : ""}</p>
        {slot === "class" && (
          <label className="hb-try-check">
            <input type="checkbox" checked={asSecondClass} onChange={(e) => onSecondClass(e.target.checked)} />
            Try as a second class (Fighter 1, then this class)
          </label>
        )}
      </div>
      <div className="v4-row">
        <div>
          <label className="v4-fl" htmlFor="hb-try-edition">Edition</label>
          <ComboBox light id="hb-try-edition" options={EDITIONS} value={s.ruleset} onChange={(v) => onEdition(v as Ruleset)} />
        </div>
        <div>
          <span className="v4-fl" id="hb-try-level-label">Level</span>
          <NumericUpDown label="Level" value={level} min={1} max={MAX_LEVEL} onChange={onLevel} />
        </div>
      </div>
      <input className="aur-slider hb-try-slider" type="range" min={1} max={MAX_LEVEL} step={1} value={level} aria-labelledby="hb-try-level-label"
        onChange={(e) => onLevel(Number(e.target.value))} />
      <span className="v4-fl">Ability scores</span>
      <p className="sub hint">The standard array, set by the class's main abilities. Press one score, then another, to swap them.</p>
      <div className="v4-abil" role="group" aria-label="Ability scores">
        {ABILITY_KEYS.map((k, i) => (
          <button key={k} type="button" className="v4-tile-abil" aria-pressed={grab === i}
            aria-label={`${k.toUpperCase()} ${scores[i]}: press, then press another score to swap`} onClick={() => tap(i)}>
            <b>{k}</b><span>{scores[i]}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
