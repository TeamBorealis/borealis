import type { ReactNode } from "react";

export function Stat({ label, value, sub, className = "", valueTestId }: { label: string; value: ReactNode; sub?: ReactNode; className?: string; valueTestId?: string }) {
  return (
    <div className={`stat ${className}`}>
      <span className="stat-label">{label}</span>
      <span className="stat-value" data-testid={valueTestId}>{value}</span>
      {sub !== undefined && <span className="stat-sub">{sub}</span>}
    </div>
  );
}

export function StatRow({ children }: { children: ReactNode }) {
  return <div className="stat-row">{children}</div>;
}

export const signed = (n: number): string => (n >= 0 ? `+${n}` : `${n}`);
