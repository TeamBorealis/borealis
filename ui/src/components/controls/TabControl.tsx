import { type KeyboardEvent, type ReactNode, useEffect, useRef } from "react";

export interface TabItem { id: string; label: ReactNode; badge?: number; disabled?: boolean }

/** Metro tab strip. variant "main" = 18px tabs with accent underline + pointer; "sub" = 14px band tabs. Roving tabindex + arrow keys. */
export function TabControl({ tabs, active, onChange, label, idPrefix = "tab", variant = "sub" }: {
  tabs: TabItem[]; active: string; onChange: (id: string) => void; label: string; idPrefix?: string; variant?: "main" | "sub";
}) {
  const ref = useRef<HTMLDivElement>(null);
  // Narrow strips scroll sideways: keep the selected tab in view.
  useEffect(() => {
    const el = ref.current?.querySelector<HTMLElement>("[aria-selected=true]");
    el?.scrollIntoView?.({ inline: "nearest", block: "nearest" });
  }, [active]);
  const move = (e: KeyboardEvent, i: number) => {
    const dir = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const enabled = tabs.filter((t) => !t.disabled);
    const cur = enabled.findIndex((t) => t.id === tabs[i]!.id);
    const next = enabled[(cur + dir + enabled.length) % enabled.length]!;
    onChange(next.id);
    ref.current?.querySelector<HTMLElement>(`#${idPrefix}-${next.id}`)?.focus();
  };
  return (
    <div className={`tabs ${variant}`} role="tablist" aria-label={label} ref={ref}>
      {tabs.map((t, i) => (
        <button key={t.id} id={`${idPrefix}-${t.id}`} data-testid={`${idPrefix}-step-${t.id}`} role="tab" type="button" className="tab" disabled={t.disabled} aria-selected={t.id === active}
          aria-controls={`${idPrefix}-panel`} tabIndex={t.id === active ? 0 : -1} onClick={() => onChange(t.id)} onKeyDown={(e) => move(e, i)}>
          {t.label}
          {t.badge ? <span className="badge-count" aria-label={`${t.badge} choices remaining`}>{t.badge}</span> : null}
        </button>
      ))}
    </div>
  );
}
