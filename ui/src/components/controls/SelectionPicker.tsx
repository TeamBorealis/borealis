import { type ReactNode } from "react";
import { Icon } from "../Icon";

/** The 4px focus/selection bar: wraps content; the bar lights up (gold) while anything inside has focus. */
export function FocusBar({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <div className={`focus-bar ${className}`}>{children}</div>;
}

/** Picker frame: stack of Expander sections inside the focus bar, with the 24px footer slot. */
export function SelectionPicker({ children, footer, label }: { children: ReactNode; footer?: ReactNode; label: string }) {
  return (
    <div className="selection-picker" role="group" aria-label={label}>
      <FocusBar>{children}</FocusBar>
      {footer}
    </div>
  );
}

/** One picker row: [check] name ....... source [x]. Click selects; the x clears a selected row. */
export function PickerItem({ name, source, selected, onSelect, onClear, disabled, action }: {
  name: ReactNode; source?: ReactNode; selected?: boolean; onSelect: () => void; onClear?: () => void; disabled?: boolean; action?: ReactNode;
}) {
  return (
    <div className={`picker-item ${selected ? "selected" : ""}`}>
      <button type="button" className="picker-item-main" aria-pressed={!!selected} disabled={disabled} onClick={onSelect}>
        <span className="picker-item-check">{selected && <Icon name="check-box" />}</span>
        <span className="picker-item-name">{name}</span>
        {source && <span className="picker-item-source">{source}</span>}
      </button>
      {action && <span className="picker-item-action">{action}</span>}
      {selected && onClear && <button type="button" className="picker-item-clear" aria-label="Clear selection" onClick={onClear}><Icon name="x" /></button>}
    </div>
  );
}

/** 24px "choices remaining" footer. */
export function ChoicesFooter({ remaining, total }: { remaining: number; total?: number }) {
  const done = remaining <= 0;
  return (
    <div className={`choices-footer ${done ? "done" : ""}`} role="status">
      {done ? "All choices made" : `${remaining} ${remaining === 1 ? "choice" : "choices"} remaining${total ? ` of ${total}` : ""}`}
    </div>
  );
}
