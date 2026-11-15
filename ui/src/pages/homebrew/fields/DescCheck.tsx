import type { ReactNode } from "react";
import { CheckBox } from "../../../components";

/**
 * Aurora's DescriptionCheckBox in a creator form (spec 13.4, U7): the 18 px box with its name beside it (11 px SemiBold),
 * the help indented under the name (11 px secondary). The name is the box's `<label>`, so a click on it toggles the box.
 */
export function DescCheck({ id, label, checked, onChange, help, describedBy }: {
  id: string; label: ReactNode; checked: boolean; onChange: (on: boolean) => void; help?: ReactNode; describedBy?: string;
}) {
  return (
    <div className="hb-dcheck">
      <CheckBox id={id} checked={checked} aria-describedby={describedBy} onChange={(e) => onChange(e.target.checked)} />
      <label htmlFor={id} className="hb-dcheck-head">{label}</label>
      {help && <span className="hb-dcheck-desc">{help}</span>}
    </div>
  );
}
