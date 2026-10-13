import { type ReactNode } from "react";
import { TabControl } from "./controls/TabControl";
import { ShellSubTabs } from "./shell/ShellContext";

export interface TabDef {
  id: string;
  label: ReactNode;
  badge?: number;
}

/**
 * Accessible tab strip (roles + arrow-key navigation). Panels are rendered by the caller.
 * `inBand` hoists the strip into the shell's 30px sub-tab band (classic builder look); without a shell it renders inline.
 */
export function Tabs({ tabs, active, onChange, label, idPrefix = "tab", inBand }: { tabs: TabDef[]; active: string; onChange: (id: string) => void; label: string; idPrefix?: string; inBand?: boolean }) {
  const strip = <TabControl tabs={tabs} active={active} onChange={onChange} label={label} idPrefix={idPrefix} variant="sub" />;
  return inBand ? <ShellSubTabs>{strip}</ShellSubTabs> : strip;
}

export function TabPanel({ active, idPrefix = "tab", children }: { active: string; idPrefix?: string; children: ReactNode }) {
  return (
    <div role="tabpanel" id={`${idPrefix}-panel`} aria-labelledby={`${idPrefix}-${active}`} tabIndex={-1} className="tab-panel">
      {children}
    </div>
  );
}

export function Segmented<T extends string>({ options, value, onChange, label }: { options: { id: T; label: string }[]; value: T; onChange: (v: T) => void; label: string }) {
  return (
    <div className="segmented" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.id} type="button" aria-pressed={o.id === value} onClick={() => onChange(o.id)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}
