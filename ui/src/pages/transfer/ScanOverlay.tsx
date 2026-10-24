import { useEffect } from "react";
import { Button } from "../../components";
import { OverlayPortal } from "../../components/controls/OverlayHost";

const SCANNING_CLASS = "aur-scanning";

/**
 * Scan screen drawn over the camera. The scanner plugin's windowed mode shows the camera behind a
 * transparent web view, so while this is up the page turns transparent and only this layer shows:
 * the dialog title bar, a framed viewfinder over the 50% scrim, a hint, and the fallbacks.
 */
export function ScanOverlay({ title, onType, onCancel }: { title: string; onType: () => void; onCancel: () => void }) {
  useEffect(() => {
    const root = document.documentElement;
    root.classList.add(SCANNING_CLASS);
    return () => root.classList.remove(SCANNING_CLASS);
  }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onCancel(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);
  return (
    <OverlayPortal>
      <div className="xfer-scan" role="dialog" aria-modal="true" aria-label={title}>
        <div className="dialog-head"><h2>{title}</h2></div>
        <div className="xfer-scan-view">
          <div className="xfer-scan-frame" aria-hidden />
          <p className="xfer-scan-hint" role="status">Point the camera at the QR code on the other device.</p>
        </div>
        <div className="dialog-foot">
          <Button onClick={onType}>Type the code instead</Button>
          <Button onClick={onCancel}>Cancel</Button>
        </div>
      </div>
    </OverlayPortal>
  );
}
