import qrcode from "qrcode-generator";
import { useMemo } from "react";

const QUIET_ZONE = 4;

/** Renders `value` as a QR code (black on white, with a quiet zone, so phone cameras read it in either theme). */
export function QrCode({ value, label, size = 168 }: { value: string; label: string; size?: number }) {
  const path = useMemo(() => {
    const qr = qrcode(0, "M");
    qr.addData(value);
    qr.make();
    const n = qr.getModuleCount();
    let d = "";
    for (let r = 0; r < n; r++) {
      for (let c = 0; c < n; c++) if (qr.isDark(r, c)) d += `M${c + QUIET_ZONE} ${r + QUIET_ZONE}h1v1h-1z`;
    }
    return { d, span: n + QUIET_ZONE * 2 };
  }, [value]);
  return (
    <svg role="img" aria-label={label} width={size} height={size} viewBox={`0 0 ${path.span} ${path.span}`} shapeRendering="crispEdges">
      <rect width={path.span} height={path.span} fill="#fff" />
      <path d={path.d} fill="#000" />
    </svg>
  );
}
