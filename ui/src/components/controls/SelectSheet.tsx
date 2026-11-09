import { type KeyboardEvent, useId, useRef, useState } from "react";
import { Icon } from "../Icon";
import { Dialog } from "./Dialog";

/** One choice of a select (ComboBox, SelectField and the phone select sheet). */
export interface ComboOption { value: string; label: string; disabled?: boolean }

export interface SelectSheetProps {
  /** Id of the trigger button (a `<label htmlFor>` names it, as it named the select). */
  id?: string;
  value: string;
  options: readonly ComboOption[];
  /** A different option was chosen. Cancelling, or choosing the current option again, calls nothing (as a native select). */
  onChange: (value: string) => void;
  disabled?: boolean;
  /** Extra class on the trigger button. */
  className?: string;
  "aria-label"?: string;
  "aria-describedby"?: string;
  "aria-invalid"?: boolean;
}

/** Sheet title when nothing names the select. */
const UNNAMED = "Choose";
/** Keys that open the sheet from the trigger, besides Enter and Space (the native select's keys). */
const OPEN_KEYS = new Set(["ArrowDown", "ArrowUp"]);

const stop = (e: { stopPropagation: () => void }) => e.stopPropagation();

/** The select's visible name: its aria-label, else its `<label>` text. */
function selectName(button: HTMLButtonElement | null, ariaLabel: string | undefined): string {
  if (ariaLabel) return ariaLabel;
  const text = button?.labels?.[0]?.textContent?.trim();
  return text || UNNAMED;
}

/**
 * U22 (B-138): the phone select. Android's WebView select dialog scrolls when a finger drifts during the tap and selects
 * the wrong row, so on phones (iOS and Android alike) a select is a button that shows the chosen label and opens the app's
 * bottom sheet (Dialog: grabber, swipe down, the scrim, Close, Escape and Android back cancel; O4 B). The sheet lists every
 * option as a 48 dp row with the current one marked; a tap chooses it and closes the sheet. Focus starts on the current
 * row, the arrow keys move between rows, and focus returns to the button when the sheet closes. Desktop keeps the native
 * select (U7): ComboBox and SelectField render this only while `usePhone()`.
 */
export function SelectSheet({ id, value, options, onChange, disabled, className, "aria-label": ariaLabel, "aria-describedby": describedBy, "aria-invalid": invalid }: SelectSheetProps) {
  const auto = useId();
  const valueId = `${auto}-value`;
  const button = useRef<HTMLButtonElement>(null);
  const [title, setTitle] = useState<string | null>(null);
  const current = options.find((o) => o.value === value);
  const firstEnabled = options.find((o) => !o.disabled);
  const focusValue = current && !current.disabled ? current.value : firstEnabled?.value;

  const show = () => { if (!disabled) setTitle(selectName(button.current, ariaLabel)); };
  const cancel = () => setTitle(null);
  const choose = (o: ComboOption) => {
    if (o.disabled) return;
    setTitle(null);
    if (o.value !== value) onChange(o.value);
  };
  const onTriggerKey = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (!OPEN_KEYS.has(e.key)) return;
    e.preventDefault();
    show();
  };

  return (
    <>
      <button ref={button} type="button" id={id} className={`ph-select ${className ?? ""}`.trim()} disabled={disabled}
        aria-haspopup="dialog" aria-expanded={title !== null} aria-label={ariaLabel} aria-invalid={invalid || undefined}
        aria-describedby={[valueId, describedBy].filter(Boolean).join(" ")} onClick={show} onKeyDown={onTriggerKey}>
        <span id={valueId} className="ph-select-value">{current?.label ?? ""}</span>
        <svg className="ph-select-arrow" width="12" height="7" viewBox="0 0 12 7" aria-hidden="true"><path d="M0 0h12L6 7z" /></svg>
      </button>
      {title !== null && (
        // The sheet renders in a portal, but React bubbles its events to this select's ancestors: a tap on a row must not
        // also reach a clickable row or tile that holds the select.
        <span className="select-sheet-host" onClick={stop} onKeyDown={stop} onPointerDown={stop}>
          <Dialog title={title} onClose={cancel} className="select-sheet">
            <SheetOptions label={title} options={options} value={value} focusValue={focusValue} onChoose={choose} />
          </Dialog>
        </span>
      )}
    </>
  );
}

/** Moves focus to the next enabled row in `step` direction (wrapping), or to the first / last for Home / End. */
function moveFocus(list: HTMLElement, key: string): boolean {
  const rows = [...list.querySelectorAll<HTMLButtonElement>("[role=option]:not(:disabled)")];
  if (!rows.length) return false;
  const at = rows.indexOf(document.activeElement as HTMLButtonElement);
  const next = key === "Home" ? 0 : key === "End" ? rows.length - 1
    : key === "ArrowDown" ? (at + 1) % rows.length
    : key === "ArrowUp" ? (at <= 0 ? rows.length - 1 : at - 1)
    : NaN;
  if (Number.isNaN(next)) return false;
  rows[next]?.focus();
  return true;
}

function SheetOptions({ label, options, value, focusValue, onChoose }: {
  label: string; options: readonly ComboOption[]; value: string; focusValue: string | undefined; onChoose: (o: ComboOption) => void;
}) {
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (moveFocus(e.currentTarget, e.key)) e.preventDefault();
  };
  return (
    <div role="listbox" aria-label={label} className="select-sheet-list" onKeyDown={onKeyDown}>
      {options.map((o) => {
        const selected = o.value === value;
        return (
          <button key={o.value} type="button" role="option" aria-selected={selected} disabled={o.disabled}
            className={`select-sheet-row ${selected ? "current" : ""}`.trim()} data-autofocus={o.value === focusValue ? "" : undefined}
            onClick={() => onChoose(o)}>
            <span className="select-sheet-label">{o.label}</span>
            <span className="select-sheet-mark" aria-hidden="true">{selected && <Icon name="check" />}</span>
          </button>
        );
      })}
    </div>
  );
}
