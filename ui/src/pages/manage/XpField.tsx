import { useEffect, useState } from "react";
import { MgIcon } from "./mgIcons";

const XP_MAX = 9_999_999;
const XP_STEP = 100;

type XpFieldProps = { value: number; onChange: (n: number) => void; label?: string; noun?: string; min?: number; max?: number; step?: number };

/** Classic builder's CURRENT EXPERIENCE box: full-width field, value right-aligned, flat + and - glyphs. */
export function XpField({ value, onChange, label = "Current experience", noun = "experience", min = 0, max = XP_MAX, step = XP_STEP }: XpFieldProps) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const clamp = (n: number) => Math.min(max, Math.max(min, n));
  const commit = (raw: string) => {
    const n = Number(raw.replace(/[^\d]/g, ""));
    const next = Number.isFinite(n) ? clamp(n) : value;
    setDraft(String(next));
    if (next !== value) onChange(next);
  };
  return (
    <span className="mg-xp">
      <input type="text" inputMode="numeric" role="spinbutton" aria-label={label} aria-valuenow={value} aria-valuemin={min} aria-valuemax={max}
        value={draft} onChange={(e) => setDraft(e.target.value)} onBlur={() => commit(draft)}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit(draft);
          if (e.key === "ArrowUp") { e.preventDefault(); onChange(clamp(value + step)); }
          if (e.key === "ArrowDown") { e.preventDefault(); onChange(clamp(value - step)); }
        }} />
      <button type="button" className="mg-xp-btn" aria-label={`Increase ${noun}`} onClick={() => onChange(clamp(value + step))}><MgIcon name="plus" /></button>
      <button type="button" className="mg-xp-btn" aria-label={`Decrease ${noun}`} onClick={() => onChange(clamp(value - step))}><MgIcon name="minus" /></button>
    </span>
  );
}
