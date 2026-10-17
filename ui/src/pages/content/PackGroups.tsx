import type { ReactNode } from "react";
import type { ContentPack } from "../../api/contract";
import { Expander } from "../../components/controls";

export type PackOrigin = "bundled" | "aurora-index" | "5etools" | "custom" | "ai";

/** Display order and labels of the origin groups. */
export const ORIGIN_GROUPS: { origin: PackOrigin; label: string }[] = [
  { origin: "bundled", label: "Built-in" },
  { origin: "aurora-index", label: "Aurora index" },
  { origin: "5etools", label: "5etools" },
  { origin: "custom", label: "Custom" },
  { origin: "ai", label: "AI import" },
];

/**
 * Where a pack came from: index downloads are `downloaded`, 5etools conversions `local-import`,
 * AI imports of the user's own documents `imported` + not redistributable, other user files `imported`/`homebrew`.
 */
export function packOrigin(p: ContentPack): PackOrigin {
  switch (p.kind) {
    case "bundled": return "bundled";
    case "downloaded": return "aurora-index";
    case "local-import": return "5etools";
    case "imported": return p.redistributable === false ? "ai" : "custom";
    default: return "custom";
  }
}

/** Phones: installed packs grouped by origin, each group collapsed with its count, so a long 5etools import stays short. */
export function PackGroups({ packs, row }: { packs: ContentPack[]; row: (p: ContentPack) => ReactNode }) {
  const groups = ORIGIN_GROUPS
    .map((g) => ({ ...g, packs: packs.filter((p) => packOrigin(p) === g.origin) }))
    .filter((g) => g.packs.length > 0);
  return (
    <div className="v4-pack-groups" role="group" aria-label="Content packs by origin">
      {groups.map((g) => (
        <Expander key={g.origin} title={`${g.label} (${g.packs.length})`} defaultOpen={false}>
          <ul className="list" aria-label={`${g.label} packs`}>{g.packs.map(row)}</ul>
        </Expander>
      ))}
    </div>
  );
}
