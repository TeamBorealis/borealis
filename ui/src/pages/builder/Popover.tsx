import { type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { OverlayPortal } from "../../components/controls/OverlayHost";

/** Gap (px) between the anchor and the popup, and the popup's minimum distance from the window edge. */
const GAP = 2;
const EDGE = 8;

interface Props {
  anchor: HTMLElement;
  /** "end": the popup's right edge meets the anchor's (version menu); "start": left edges meet (filter menus). */
  align: "start" | "end";
  /** `restoreFocus` is true for Escape: focus goes back to the anchor. */
  onClose: (restoreFocus: boolean) => void;
  className?: string;
  children: ReactNode;
}

/**
 * A small popup under an anchor (B-55 version menu, source checklist). Closes on Escape (focus returns to the anchor),
 * a press outside it, a scroll outside it or a window resize. Rendered in the window root, fixed to the viewport.
 */
export function Popover({ anchor, align, onClose, className = "", children }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  const close = useRef(onClose);
  close.current = onClose;

  useLayoutEffect(() => {
    const r = anchor.getBoundingClientRect();
    const w = ref.current?.offsetWidth ?? 0;
    const max = Math.max(EDGE, window.innerWidth - w - EDGE);
    const left = align === "end" ? Math.min(Math.max(EDGE, r.right - w), max) : Math.min(Math.max(EDGE, r.left), max);
    setPos({ top: r.bottom + GAP, left });
  }, [anchor, align]);

  useEffect(() => {
    const inside = (t: EventTarget | null) => t instanceof Node && (!!ref.current?.contains(t) || anchor.contains(t));
    const down = (e: Event) => { if (!inside(e.target)) close.current(false); };
    const scroll = (e: Event) => { if (!(e.target instanceof Node && ref.current?.contains(e.target))) close.current(false); };
    const resize = () => close.current(false);
    document.addEventListener("mousedown", down, true);
    document.addEventListener("touchstart", down, true);
    window.addEventListener("scroll", scroll, true);
    window.addEventListener("resize", resize);
    return () => {
      document.removeEventListener("mousedown", down, true);
      document.removeEventListener("touchstart", down, true);
      window.removeEventListener("scroll", scroll, true);
      window.removeEventListener("resize", resize);
    };
  }, [anchor]);

  return (
    <OverlayPortal>
      <div ref={ref} className={`pick-pop ${className}`.trim()} style={{ top: pos?.top ?? 0, left: pos?.left ?? 0, opacity: pos ? undefined : 0 }}
        onKeyDown={(e) => {
          if (e.key !== "Escape") return;
          e.preventDefault();
          e.stopPropagation();
          close.current(true);
          anchor.focus();
        }}>
        {children}
      </div>
    </OverlayPortal>
  );
}
