import { useState } from "react";
import { Button } from "../../components";
import "./hit-points.css";

/** One command of an amount field ("Damage", "Heal", "Use"): what it runs with the typed whole number. */
export interface AmountAction {
  label: string;
  /** Accessible name ("Take damage", "Use Luck"). */
  name: string;
  run: (amount: number) => void;
  disabled?: boolean;
  /** Takes 0 too (setting temporary hit points to 0 ends them). */
  allowZero?: boolean;
}

export interface AmountControlProps {
  /** Accessible name of the number field. */
  label: string;
  /** Largest amount one change takes. */
  max: number;
  actions: AmountAction[];
}

/** The typed amount, or null when it is not a whole number in 0..max. */
export function parseAmount(text: string, max: number): number | null {
  const t = text.trim();
  if (!/^\d+$/.test(t)) return null;
  const n = Number(t);
  return n <= max ? n : null;
}

/**
 * Change by any amount (B-194): a number field and its commands, beside the +1/-1 steps. A command is enabled once the
 * field holds a whole number it takes; the field clears after a command runs.
 */
export function AmountControl({ label, max, actions }: AmountControlProps) {
  const [text, setText] = useState("");
  const amount = parseAmount(text, max);
  const usable = (a: AmountAction) => amount !== null && (amount > 0 || !!a.allowZero) && !a.disabled;
  const press = (a: AmountAction) => {
    if (amount === null || !usable(a)) return;
    a.run(amount);
    setText("");
  };
  return (
    <span className="hp-amount" role="group" aria-label={label}>
      <input className="input hp-amount-input" type="number" inputMode="numeric" min={0} max={max} step={1} placeholder="0"
        aria-label={label} value={text} onChange={(e) => setText(e.target.value)} />
      {actions.map((a) => (
        <Button key={a.label} small aria-label={a.name} disabled={!usable(a)} onClick={() => press(a)}>{a.label}</Button>
      ))}
    </span>
  );
}
