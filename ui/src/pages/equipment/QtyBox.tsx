import { useState } from "react";

const MIN_QTY = 1;
const MAX_QTY = 100;
const clamp = (n: number) => Math.min(MAX_QTY, Math.max(MIN_QTY, Math.round(n) || MIN_QTY));

/** Classic builder command tile (ADD / BUY) with its quantity in an inline dark box (no spinner arrows). */
export function QtyCommand({ label, value, onChange, onRun, disabled }: { label: string; value: number; onChange: (n: number) => void; onRun: () => void; disabled?: boolean }) {
  const [text, setText] = useState<string | null>(null);
  const commit = () => { onChange(clamp(Number(text ?? value))); setText(null); };
  return (
    <span className="eq-cmd">
      <button type="button" className="eq-cmd-btn" disabled={disabled} onClick={onRun}>{label}</button>
      <input
        className="eq-qtybox" inputMode="numeric" aria-label={`${label} quantity`} value={text ?? String(value)}
        onChange={(e) => setText(e.target.value.replace(/\D/g, "").slice(0, 3))}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
          else if (e.key === "ArrowUp") { e.preventDefault(); onChange(clamp(value + 1)); setText(null); }
          else if (e.key === "ArrowDown") { e.preventDefault(); onChange(clamp(value - 1)); setText(null); }
        }}
      />
    </span>
  );
}
