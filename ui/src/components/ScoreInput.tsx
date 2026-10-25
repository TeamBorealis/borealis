import { useRef } from "react";
import { IconButton } from "./Button";
import { useHoldRepeat } from "./useHoldRepeat";

/** Stepper with large -/+ targets and press-and-hold repeat. Does not clamp: callers decide limits. */
export function ScoreInput({ label, value, min, max, onChange, canIncrease = true, canDecrease = true, format }: {
  label: string; value: number; min: number; max: number; onChange: (v: number) => void; canIncrease?: boolean; canDecrease?: boolean; format?: (n: number) => string;
}) {
  const atMin = value <= min || !canDecrease;
  const atMax = value >= max || !canIncrease;
  const latest = useRef({ value, onChange, atMin, atMax });
  latest.current = { value, onChange, atMin, atMax };
  const hold = useHoldRepeat();
  const step = (d: number) => () => {
    const cur = latest.current;
    if (d < 0 ? cur.atMin : cur.atMax) return;
    // A held press repeats before React re-renders: count on from the value just sent.
    latest.current = { ...cur, value: cur.value + d, atMin: cur.value + d <= min || !canDecrease, atMax: cur.value + d >= max || !canIncrease };
    cur.onChange(cur.value + d);
  };
  return (
    <div className="score-input" role="group" aria-label={label}>
      <IconButton icon="minus" label={`Decrease ${label}`} variant="default" disabled={atMin} {...hold(step(-1))} />
      <output aria-live="polite" aria-label={`${label} value`}>{format ? format(value) : value}</output>
      <IconButton icon="plus" label={`Increase ${label}`} variant="default" disabled={atMax} {...hold(step(1))} />
    </div>
  );
}
