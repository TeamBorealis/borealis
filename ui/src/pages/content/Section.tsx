import type { ReactNode } from "react";
import "./content.css";

/** Classic builder stacked text section: 13px semibold uppercase header with optional right-aligned actions, 11px body. */
export function Section({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <section className="v4-csec" aria-label={title}>
      <header><h2>{title}</h2>{actions && <span className="act">{actions}</span>}</header>
      <div className="body">{children}</div>
    </section>
  );
}
