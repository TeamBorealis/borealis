import { type KeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from "react";
import { OverlayPortal } from "../../../components/controls/OverlayHost";

export interface PressMenuItem { label: string; run: () => void; disabled?: boolean }

/** Space kept between the menu and the window edge. */
const EDGE_PX = 8;

function place(anchor: DOMRect, menu: DOMRect): { left: number; top: number } {
  const left = Math.max(EDGE_PX, Math.min(anchor.left, window.innerWidth - menu.width - EDGE_PX));
  const below = anchor.bottom + 2;
  // a button in the lower half opens the menu upwards, clear of the phone's tab bar and the result card
  const up = anchor.top > window.innerHeight / 2 || below + menu.height > window.innerHeight - EDGE_PX;
  const top = up ? Math.max(EDGE_PX, anchor.top - menu.height - 2) : below;
  return { left, top };
}

/**
 * The roll menu (Aurora's menu panel, as the Sources right-click menu): opens at the button, focus on the first item,
 * arrows move, Enter picks, Escape or a press outside closes and gives focus back to the button.
 */
export function PressMenu({ anchor, label, items, onClose }: { anchor: HTMLElement; label: string; items: PressMenuItem[]; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const close = () => {
    onClose();
    anchor.focus();
  };
  useLayoutEffect(() => {
    if (ref.current) setPos(place(anchor.getBoundingClientRect(), ref.current.getBoundingClientRect()));
  }, [anchor]);
  // focus once placed: a hidden (still unplaced) item cannot take focus in a browser
  const placed = pos !== null;
  useEffect(() => {
    if (placed) ref.current?.querySelector<HTMLButtonElement>("[role=menuitem]:not(:disabled)")?.focus();
  }, [placed]);
  useEffect(() => {
    const away = () => onClose();
    // Escape closes the menu wherever the focus is
    const key = (e: globalThis.KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("resize", away);
    window.addEventListener("keydown", key);
    return () => { window.removeEventListener("resize", away); window.removeEventListener("keydown", key); };
  }, [onClose]);
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const all = Array.from(ref.current?.querySelectorAll<HTMLButtonElement>("[role=menuitem]:not(:disabled)") ?? []);
    const at = all.indexOf(document.activeElement as HTMLButtonElement);
    const go = (i: number) => { e.preventDefault(); all[(i + all.length) % all.length]?.focus(); };
    if (e.key === "ArrowDown") go(at + 1);
    else if (e.key === "ArrowUp") go(at - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(all.length - 1);
    else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); }
    else if (e.key === "Tab") { e.preventDefault(); close(); }
  };
  return (
    <OverlayPortal>
      <div className="v4-ctx-scrim pr-scrim" onPointerDown={(e) => { e.preventDefault(); close(); }} onContextMenu={(e) => { e.preventDefault(); close(); }} />
      <div ref={ref} className="aur-menu-panel v4-ctx pr-menu" role="menu" aria-label={label} onKeyDown={onKeyDown}
        style={pos ? { left: pos.left, top: pos.top } : { left: 0, top: 0, visibility: "hidden" }}>
        {items.map((it) => (
          <button key={it.label} type="button" role="menuitem" className="aur-menu-item" disabled={it.disabled}
            onClick={() => { close(); it.run(); }}>{it.label}</button>
        ))}
      </div>
    </OverlayPortal>
  );
}
