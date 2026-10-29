import { type KeyboardEvent, useRef, useState } from "react";

/** Most pips a row shows; a larger maximum tracks with - and + (GitHub #2). */
export const PIP_LIMIT = 12;

export interface PipButtonsProps {
  /** Accessible name of the row's pip group ("Level 1 slots"). */
  label: string;
  max: number;
  used: number;
  /** Filled pips are the used ones, from the left (a tracker that counts up); else filled = still available. */
  countsUp?: boolean;
  /** Accessible names: what a press does, with the row's state ("Use a level 1 slot, 3 of 3 left"). */
  spendName: string;
  restoreName: string;
  onSpend: () => void;
  onRestore: () => void;
}

const NEXT: Record<string, number> = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 };

/**
 * One button per use, as the Resources card's pips (filled = available; for a tracker that counts up, filled = gained).
 * Each pip is labelled by what a press does: an available pip spends one, a used pip gives one back. The row is one
 * tab stop; the arrow keys, Home and End move between its pips.
 */
export function PipButtons({ label, max, used, countsUp, spendName, restoreName, onSpend, onRestore }: PipButtonsProps) {
  const [focus, setFocus] = useState(0);
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const current = Math.min(focus, max - 1);
  const filled = countsUp ? used : max - used;
  const move = (e: KeyboardEvent, i: number) => {
    const step = NEXT[e.key];
    const to = e.key === "Home" ? 0 : e.key === "End" ? max - 1 : step !== undefined ? i + step : null;
    if (to === null || to < 0 || to >= max) return;
    e.preventDefault();
    setFocus(to);
    refs.current[to]?.focus();
  };
  return (
    <span className="v5-pips st-pips" role="group" aria-label={label}>
      {Array.from({ length: max }, (_, i) => {
        const isFilled = i < filled;
        // a press on an available pip (counting down) or an empty pip (counting up) marks one more used
        const spends = countsUp ? !isFilled : isFilled;
        return (
          <button key={i} ref={(el) => { refs.current[i] = el; }} type="button" className="st-pip" tabIndex={i === current ? 0 : -1}
            aria-label={spends ? spendName : restoreName} onFocus={() => setFocus(i)} onKeyDown={(e) => move(e, i)}
            onClick={spends ? onSpend : onRestore}>
            <span className={isFilled ? "v5-pip full" : "v5-pip"} aria-hidden="true" />
          </button>
        );
      })}
    </span>
  );
}

/** The row's count: "left/max" (or "used/max" when it counts up), announced as "<name>: <n> of <max> left". */
export function UsesLeft({ name, max, used, countsUp }: { name: string; max: number; used: number; countsUp?: boolean }) {
  const shown = countsUp ? used : max - used;
  return <span className="v5-count" role="status" aria-label={`${name}: ${shown} of ${max}${countsUp ? "" : " left"}`}>{shown}/{max}</span>;
}
