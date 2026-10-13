import { type ReactNode, useId } from "react";
import { OverlayPortal } from "./OverlayHost";
import { useModalBehavior } from "./useModalBehavior";

/** Spec widths (px): default 400, wide 460, coinage 500, app-settings 300, portraits 435. */
export const FLYOUT_WIDTHS = { default: 400, wide: 460, coinage: 500, settings: 300, portraits: 435 } as const;
export type FlyoutWidth = keyof typeof FLYOUT_WIDTHS;

/** Right-side (or left-side) panel over a 50% scrim. Bordered centred uppercase title (no close button: Cancel, Esc or a scrim click closes), scrolling content, optional footer. Full width on phones. */
export function Flyout({ title, onClose, children, footer, width = "default", position = "right" }: {
  title: ReactNode; onClose: () => void; children: ReactNode; footer?: ReactNode; width?: FlyoutWidth; position?: "right" | "left" | "bottom";
}) {
  const { ref, onKeyDown } = useModalBehavior(onClose);
  const titleId = useId();
  return (
    <OverlayPortal>
      <div className={`overlay flyout-overlay ${position}`} onMouseDown={(e) => e.target === e.currentTarget && onClose()} onKeyDown={onKeyDown}>
        <div className="flyout" role="dialog" aria-modal="true" aria-labelledby={titleId} ref={ref} style={{ ["--flyout-w" as string]: `${FLYOUT_WIDTHS[width]}px` }}>
          <div className="flyout-head">
            <h2 id={titleId} className="flyout-title">{title}</h2>
          </div>
          <div className="flyout-body">{children}</div>
          {footer && <div className="flyout-foot">{footer}</div>}
        </div>
      </div>
    </OverlayPortal>
  );
}
