import { useEffect, useState, type KeyboardEvent } from "react";

const MAX_DIGITS = 2;

/** The typed text as a score inside min..max, or null when nothing usable was typed. */
export function parseScore(text: string, min: number, max: number): number | null {
  const n = Number.parseInt(text, 10);
  if (!Number.isFinite(n)) return null;
  return Math.min(max, Math.max(min, n));
}

export interface BaseScoreFieldProps {
  label: string;
  value: number;
  min: number;
  max: number;
  onCommit: (v: number) => void;
}

/**
 * B-97: with the manual method the base score beside the chevrons is a text field, so a score rolled with real dice
 * can be typed in. Enter or leaving the field applies it (clamped to the allowed range); Escape or an empty entry puts
 * the current score back.
 */
export function BaseScoreField({ label, value, min, max, onCommit }: BaseScoreFieldProps) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const next = parseScore(draft, min, max);
    setDraft(String(next ?? value));
    if (next !== null && next !== value) onCommit(next);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") { e.preventDefault(); commit(); }
    if (e.key === "Escape") { e.preventDefault(); setDraft(String(value)); }
  };
  return (
    <input className="abil-base-input" type="text" inputMode="numeric" autoComplete="off" maxLength={MAX_DIGITS}
      aria-label={`${label} base score`} title="Base score: type a value or use the arrows" value={draft}
      onChange={(e) => setDraft(e.target.value.replace(/\D/g, ""))} onBlur={commit} onKeyDown={onKeyDown}
      onFocus={(e) => e.target.select()} />
  );
}
