import { type KeyboardEvent, type ReactNode, useRef } from "react";

export interface ListBoxItemState { selected: boolean }

/** Metro list with item templates. Rest Light1 -> hover Light2 -> selected Light3 -> selected+hover Light4; 4px gap, 2px item border. */
export function ListBox<T>({ items, getKey, selected, onSelect, itemTemplate, label, wrap }: {
  items: readonly T[]; getKey: (item: T) => string; selected: readonly string[]; onSelect: (item: T) => void;
  itemTemplate: (item: T, state: ListBoxItemState) => ReactNode; label: string; wrap?: boolean;
}) {
  const ref = useRef<HTMLUListElement>(null);
  const move = (e: KeyboardEvent, i: number) => {
    const dir = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    ref.current?.querySelectorAll<HTMLElement>("[role=option]")[Math.max(0, Math.min(items.length - 1, i + dir))]?.focus();
  };
  return (
    <ul className={`aur-listbox ${wrap ? "wrap" : ""}`} role="listbox" aria-label={label} ref={ref}>
      {items.map((item, i) => {
        const key = getKey(item);
        const isSel = selected.includes(key);
        return (
          <li key={key} role="option" aria-selected={isSel} tabIndex={0} className="aur-listbox-item"
            onClick={() => onSelect(item)} onKeyDown={(e) => { move(e, i); if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onSelect(item); } }}>
            {itemTemplate(item, { selected: isSel })}
          </li>
        );
      })}
    </ul>
  );
}
