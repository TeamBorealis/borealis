import type { HTMLAttributes, ReactNode } from "react";

export function Card({ title, actions, children, className = "", ...rest }: { title?: ReactNode; actions?: ReactNode } & HTMLAttributes<HTMLElement>) {
  return (
    <section className={`card ${className}`} {...rest}>
      {(title || actions) && (
        <div className="card-head">
          {title && <h2>{title}</h2>}
          <span className="spacer" />
          {actions}
        </div>
      )}
      {children}
    </section>
  );
}

export type ChipTone = "default" | "accent" | "gold" | "success" | "danger" | "warning" | "info";

export function Chip({ tone = "default", children, title }: { tone?: ChipTone; children: ReactNode; title?: string }) {
  return (
    <span className={`chip ${tone === "default" ? "" : tone}`} title={title}>
      {children}
    </span>
  );
}

export function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <strong>{title}</strong>
      {children && <div>{children}</div>}
    </div>
  );
}

export function Alert({ tone = "info", children, role }: { tone?: "info" | "warning" | "danger" | "success"; children: ReactNode; role?: "alert" | "status" }) {
  return (
    <div className={`alert ${tone}`} role={role ?? (tone === "danger" ? "alert" : "status")}>
      <div>{children}</div>
    </div>
  );
}

export function ProgressBar({ value, max, label }: { value: number; max: number; label: string }) {
  const pct = max <= 0 ? 0 : Math.min(100, Math.max(0, (value / max) * 100));
  return (
    <div className="progress" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={max} aria-valuenow={Math.min(value, max)}>
      <span style={{ width: `${pct}%` }} />
    </div>
  );
}
