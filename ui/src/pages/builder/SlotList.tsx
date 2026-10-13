import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import type { SelectionSlot } from "../../api/contract";
import { useCharacter } from "../../state/CharacterContext";
import { DescriptionPane } from "./DescriptionPane";
import { type DescribeFn, groupSlots, isListSlot, PickerGroup, type SlotGroup } from "./PickerGroup";

const EMPTY_TEXT = "Your character doesn't have any selections available here. This area fills in as your build allows for choices.";

export interface SlotListProps {
  slots: SelectionSlot[];
  emptyText?: string;
  /** When the page owns a description pane, it passes the shared described id + setter; otherwise a pane renders inline. */
  describedId?: string | null;
  onDescribe?: DescribeFn;
}

/** Renders every selection slot of a tab as stacked classic builder pickers. */
export function SlotList({ slots, emptyText = EMPTY_TEXT, describedId, onDescribe }: SlotListProps) {
  const { snapshot, mutate } = useCharacter();
  const [params, setParams] = useSearchParams();
  const wanted = params.get("slot");
  const groups = useMemo(() => groupSlots(slots), [slots]);
  const [openKeys, setOpenKeys] = useState<Record<string, boolean>>({});
  const [advance, setAdvance] = useState(false);
  const [local, setLocal] = useState<{ id: string; isElement: boolean } | null>(null);
  const described = onDescribe ? describedId ?? null : local?.id ?? null;
  const describe: DescribeFn = onDescribe ?? ((id, isElement = true) => setLocal({ id, isElement }));
  // Wanted slot (selection flow / "next selection") opens its group.
  useEffect(() => {
    if (!wanted) return;
    const g = groups.find((x) => x.slots.some((s) => s.id === wanted));
    if (g) setOpenKeys((o) => ({ ...o, [g.key]: true }));
  }, [wanted, groups]);
  // After a choice lands, the chosen picker follows its default (collapsed) and the next pending one opens.
  useEffect(() => {
    if (!advance) return;
    setAdvance(false);
    const pending = groups.find((g) => g.slots.some((s) => s.required && !s.chosen && !s.locked));
    if (pending) setOpenKeys((o) => ({ ...o, [pending.key]: true }));
  }, [advance, groups]);
  if (!snapshot) return null;
  /** Classic builder: a picker with a pending pick is open; once chosen it collapses to its footer line. */
  const isOpen = (g: SlotGroup): boolean => {
    const set = openKeys[g.key];
    if (set !== undefined) return set;
    return g.slots.some((s) => !s.locked && (!s.chosen || s.id === wanted));
  };
  const clearWanted = () => { if (wanted) { const p = new URLSearchParams(params); p.delete("slot"); setParams(p, { replace: true }); } };
  const choose = async (slotId: string, elementId: string) => {
    clearWanted();
    const g = groups.find((x) => x.slots.some((s) => s.id === slotId));
    describe(elementId, !g?.slots.some((s) => s.id === slotId && isListSlot(s)));
    if (g) setOpenKeys((m) => Object.fromEntries(Object.entries(m).filter(([k]) => k !== g.key)));
    await mutate((api, id) => api.applySelection({ characterId: id, slotId, elementId }));
    setAdvance(true);
  };
  const clear = (slotId: string) => {
    const g = groups.find((x) => x.slots.some((s) => s.id === slotId));
    if (g) setOpenKeys((m) => ({ ...m, [g.key]: true }));
    void mutate((api, id) => api.clearSelection({ characterId: id, slotId }));
  };
  if (groups.length === 0) return <p className="build-empty">{emptyText}</p>;
  const list = (
    <div className="pick-stack">
      {groups.map((g, i) => (
        <section key={g.key} aria-label={g.label ?? g.title}>
          {g.label && (i === 0 || groups[i - 1]!.label !== g.label) && <h3 className="pick-group-label">{g.label}</h3>}
          <PickerGroup characterId={snapshot.id} group={g} isNext={g.slots.some((s) => s.id === wanted)}
            open={isOpen(g)} onToggle={(o) => setOpenKeys((m) => ({ ...m, [g.key]: o }))}
            describedId={described} onDescribe={describe} onChoose={(s, e) => void choose(s, e)} onClear={clear} />
        </section>
      ))}
    </div>
  );
  if (onDescribe) return list;
  return <>{list}{local?.isElement && <div className="pick-inline-desc"><DescriptionPane elementId={local.id} fallback={null} /></div>}</>;
}
