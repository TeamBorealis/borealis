/** Most pips a row shows; a larger maximum tracks with - and + (GitHub #2). */
export const PIP_LIMIT = 12;

export interface PipButtonsProps {
  max: number;
  used: number;
  /** Accessible name of pip `n` (1-based). */
  pipName: (n: number, max: number, used: boolean) => string;
  /** The new used count. */
  onSet: (used: number) => void;
  disabled?: boolean;
}

/**
 * One button per use, as the Resources card's pips: filled = still available, empty = used (the used ones are last).
 * A press on an available pip marks one more used; on a used pip it gives one back.
 */
export function PipButtons({ max, used, pipName, onSet, disabled }: PipButtonsProps) {
  const left = max - used;
  return (
    <span className="v5-pips st-pips">
      {Array.from({ length: max }, (_, i) => {
        const isUsed = i >= left;
        return (
          <button key={i} type="button" className="st-pip" aria-pressed={isUsed} aria-label={pipName(i + 1, max, isUsed)} disabled={disabled}
            onClick={() => onSet(isUsed ? used - 1 : used + 1)}>
            <span className={isUsed ? "v5-pip" : "v5-pip full"} aria-hidden="true" />
          </button>
        );
      })}
    </span>
  );
}

/** The "left/max" count of a row, announced as "<name>: <left> of <max> left". */
export function UsesLeft({ name, max, used }: { name: string; max: number; used: number }) {
  const left = max - used;
  return <span className="v5-count" role="status" aria-label={`${name}: ${left} of ${max} left`}>{left}/{max}</span>;
}
