import { type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes, useEffect, useId, useRef } from "react";
import { Icon } from "../Icon";
import { useHoldRepeat } from "../useHoldRepeat";

/** MahApps Win10-style switch. `onLabel`/`offLabel` render beside the track. */
export function ToggleSwitch({ checked, onChange, label, onLabel = "On", offLabel = "Off", disabled }: {
  checked: boolean; onChange: (v: boolean) => void; label: string; onLabel?: string; offLabel?: string; disabled?: boolean;
}) {
  return (
    <button type="button" role="switch" aria-checked={checked} aria-label={label} disabled={disabled} className="aur-switch" onClick={() => onChange(!checked)}>
      <span className="aur-switch-track"><span className="aur-switch-thumb" /></span>
      <span className="aur-switch-text">{checked ? onLabel : offLabel}</span>
    </button>
  );
}

export interface ComboOption { value: string; label: string; disabled?: boolean }

/** Native select (best on touch) in a Metro frame. `light` = white field used inside flyouts. */
export function ComboBox({ options, light, className = "", ...rest }: { options: ComboOption[]; light?: boolean } & SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <span className={`aur-combo ${light ? "light" : ""} ${className}`}>
      <select {...rest}>
        {options.map((o) => <option key={o.value} value={o.value} disabled={o.disabled}>{o.label}</option>)}
      </select>
      <svg className="aur-combo-arrow" width="7" height="4" viewBox="0 0 7 4" aria-hidden="true"><path d="M0 0h7L3.5 4z" /></svg>
    </span>
  );
}

/** Metro text box with watermark (placeholder) and an optional clear button. */
export function TextBox({ light, invalid, onClear, className = "", ...rest }: { light?: boolean; invalid?: boolean; onClear?: () => void } & InputHTMLAttributes<HTMLInputElement>) {
  const showClear = onClear && String(rest.value ?? "") !== "";
  return (
    <span className={`aur-textbox ${light ? "light" : ""} ${className}`}>
      <input type="text" aria-invalid={invalid || undefined} {...rest} />
      {showClear && <button type="button" className="aur-textbox-clear" aria-label="Clear" onClick={onClear}><Icon name="x" /></button>}
    </span>
  );
}

/** Compact numeric spinner (equipment qty is 21-40 px wide; `compact`). */
export function NumericUpDown({ value, onChange, min = 0, max = 999, step = 1, label, compact }: {
  value: number; onChange: (v: number) => void; min?: number; max?: number; step?: number; label: string; compact?: boolean;
}) {
  const id = useId();
  const clamp = (n: number) => Math.min(max, Math.max(min, n));
  const latest = useRef({ value, onChange });
  latest.current = { value, onChange };
  const hold = useHoldRepeat();
  /** One step from the latest value (a held press repeats before React re-renders, so it counts on from what it sent). */
  const stepBy = (d: number) => () => {
    const next = clamp(latest.current.value + d);
    if (next === latest.current.value) return;
    latest.current = { ...latest.current, value: next };
    latest.current.onChange(next);
  };
  return (
    <span className={`aur-numeric ${compact ? "compact" : ""}`}>
      <input
        id={id} type="text" inputMode="numeric" role="spinbutton" aria-label={label} aria-valuenow={value} aria-valuemin={min} aria-valuemax={max}
        value={value}
        onChange={(e) => { const n = Number(e.target.value.replace(/[^\d-]/g, "")); if (Number.isFinite(n)) onChange(clamp(n)); }}
        onKeyDown={(e) => {
          if (e.key === "ArrowUp") { e.preventDefault(); onChange(clamp(value + step)); }
          if (e.key === "ArrowDown") { e.preventDefault(); onChange(clamp(value - step)); }
        }}
      />
      <span className="aur-numeric-btns">
        <button type="button" tabIndex={-1} aria-label={`Increase ${label}`} disabled={value >= max} {...hold(stepBy(step))}><Icon name="up" /></button>
        <button type="button" tabIndex={-1} aria-label={`Decrease ${label}`} disabled={value <= min} {...hold(stepBy(-step))}><Icon name="down" /></button>
      </span>
    </span>
  );
}

/** Hover/focus tooltip for icon buttons. The tip text is also the accessible description. */
export function ToolTip({ text, children }: { text: string; children: ReactNode }) {
  const id = useId();
  return (
    <span className="aur-tip" aria-describedby={id}>
      {children}
      <span role="tooltip" id={id} className="aur-tip-text">{text}</span>
    </span>
  );
}

/** 4px-track MetroProgressBar. Omit `value` for indeterminate. */
export function MetroProgressBar({ value, max = 1, label, width }: { value?: number; max?: number; label: string; width?: number | string }) {
  const pct = value === undefined ? undefined : Math.max(0, Math.min(1, value / max)) * 100;
  return (
    <div className={`aur-progress ${pct === undefined ? "indeterminate" : ""}`} role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct === undefined ? undefined : Math.round(pct)} style={width ? { width } : undefined}>
      <span style={pct === undefined ? undefined : { width: `${pct}%` }} />
    </div>
  );
}

/** Classic builder CheckBox: 16 px white square with a dark tick; `mixed` draws the tri-state dash. */
export function CheckBox({ mixed, className = "", ...rest }: { mixed?: boolean } & Omit<InputHTMLAttributes<HTMLInputElement>, "type">) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => { if (ref.current) ref.current.indeterminate = !!mixed; }, [mixed]);
  return <input ref={ref} type="checkbox" className={`v4-cb ${className}`} {...rest} />;
}

/** Tri-state of a group from how many of its members are on. */
export function triState(on: number, total: number): { checked: boolean; mixed: boolean } {
  return { checked: total > 0 && on === total, mixed: on > 0 && on < total };
}
