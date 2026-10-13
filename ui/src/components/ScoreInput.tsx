import { useEffect, useRef } from "react";
import { IconButton } from "./Button";

/** Stepper with large -/+ targets and press-and-hold repeat. Does not clamp: callers decide limits. */
export function ScoreInput({ label, value, min, max, onChange, canIncrease = true, canDecrease = true, format }: {
  label: string; value: number; min: number; max: number; onChange: (v: number) => void; canIncrease?: boolean; canDecrease?: boolean; format?: (n: number) => string;
}) {
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);
  const latest = useRef({ value, onChange });
  latest.current = { value, onChange };
  const stop = () => { if (timer.current) clearInterval(timer.current); timer.current = null; };
  useEffect(() => stop, []);
  const step = (d: number) => latest.current.onChange(latest.current.value + d);
  const hold = (d: number) => {
    stop();
    timer.current = setInterval(() => step(d), 180);
  };
  const atMin = value <= min || !canDecrease;
  const atMax = value >= max || !canIncrease;
  return (
    <div className="score-input" role="group" aria-label={label}>
      <IconButton icon="minus" label={`Decrease ${label}`} variant="default" disabled={atMin} onClick={() => step(-1)}
        onPointerDown={(e) => e.pointerType === "touch" && hold(-1)} onPointerUp={stop} onPointerLeave={stop} />
      <output aria-live="polite" aria-label={`${label} value`}>{format ? format(value) : value}</output>
      <IconButton icon="plus" label={`Increase ${label}`} variant="default" disabled={atMax} onClick={() => step(1)}
        onPointerDown={(e) => e.pointerType === "touch" && hold(1)} onPointerUp={stop} onPointerLeave={stop} />
    </div>
  );
}
