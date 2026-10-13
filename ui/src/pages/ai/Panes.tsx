import type { ReactNode } from "react";
import "./panes.css";

/** Classic builder two-pane screen: list pane (11*) beside a description pane (5*). `chat` widens the description pane. */
export function PaneSplit({ list, desc, chat, label }: { list: ReactNode; desc: ReactNode; chat?: boolean; label?: string }) {
  return (
    <div className={`v5-split ${chat ? "wide-desc" : ""}`} aria-label={label}>
      <div className="v5-list-pane">{list}</div>
      <aside className="v5-desc" aria-label={label ? `${label} details` : "Details"}>{desc}</aside>
    </div>
  );
}

/** Description pane header: big uppercase title, italic lead. */
export function DescHead({ title, lead }: { title: ReactNode; lead?: ReactNode }) {
  return <><h1 className="v5-desc-title">{title}</h1>{lead && <p className="v5-lead">{lead}</p>}</>;
}

export type TagTone = "ok" | "warn" | "bad" | "info" | "";
export const Tag = ({ tone = "", title, children }: { tone?: TagTone; title?: string; children: ReactNode }) => <span className={`v5-tag ${tone}`} title={title}>{children}</span>;

/** classic-builder-style toggle-button group (flat Light1 buttons, selected Light3). */
export function Picker<T extends string>({ label, options, value, onChange }: { label: string; options: { id: T; label: string }[]; value: T; onChange: (v: T) => void }) {
  return (
    <div className="v5-picker" role="group" aria-label={label}>
      {options.map((o) => <button key={o.id} type="button" aria-pressed={o.id === value} onClick={() => onChange(o.id)}>{o.label}</button>)}
    </div>
  );
}
