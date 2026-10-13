import { Button } from "./Button";

export function DiceRoll({ label, dice, total, onRoll, disabled }: { label: string; dice?: number[]; total?: number; onRoll: () => void; disabled?: boolean }) {
  return (
    <span className="row nowrap" style={{ gap: "var(--sp-2)" }}>
      <Button small icon="dice" onClick={onRoll} disabled={disabled} aria-label={label}>Roll</Button>
      {dice && <span className="muted small" aria-live="polite">[{dice.join(", ")}] = {total}</span>}
    </span>
  );
}
