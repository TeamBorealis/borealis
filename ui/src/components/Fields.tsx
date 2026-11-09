import { type InputHTMLAttributes, type ReactNode, type TextareaHTMLAttributes, useId } from "react";
import { usePhone } from "../lib/phone";
import { type ComboOption, SelectSheet } from "./controls/SelectSheet";
import { Icon } from "./Icon";

interface FieldShell { label: string; hint?: string; error?: string | null; children: (id: string, describedBy: string | undefined) => ReactNode }

function Shell({ label, hint, error, children }: FieldShell) {
  const id = useId();
  const hid = `${id}-h`;
  const by = hint || error ? hid : undefined;
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      {children(id, by)}
      {error ? <span id={hid} className="error-text">{error}</span> : hint ? <span id={hid} className="hint">{hint}</span> : null}
    </div>
  );
}

type Common = { label: string; hint?: string; error?: string | null };

export function TextField({ label, hint, error, ...rest }: Common & InputHTMLAttributes<HTMLInputElement>) {
  return <Shell label={label} hint={hint} error={error}>{(id, by) => <input id={id} className="input" aria-describedby={by} aria-invalid={error ? true : undefined} {...rest} />}</Shell>;
}

export function TextArea({ label, hint, error, ...rest }: Common & TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <Shell label={label} hint={hint} error={error}>{(id, by) => <textarea id={id} className="textarea" aria-describedby={by} {...rest} />}</Shell>;
}

/** A labelled select: native on desktop (U7), the select sheet on phones (U22, B-138). */
export function SelectField({ label, hint, error, options, value, onChange, disabled }: Common & {
  options: readonly ComboOption[]; value: string; onChange: (value: string) => void; disabled?: boolean;
}) {
  const phone = usePhone();
  return (
    <Shell label={label} hint={hint} error={error}>
      {(id, by) => phone
        ? <SelectSheet id={id} className="select" value={value} options={options} onChange={onChange} disabled={disabled} aria-describedby={by} aria-invalid={!!error} />
        : (
          <select id={id} className="select" aria-describedby={by} value={value} disabled={disabled} onChange={(e) => onChange(e.target.value)}>
            {options.map((o) => <option key={o.value} value={o.value} disabled={o.disabled}>{o.label}</option>)}
          </select>
        )}
    </Shell>
  );
}

export function Check({ label, description, ...rest }: { label: string; description?: string } & Omit<InputHTMLAttributes<HTMLInputElement>, "type">) {
  return (
    <label className="check">
      <input type="checkbox" {...rest} />
      <span className="check-text">
        <span>{label}</span>
        {description && <span className="hint">{description}</span>}
      </span>
    </label>
  );
}

export function Switch({ label, ...rest }: { label: string } & Omit<InputHTMLAttributes<HTMLInputElement>, "type">) {
  return (
    <span className="switch">
      <input type="checkbox" role="switch" aria-label={label} {...rest} />
    </span>
  );
}

export function SearchInput({ value, onChange, label = "Search", placeholder, ...rest }: { value: string; onChange: (v: string) => void; label?: string; placeholder?: string } & Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange">) {
  return (
    <div className="search">
      <Icon name="search" />
      <input type="search" className="input" aria-label={label} placeholder={placeholder ?? label} value={value} onChange={(e) => onChange(e.target.value)} {...rest} />
    </div>
  );
}
