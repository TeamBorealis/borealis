import { type FocusEvent, type KeyboardEvent, type MouseEvent, useEffect, useId, useRef, useState } from "react";

export interface EditableComboBoxProps {
  /** Id of the text input (a `<label htmlFor>` names it). */
  id?: string;
  value: string;
  /** Suggestions; any other text is accepted too. */
  options: readonly string[];
  /** Every edit: each keystroke and each picked option. */
  onChange: (value: string) => void;
  /** The value is final: focus left, Enter, or an option was picked. */
  onCommit?: (value: string) => void;
  /** Field name for the arrow button ("Show <label> options") and the list. */
  label: string;
  /** White field used inside flyouts. */
  light?: boolean;
  className?: string;
  placeholder?: string;
  /** Name of the text box when no `<label htmlFor={id}>` names it. */
  "aria-label"?: string;
}

const NONE = -1;

/** WPF `ComboBox IsEditable="True"`: a text box that takes typed text, with a drop-down of suggestions
 *  (WAI-ARIA combobox with a list popup). Arrow Down / Alt+Down open the list, the arrows move, Enter picks,
 *  Escape closes; typed text is kept as written. */
export function EditableComboBox({ id, value, options, onChange, onCommit, label, light, className = "", placeholder, "aria-label": ariaLabel }: EditableComboBoxProps) {
  const auto = useId();
  const inputId = id ?? `${auto}-input`;
  const listId = `${inputId}-list`;
  const optionId = (i: number) => `${inputId}-opt-${i}`;
  const wrap = useRef<HTMLSpanElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(NONE);

  const show = () => { setActive(options.indexOf(value)); setOpen(true); };
  const hide = () => { setOpen(false); setActive(NONE); };
  const pick = (i: number) => {
    const next = options[i];
    if (next === undefined) return;
    onChange(next);
    onCommit?.(next);
    hide();
  };
  /* A press anywhere else closes the list (blur alone misses a list opened without focus in the text box). */
  useEffect(() => {
    if (!open) return undefined;
    const outside = (e: PointerEvent) => { if (!wrap.current?.contains(e.target as Node | null)) hide(); };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);
  const step = (by: number) => {
    if (!options.length) return;
    setActive((a) => (a === NONE ? (by > 0 ? 0 : options.length - 1) : Math.min(options.length - 1, Math.max(0, a + by))));
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!open || e.altKey) { if (!open) show(); return; }
      step(e.key === "ArrowDown" ? 1 : -1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (open && active !== NONE) pick(active);
      else { hide(); onCommit?.(value); }
    } else if (e.key === "Escape" && open) {
      e.preventDefault();
      e.stopPropagation();
      hide();
    }
  };
  const onBlur = (e: FocusEvent<HTMLInputElement>) => {
    if (wrap.current?.contains(e.relatedTarget as Node | null)) return;
    hide();
    onCommit?.(value);
  };
  /* Keep focus in the text box while the arrow or an option is pressed. */
  const keepFocus = (e: MouseEvent) => e.preventDefault();
  const toggle = () => {
    if (open) hide(); else show();
    input.current?.focus();
  };

  return (
    <span ref={wrap} className={`aur-combo aur-combo-edit ${light ? "light" : ""} ${open ? "open" : ""} ${className}`}>
      <input ref={input} id={inputId} type="text" role="combobox" autoComplete="off" value={value} placeholder={placeholder}
        aria-label={ariaLabel} aria-autocomplete="none" aria-expanded={open} aria-controls={listId}
        aria-activedescendant={open && active !== NONE ? optionId(active) : undefined}
        onChange={(e) => onChange(e.target.value)} onKeyDown={onKeyDown} onBlur={onBlur} />
      <button type="button" className="aur-combo-toggle" tabIndex={-1} aria-label={`Show ${label} options`}
        onMouseDown={keepFocus} onClick={toggle}>
        <svg className="aur-combo-arrow" width="7" height="4" viewBox="0 0 7 4" aria-hidden="true"><path d="M0 0h7L3.5 4z" /></svg>
      </button>
      {open && (
        <ul id={listId} role="listbox" className="aur-combo-list" aria-label={label}>
          {options.map((o, i) => (
            <li key={o} id={optionId(i)} role="option" tabIndex={-1} aria-selected={o === value}
              className={i === active ? "active" : undefined} onMouseDown={keepFocus} onMouseEnter={() => setActive(i)} onClick={() => pick(i)}>
              {o}
            </li>
          ))}
        </ul>
      )}
    </span>
  );
}

