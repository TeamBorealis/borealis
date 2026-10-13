import { type ReactNode, useId } from "react";
import { type ButtonVariant, IconButton } from "../Button";
import { Button } from "../Button";
import { OverlayPortal } from "./OverlayHost";
import { useModalBehavior } from "./useModalBehavior";

/** Classic builder dialog window: accent title bar (13px SemiBold, left aligned), flat body, footer button row. Bottom sheet on phones. */
export function Dialog({ title, onClose, children, footer, wide, full, className }: {
  title: ReactNode; onClose: () => void; children: ReactNode; footer?: ReactNode; wide?: boolean; full?: boolean; className?: string;
}) {
  const { ref, onKeyDown } = useModalBehavior(onClose);
  const titleId = useId();
  return (
    <OverlayPortal>
      <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()} onKeyDown={onKeyDown}>
        <div className={`dialog ${wide ? "wide" : ""} ${full ? "full" : ""} ${className ?? ""}`.trim()} role="dialog" aria-modal="true" aria-labelledby={titleId} ref={ref}>
          <div className="dialog-head">
            <h2 id={titleId}>{title}</h2>
            <IconButton icon="x" label="Close" onClick={onClose} />
          </div>
          <div className="dialog-body">{children}</div>
          {footer && <div className="dialog-foot">{footer}</div>}
        </div>
      </div>
    </OverlayPortal>
  );
}

export interface MessageBoxButton { id: string; label: string; variant?: ButtonVariant }

/** Modal message with a result button row (OK / Yes-No style). Escape or the close button resolves with `cancelId`. */
export function MessageBox({ title, message, buttons, onResult, cancelId }: {
  title: ReactNode; message: ReactNode; buttons: MessageBoxButton[]; onResult: (id: string) => void; cancelId?: string;
}) {
  const cancel = cancelId ?? buttons[buttons.length - 1]?.id ?? "";
  return (
    <Dialog
      title={title}
      onClose={() => onResult(cancel)}
      footer={buttons.map((b, i) => (
        <Button key={b.id} variant={b.variant ?? (i === 0 ? "primary" : "default")} data-autofocus={i === 0 ? "" : undefined} onClick={() => onResult(b.id)}>{b.label}</Button>
      ))}
    >
      <div className="messagebox-body">{message}</div>
    </Dialog>
  );
}
