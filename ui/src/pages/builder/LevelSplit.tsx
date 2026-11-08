import { type ReactNode, useState } from "react";
import type { CharacterSnapshot, ClassLevelInfo } from "../../api/contract";
import { Button, Modal, ScoreInput, useConfirm } from "../../components";
import { haptic } from "../../lib/haptics";
import { useCharacter } from "../../state/CharacterContext";
import { useSettings } from "../../state/SettingsContext";

/** B2a (#34): dividing the character's levels between its main class and a multiclass, the total level kept. */
export interface LevelSplit {
  /** The main class (it gives the levels and takes them back). */
  mainName: string;
  mainLevel: number;
  /** The multiclass, new or existing. */
  className: string;
  /** Its levels now (0 for a new multiclass). */
  current: number;
}

/** Most levels the multiclass can hold: the main class keeps one. */
export const maxLevels = (s: LevelSplit): number => s.current + s.mainLevel - 1;

const levelRange = (name: string, from: number, to: number): string =>
  from === to ? `${name} level ${from}` : `${name} levels ${from} to ${to}`;

/** The classes after the move, as the class cards show them: "Fighter 5 / Wizard 4". */
export function splitResult(s: LevelSplit, next: number): string {
  const main = s.mainLevel - (next - s.current);
  return next > 0 ? `${s.mainName} ${main} / ${s.className} ${next}` : `${s.mainName} ${main}`;
}

/** What a move drops: the choices made at the levels that change class (null when nothing moves). */
export function droppedNote(s: LevelSplit, next: number): string | null {
  if (next === s.current) return null;
  const moved = next > s.current
    ? levelRange(s.mainName, s.mainLevel - (next - s.current) + 1, s.mainLevel)
    : levelRange(s.className, next + 1, s.current);
  return `Choices made at ${moved} are dropped. The moved levels ask for their own choices on the Build tabs.`;
}

interface LevelSplitDialogProps {
  split: LevelSplit;
  title: string;
  onConfirm: (levels: number) => void;
  onClose: () => void;
}

/** Choose how many levels the multiclass holds; the main class keeps the rest, so the character's level stays. */
export function LevelSplitDialog({ split, title, onConfirm, onClose }: LevelSplitDialogProps) {
  const [next, setNext] = useState(Math.max(1, split.current));
  const total = split.mainLevel + split.current;
  const note = droppedNote(split, next);
  return (
    <Modal title={title} onClose={onClose}
      footer={<>
        <Button onClick={onClose}>Cancel</Button>
        <Button variant="primary" data-autofocus disabled={next === split.current} onClick={() => onConfirm(next)}>Move levels</Button>
      </>}>
      <div className="lvl-split">
        <p className="lvl-split-text">Your character stays level {total}. {split.className} takes levels from {split.mainName}, which keeps at least one.</p>
        <div className="lvl-split-row">
          <span>{split.className} levels</span>
          <ScoreInput label={`${split.className} levels`} value={next} min={1} max={maxLevels(split)} onChange={setNext} />
        </div>
        <p className="lvl-split-result" aria-live="polite">{splitResult(split, next)}</p>
        {note && <p className="lvl-split-text muted">{note}</p>}
      </div>
    </Modal>
  );
}

/** A class of the Multiclassing expander that levels can move to (its multiclass entry, when the compendium has one). */
export interface SplitRow { name: string; multiId: string | null }

interface Pending { split: LevelSplit; title: string; run: (levels: number) => Promise<unknown> }

interface ClassLevelMoves {
  /** The open move dialog (null when none). */
  dialog: ReactNode;
  /** Move levels between the main class and an existing multiclass. */
  moveLevels: (c: ClassLevelInfo) => void;
  /** Start a multiclass of `row`'s class from the main class's levels. `onEmpty` runs when it was added without a class (pick it on Build > Class). */
  splitInto: (row: SplitRow, onEmpty: () => void) => void;
  /** Give a multiclass's levels back to the main class, after the Remove Level confirmation when that setting is on. */
  removeMulticlass: (c: ClassLevelInfo) => Promise<void>;
}

const classLabel = (c: ClassLevelInfo): string => c.name ?? "the empty multiclass";

/** B2a: the Level up & multiclassing panel's level moves, all through `setClassLevels` / `splitMulticlass`. */
export function useClassLevelMoves(snapshot: CharacterSnapshot | null): ClassLevelMoves {
  const { mutate } = useCharacter();
  const { settings } = useSettings();
  const confirm = useConfirm();
  const [pending, setPending] = useState<Pending | null>(null);
  const main = snapshot?.level.classes.find((c) => c.isMain);
  const base = { mainName: main?.name ?? "Main class", mainLevel: main?.level ?? 0 };
  const moveLevels = (c: ClassLevelInfo) => setPending({
    title: `${c.name ?? "Multiclass"} levels`,
    split: { ...base, className: c.name ?? "Multiclass", current: c.level },
    run: (levels) => mutate((api, id) => api.setClassLevels({ characterId: id, classSlotId: c.slotId, levels })),
  });
  const splitInto = (row: SplitRow, onEmpty: () => void) => setPending({
    title: `Move levels to ${row.name}`,
    split: { ...base, className: row.name, current: 0 },
    run: async (levels) => {
      const next = await mutate((api, id) => api.splitMulticlass({ characterId: id, levels, ...(row.multiId ? { elementId: row.multiId } : {}) }));
      if (next && !row.multiId) onEmpty();
    },
  });
  const removeMulticlass = async (c: ClassLevelInfo) => {
    if (settings.confirmRemoveLevel) {
      const levels = c.level === 1 ? "level goes" : `${c.level} levels go`;
      const r = await confirm({
        title: "Remove multiclass", confirmLabel: "Remove multiclass", danger: true,
        body: `Remove ${classLabel(c)}? Its ${levels} back to ${base.mainName} (${base.mainName} ${base.mainLevel + c.level}), and the choices made at them are dropped.`,
      });
      if (r !== "confirm") return;
    }
    await mutate((api, id) => api.setClassLevels({ characterId: id, classSlotId: c.slotId, levels: 0 }));
  };
  const dialog = pending && (
    <LevelSplitDialog split={pending.split} title={pending.title} onClose={() => setPending(null)}
      onConfirm={(levels) => { setPending(null); haptic("levelUp"); void pending.run(levels); }} />
  );
  return { dialog, moveLevels, splitInto, removeMulticlass };
}
