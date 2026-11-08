import type { ReactNode } from "react";
import { type GuideId, GuideLink } from "../../components/GuideLink";
import "./content.css";

/** Classic builder stacked text section: 13px semibold uppercase header with optional right-aligned actions, 11px body.
 *  `guide` puts the website guide's help button right after the title (U18). */
export function Section({ title, actions, guide, children }: { title: string; actions?: ReactNode; guide?: GuideId; children: ReactNode }) {
  const heading = guide
    ? <span className="v4-csec-title"><h2>{title}</h2><GuideLink guide={guide} /></span>
    : <h2>{title}</h2>;
  return (
    <section className="v4-csec" aria-label={title}>
      <header>{heading}{actions && <span className="act">{actions}</span>}</header>
      <div className="body">{children}</div>
    </section>
  );
}
