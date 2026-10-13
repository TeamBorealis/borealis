import type { ReactNode } from "react";
import { DescriptionPane } from "../builder/DescriptionPane";
import "../builder/builder.css";
import "./pane.css";

/** Classic builder page grid: content on the left (11*), the builder DescriptionPane on the right (5*, below on phones). */
export function TwoPane({ left, descId, descEmpty }: { left: ReactNode; descId: string | null; descEmpty?: boolean }) {
  return (
    <div className="ap-page">
      <div className="ap-left">{left}</div>
      <div className={`ap-right ${descEmpty ? "ap-none" : ""}`}>
        <DescriptionPane elementId={descId} fallback={<aside className="desc-pane" aria-label="Description" />} />
      </div>
    </div>
  );
}

/** Highlight header band with a title and right-aligned stats (`ABILITY: INT`). Values render gold. */
export function Band({ title, stats, children }: { title: ReactNode; stats?: [string, ReactNode][]; children?: ReactNode }) {
  return (
    <div className="ap-band">
      <h2>{title}</h2>
      {children}
      {stats && (
        <span className="ap-stats">
          {stats.map(([k, v]) => <span key={k}>{k}<b>{v}</b></span>)}
        </span>
      )}
    </div>
  );
}
