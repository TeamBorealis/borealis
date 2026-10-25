import { type MouseEvent, type ReactNode, useId } from "react";
import { CheckBox, TextBox } from "../../components";
import { Icon } from "../../components/Icon";
import { usePhone } from "../../lib/phone";
import { PhoneCase } from "../../lib/phoneCase";

/** Section title (SectionPrimary): 13 px SemiBold; `first` drops the 25 px top margin used by later sections. */
export function SectionTitle({ children, first }: { children: ReactNode; first?: boolean }) {
  return <h3 className={`aur-set-title13 ${first ? "first" : ""}`}>{typeof children === "string" ? <PhoneCase>{children}</PhoneCase> : children}</h3>;
}

/** Section description (SectionSecondary): 11 px secondary text, wraps, 10 px below. */
export function SectionDesc({ children }: { children: ReactNode }) {
  return <p className="aur-set-desc">{children}</p>;
}

/** Field label (FieldHeader): 10 px SemiBold uppercase secondary text. */
export function FieldLabel({ children, htmlFor }: { children: ReactNode; htmlFor?: string }) {
  return <label className="aur-set-field" htmlFor={htmlFor}>{typeof children === "string" ? <PhoneCase>{children}</PhoneCase> : children}</label>;
}

/** Small help under a field (10 px). */
export function Help({ children }: { children: ReactNode }) {
  return <p className="aur-set-help">{children}</p>;
}

/** DescriptionCheckBox: header text beside the box, wrapped description indented 24 px under it. `sub` = 20 px sub-option. */
export function DescCheck({ title, description, checked, onChange, sub, spaced, tooltip, disabled }: {
  title: string; description?: ReactNode; checked: boolean; onChange: (v: boolean) => void; sub?: boolean; spaced?: boolean; tooltip?: string; disabled?: boolean;
}) {
  const id = useId();
  // Phones (B-48 O6): the whole row toggles the checkbox; a tap on the box itself, or on a control in the text, keeps its own action.
  const phone = usePhone();
  const toggleRow = (e: MouseEvent<HTMLDivElement>) => {
    if (disabled || (e.target as HTMLElement).closest("input, button, a, select, textarea")) return;
    onChange(!checked);
  };
  return (
    <div className={`aur-set-check ${sub ? "sub" : ""} ${spaced ? "spaced" : ""}`} title={tooltip} onClick={phone ? toggleRow : undefined}>
      <CheckBox disabled={disabled} aria-labelledby={`${id}-t`} aria-describedby={description ? `${id}-d` : undefined} checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span className="aur-set-check-head" id={`${id}-t`}>{title}</span>
      {description && <span className="aur-set-check-desc" id={`${id}-d`}>{description}</span>}
    </div>
  );
}

/** Read-only path box with the folder button embedded at the right (MetroButtonTextBox). */
export function PathBox({ value, label, placeholder, onBrowse }: { value: string; label: string; placeholder?: string; onBrowse: () => void }) {
  return (
    <span className="aur-set-path">
      <input type="text" readOnly aria-label={label} value={value} placeholder={placeholder} title={value || undefined} />
      <button type="button" aria-label={`Browse for ${label.toLowerCase()}`} onClick={onBrowse}><Icon name="folder-search" /></button>
    </span>
  );
}

export function NumberField({ label, value, min, max, onChange }: { label: string; value: number; min: number; max: number; onChange: (n: number) => void }) {
  const id = useId();
  return (
    <>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <TextBox id={id} type="number" min={min} max={max} value={value}
        onChange={(e) => { const n = Number(e.target.value); if (Number.isFinite(n) && n >= min && n <= max) onChange(n); }} />
    </>
  );
}
