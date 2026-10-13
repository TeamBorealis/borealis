import type { ReactNode } from "react";
import { Button } from "./Button";

/** Two panes on wide screens; list-then-detail with a back button on phones. */
export function ListDetail({ list, detail, hasSelection, onBack, backLabel = "Back to list" }: {
  list: ReactNode; detail: ReactNode; hasSelection: boolean; onBack: () => void; backLabel?: string;
}) {
  return (
    <div className="list-detail" data-view={hasSelection ? "detail" : "list"}>
      <div className="ld-list">{list}</div>
      <div className="ld-detail">
        <Button className="back" variant="ghost" icon="back" onClick={onBack} style={{ marginBottom: "var(--sp-2)" }}>{backLabel}</Button>
        {detail}
      </div>
    </div>
  );
}
