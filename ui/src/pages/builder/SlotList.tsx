import { useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useSearchParams } from "react-router";
import type { SelectionSlot } from "../../api/contract";
import { haptic } from "../../lib/haptics";
import { revealAtTop } from "../../lib/scrollReveal";
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
  const ruleset = snapshot?.ruleset;
  const groups = useMemo(() => groupSlots(slots, ruleset), [slots, ruleset]);
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
  // A11: each Next selection (a navigation naming a slot) scrolls its picker to the top of the page once it is drawn.
  const { key: navKey } = useLocation();
  const stackRef = useRef<HTMLDivElement>(null);
  const revealed = useRef<string | null>(null);
  useEffect(() => {
    if (!wanted || revealed.current === navKey) return;
    const g = groups.find((x) => x.slots.some((s) => s.id === wanted));
    const head = g && [...(stackRef.current?.querySelectorAll<HTMLElement>(".pick[data-slot-id]") ?? [])].find((el) => el.getAttribute("data-slot-id") === g.slots[0]!.id);
    if (!head) return;
    const frame = requestAnimationFrame(() => { revealed.current = navKey; revealAtTop(head); });
    return () => cancelAnimationFrame(frame);
  }, [wanted, navKey, groups]);
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
    haptic("choose");
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
    <div className="pick-stack" ref={stackRef}>
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
