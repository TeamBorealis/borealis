import { useEffect } from "react";

export interface MenuItem { label: string; run: () => void; disabled?: boolean }

/** Right-click menu for source rows; closes on outside click, scroll, resize or Escape. */
export function SourceMenu({ x, y, items, onClose }: { x: number; y: number; items: MenuItem[]; onClose: () => void }) {
  useEffect(() => {
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", key);
    window.addEventListener("resize", onClose);
    window.addEventListener("scroll", onClose, true);
    return () => { window.removeEventListener("keydown", key); window.removeEventListener("resize", onClose); window.removeEventListener("scroll", onClose, true); };
  }, [onClose]);
  return (
    <>
      <div className="v4-ctx-scrim" onMouseDown={onClose} onContextMenu={(e) => { e.preventDefault(); onClose(); }} />
      <div className="aur-menu-panel v4-ctx" role="menu" aria-label="Source" style={{ left: x, top: y }}>
        {items.map((it) => (
          <button key={it.label} type="button" role="menuitem" className="aur-menu-item" disabled={it.disabled} onClick={() => { onClose(); it.run(); }}>{it.label}</button>
        ))}
      </div>
    </>
  );
}
