import { type ReactNode, useId } from "react";
import "./manage.css";

/** Classic builder form field: small uppercase label above a dark control. */
export function MgField({ label, children, className = "" }: { label: ReactNode; children: (id: string) => ReactNode; className?: string }) {
  const id = useId();
  return <div className={`mg-f ${className}`}><label htmlFor={id}>{label}</label>{children(id)}</div>;
}

/** Section with the 13px uppercase title and a grey paragraph. */
export function MgSection({ title, sub, children, className = "" }: { title: string; sub?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={`mg-sec ${className}`} aria-label={title}>
      <h2>{title}</h2>
      {sub && <p className="mg-sub">{sub}</p>}
      {children}
    </section>
  );
}
