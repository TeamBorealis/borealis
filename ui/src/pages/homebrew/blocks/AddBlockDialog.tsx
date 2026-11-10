import { useMemo, useState } from "react";
import type { HomebrewBlockSpec } from "../../../api/contract";
import { Button, CheckBox, Dialog, Expander, SearchInput } from "../../../components";
import { useCreatorEnv } from "../env";

/** The Add menu's entries (spec 6.2, hc-wb05): only `kinds` when given (a toggle's effects); effect-only kinds only there;
 * core-rule kinds only when asked for. */
export function offeredBlocks(catalog: readonly HomebrewBlockSpec[], kinds: readonly string[] | undefined, showCore: boolean): HomebrewBlockSpec[] {
  return catalog.filter((b) => (kinds ? kinds.includes(b.kind) : !b.effectOnly) && (showCore || !b.coreOnly));
}

/** Groups in catalog order, Common first. */
export function addMenuGroups(blocks: readonly HomebrewBlockSpec[]): { id: string; label: string; blocks: HomebrewBlockSpec[] }[] {
  const common = blocks.filter((b) => b.common);
  const groups = new Map<string, { id: string; label: string; blocks: HomebrewBlockSpec[] }>();
  for (const b of blocks) {
    if (b.common) continue;
    const g = groups.get(b.group) ?? { id: b.group, label: b.groupLabel, blocks: [] };
    groups.set(b.group, { ...g, blocks: [...g.blocks, b] });
  }
  return [...(common.length ? [{ id: "common", label: "Common", blocks: common }] : []), ...groups.values()];
}

/**
 * The Add menu (spec 6.2): the rule kinds in plain words, Common first, the rest under their groups ("More"), with a
 * search. Experts can show the core-rule blocks. A bottom sheet on phones.
 */
export function AddBlockDialog({ kinds, onPick, onClose }: { kinds?: readonly string[]; onPick: (spec: HomebrewBlockSpec) => void; onClose: () => void }) {
  const env = useCreatorEnv();
  const [query, setQuery] = useState("");
  const [showCore, setShowCore] = useState(false);
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const groups = useMemo(() => {
    const offered = offeredBlocks(env.blocks, kinds, showCore).filter((b) => words.every((w) => `${b.label} ${b.groupLabel}`.toLowerCase().includes(w)));
    return addMenuGroups(offered);
  }, [env.blocks, kinds, showCore, words.join(" ")]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <Dialog title="Add a rule" onClose={onClose} className="hb-add-block" footer={<Button onClick={onClose}>Cancel</Button>}>
      <SearchInput label="Search rules" value={query} onChange={setQuery} data-autofocus />
      {env.blocks.length === 0 && <p className="pick-empty">The rule kinds are loading.</p>}
      <div className="hb-stat-groups">
        {groups.map((g) => (
          <Expander key={`${g.id}-${words.length > 0}`} title={g.id === "common" ? "COMMON" : g.label.toUpperCase()} defaultOpen={g.id === "common" || words.length > 0 || !!kinds}>
            <ul className="hb-stat-list" aria-label={g.label}>
              {g.blocks.map((b) => (
                <li key={b.kind}><button type="button" className="hb-stat-row" onClick={() => onPick(b)}>{b.label}</button></li>
              ))}
            </ul>
          </Expander>
        ))}
      </div>
      {env.expert && !kinds && (
        <label className="hb-inline-check"><CheckBox checked={showCore} onChange={(e) => setShowCore(e.target.checked)} />Show core-rule blocks</label>
      )}
    </Dialog>
  );
}
