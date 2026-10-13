import { type ReactNode, useId, useState } from "react";
import { Icon } from "../Icon";

/** expander: Highlight header bar with uppercase title, optional header slot, collapsible content and footer slot. */
export function Expander({ title, headerExtra, footer, footerWhenCollapsed, children, defaultOpen = true, flyout, open: controlled, onToggle }: {
  title: ReactNode; headerExtra?: ReactNode; footer?: ReactNode; footerWhenCollapsed?: boolean; children: ReactNode; defaultOpen?: boolean; flyout?: boolean;
  open?: boolean; onToggle?: (open: boolean) => void;
}) {
  const [inner, setInner] = useState(defaultOpen);
  const open = controlled ?? inner;
  const id = useId();
  const toggle = () => { setInner(!open); onToggle?.(!open); };
  return (
    <section className={`aur-expander ${flyout ? "flyout-variant" : ""}`}>
      <div className="aur-expander-head">
        <button type="button" className="aur-expander-toggle" aria-expanded={open} aria-controls={id} onClick={toggle}>
          <span className="aur-expander-title">{title}</span>
        </button>
        {headerExtra && <span className="aur-expander-extra">{headerExtra}</span>}
        <button type="button" className="aur-expander-chev" tabIndex={-1} aria-hidden="true" onClick={toggle}><Icon name={open ? "up" : "down"} /></button>
      </div>
      {open && <div id={id} className="aur-expander-body">{children}</div>}
      {(open || footerWhenCollapsed) && footer && <div className="aur-expander-footer">{footer}</div>}
    </section>
  );
}
