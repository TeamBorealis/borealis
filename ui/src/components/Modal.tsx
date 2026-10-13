import { type ReactNode } from "react";
import { Dialog } from "./controls/Dialog";
import { Flyout } from "./controls/Flyout";

export type ModalKind = "modal" | "drawer";

/** Dialog (centred; bottom sheet on phones) or, for kind "drawer", a right-side Flyout. Traps focus, closes on Escape and scrim click. */
export function Modal({ title, onClose, children, footer, kind = "modal", wide, full, className }: {
  title: ReactNode; onClose: () => void; children: ReactNode; footer?: ReactNode; kind?: ModalKind; wide?: boolean; full?: boolean; className?: string;
}) {
  if (kind === "drawer") return <Flyout title={title} onClose={onClose} footer={footer}>{children}</Flyout>;
  return <Dialog title={title} onClose={onClose} footer={footer} wide={wide} full={full} className={className}>{children}</Dialog>;
}
