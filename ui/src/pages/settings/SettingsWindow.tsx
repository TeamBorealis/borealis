import { type FormEvent, type KeyboardEvent, type ReactNode, useId, useRef } from "react";
import { IconButton } from "../../components";
import { useModalBehavior } from "../../components/controls/useModalBehavior";
import "./settings.css";

export interface WindowTab { id: string; label: string }

/** Left vertical tab strip (classic builder's AlteredTabControl): uppercase headers, accent underline on the panel side of the selected one. */
function VerticalTabs({ tabs, active, onChange, label, idPrefix }: { tabs: readonly WindowTab[]; active: string; onChange: (id: string) => void; label: string; idPrefix: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const move = (e: KeyboardEvent, i: number) => {
    const dir = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const next = tabs[(i + dir + tabs.length) % tabs.length]!;
    onChange(next.id);
    ref.current?.querySelector<HTMLElement>(`#${idPrefix}-${next.id}`)?.focus();
  };
  return (
    <div className="aur-set-tabs" role="tablist" aria-orientation="vertical" aria-label={label} ref={ref}>
      {tabs.map((t, i) => (
        <button key={t.id} id={`${idPrefix}-${t.id}`} role="tab" type="button" aria-selected={t.id === active} aria-controls={`${idPrefix}-panel`}
          data-autofocus={t.id === active ? "" : undefined} tabIndex={t.id === active ? 0 : -1} onClick={() => onChange(t.id)} onKeyDown={(e) => move(e, i)}>{t.label}</button>
      ))}
    </div>
  );
}

/**
 * The classic builder's owned settings window: 500x600, centred, no scrim, accent title bar, left tab strip, scrolling content, footer bar.
 * Escape closes (Cancel); `onSubmit` (Enter / the default button) is optional.
 */
export function SettingsWindow({ title, tabs, active, onTab, tabsLabel, idPrefix, onClose, onSubmit, children, footer, narrowPad }: {
  title: string; tabs: readonly WindowTab[]; active: string; onTab: (id: string) => void; tabsLabel: string; idPrefix: string;
  onClose: () => void; onSubmit?: () => void; children: ReactNode; footer: ReactNode; narrowPad?: boolean;
}) {
  const { ref, onKeyDown } = useModalBehavior(onClose);
  const titleId = useId();
  const submit = (e: FormEvent) => { e.preventDefault(); onSubmit?.(); };
  return (
    <div className="overlay aur-set-overlay" onKeyDown={onKeyDown}>
      <div className="aur-set-win" role="dialog" aria-modal="true" aria-labelledby={titleId} ref={ref}>
        <div className="aur-set-head">
          <h2 id={titleId}>{title}</h2>
          <IconButton icon="x" label="Close" onClick={onClose} />
        </div>
        <form className="aur-set-form" onSubmit={submit}>
          <div className="aur-set-main">
            <VerticalTabs tabs={tabs} active={active} onChange={onTab} label={tabsLabel} idPrefix={idPrefix} />
            <div className={`aur-set-scroll ${narrowPad ? "r10" : "r20"}`} role="tabpanel" id={`${idPrefix}-panel`} aria-labelledby={`${idPrefix}-${active}`}>{children}</div>
          </div>
          <div className="aur-set-foot">{footer}</div>
        </form>
      </div>
    </div>
  );
}
