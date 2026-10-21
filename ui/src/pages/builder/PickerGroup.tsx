import { useMemo, type CSSProperties, type KeyboardEvent } from "react";
import { useNavigate } from "react-router-dom";
import type { ElementRef, SelectionSlot, SlotOption } from "../../api/contract";
import { Expander, FocusBar, Icon } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useAsync } from "../../state/useAsync";
import { useSettings } from "../../state/SettingsContext";
import type { RowHeight } from "../../api/contract";

const SOURCES_ROUTE = "/content/sources";

/** Classic builder picker row heights (px) for the ROW HEIGHT setting: small / medium (default) / large. */
export const PICKER_ROW_PX: Record<RowHeight, number> = { small: 17, medium: 21, large: 25 };

export interface SlotGroup { key: string; label: string | null; title: string; slots: SelectionSlot[] }

/** One expander per pick: a "2 of 3" rule is three separate pickers, each with its own footer line. */
export function groupSlots(slots: SelectionSlot[]): SlotGroup[] {
  // E31 SO-3: a locked slot (sole defaulted option, held) cannot change or clear and Aurora does not show it.
  return slots.filter((s) => !s.locked).map((s) => ({ key: s.id, label: s.groupLabel, title: s.title, slots: [s] }));
}

/**
 * List-rule slots (e.g. "Ability Scores (Sage)") offer text entries whose ids are list item numbers, not element ids:
 * they have no element to describe or look up (getElement would answer "That element does not exist.").
 */
export const isListSlot = (slot: Pick<SelectionSlot, "elementType">): boolean => slot.elementType === "list";

/** Describe callback: `isElement` is false for list entries, which highlight but have no element description. */
export type DescribeFn = (id: string, isElement?: boolean) => void;

/** Plain-text source of an element (footer lines and rows for choices the option list no longer carries). */
function ElementSource({ id }: { id: string }) {
  const api = useApi();
  const el = useAsync(() => api.getElement({ elementId: id }), [api, id]);
  return <>{el.data?.sourceName ?? ""}</>;
}

type Row = Pick<SlotOption, "id" | "name"> & Partial<SlotOption>;

/** Merges chosen elements the option list omits back into it, at their alphabetical place, so rows never jump. */
export function mergeRows(items: SlotOption[], chosen: ElementRef[]): Row[] {
  const out: Row[] = [...items];
  for (const m of chosen) {
    if (out.some((i) => i.id === m.id)) continue;
    const at = out.findIndex((i) => i.name.localeCompare(m.name) > 0);
    out.splice(at < 0 ? out.length : at, 0, { id: m.id, name: m.name });
  }
  return out;
}

interface Props {
  characterId: string;
  group: SlotGroup;
  isNext: boolean;
  open: boolean;
  onToggle: (open: boolean) => void;
  describedId: string | null;
  onDescribe: DescribeFn;
  onChoose: (slotId: string, elementId: string) => void;
  onClear: (slotId: string) => void;
}

/** One classic builder selection picker: focus bar + expander (plain list of options) + chosen/remaining footer. */
export function PickerGroup({ characterId, group, isNext, open, onToggle, describedId, onDescribe, onChoose, onClear }: Props) {
  const api = useApi();
  const navigate = useNavigate();
  const { settings } = useSettings();
  const rowPx = PICKER_ROW_PX[settings.rowHeight] ?? PICKER_ROW_PX.medium;
  const { slots } = group;
  const slot = slots[0]!;
  const allLocked = slot.locked;
  const chosenSig = slot.chosen?.id ?? "";
  const options = useAsync(
    () => (open && !allLocked ? api.getSlotOptions({ characterId, slotId: slot.id, query: "" }) : Promise.resolve(null)),
    [api, characterId, slot.id, open, allLocked, chosenSig],
  );
  const items = useMemo<SlotOption[]>(() => [...(options.data?.items ?? [])].sort((a, b) => a.name.localeCompare(b.name)), [options.data]);
  const rows = useMemo(() => mergeRows(items, slot.chosen ? [slot.chosen] : []), [items, slot.chosen]);
  const showDescription = slot.elementType === "class";
  const title = group.title.toUpperCase();

  const onRowKey = (e: KeyboardEvent<HTMLButtonElement>, o: Row, disabled: boolean) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (!disabled && slot.chosen?.id !== o.id) onChoose(slot.id, o.id);
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const li = (e.currentTarget.closest("li") as HTMLElement | null);
    const sib = (e.key === "ArrowDown" ? li?.nextElementSibling : li?.previousElementSibling) as HTMLElement | null;
    const btn = sib?.querySelector<HTMLButtonElement>("button.pick-main");
    if (btn) { btn.focus(); btn.click(); }
  };

  return (
    <div className={`pick ${isNext ? "is-next" : ""} ${allLocked ? "is-locked" : ""} ${open && !allLocked ? "is-open" : ""}`} data-slot-id={slot.id}
      style={{ "--pick-row": `${rowPx}px` } as CSSProperties}>
      <FocusBar>
        <Expander title={<>{title}{slot.requiredLevel ? <span className="pick-count">{` - LEVEL ${slot.requiredLevel}`}</span> : null}</>}
          open={open} onToggle={onToggle}
          headerExtra={!allLocked && open ? <button type="button" className="pick-tag" aria-label="Manage Sources" title="Manage Sources" onClick={() => navigate(SOURCES_ROUTE)}><Icon name="tags" /></button> : undefined}>
          {slot.hint && <p className="pick-hint">{slot.hint}</p>}
          {options.error && <p className="pick-empty danger-text" role="alert">{options.error}</p>}
          {allLocked && <p className="pick-empty">Granted by your build; nothing to choose.</p>}
          {options.data && rows.length === 0 && <p className="pick-empty">Nothing matches. Check your source restrictions.</p>}
          <ul className={`pick-list ${showDescription ? "with-desc" : ""}`} aria-label={`${group.title} options`}>
            {rows.map((o) => {
              const sel = slot.chosen?.id === o.id;
              const hl = describedId === o.id;
              const disabled = !!o.disabledReason;
              return (
                <li key={o.id} className={`pick-row ${sel ? "selected" : ""} ${hl ? "hl" : ""}`}>
                  <button type="button" className="pick-main" aria-pressed={sel} aria-disabled={disabled || undefined} title={o.disabledReason}
                    aria-label={[o.name, o.level !== undefined ? (o.level === 0 ? "Cantrip" : `Level ${o.level}`) : "", o.sourceName ?? ""].filter(Boolean).join(", ")}
                    onClick={() => onDescribe(o.id, !isListSlot(slot))} onKeyDown={(e) => onRowKey(e, o, disabled)}
                    onDoubleClick={() => { if (!sel && !disabled) onChoose(slot.id, o.id); }}>
                    <span className={`pick-name ${disabled ? "disabled" : ""}`} aria-hidden="true">
                      {o.name}{o.level !== undefined && <span className="pick-sub"> {o.level === 0 ? "Cantrip" : `Level ${o.level}`}</span>}
                      {o.prerequisites && <span className="pick-sub"> - Requires {o.prerequisites}</span>}
                    </span>
                    {showDescription && <span className="pick-desc" aria-hidden="true">{o.shortDescription}</span>}
                    <span className="pick-src" aria-hidden="true">{o.sourceName ?? (isListSlot(slot) ? null : <ElementSource id={o.id} />)}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </Expander>
        {slot.chosen && (
          <div className="pick-footer">
            <div className="pick-chosen" data-slot-id={slot.id}>
              <Icon name={slot.locked ? "lock" : "check-box"} />
              <span className="pick-chosen-name">{slot.chosen.name}</span>
              <span className="pick-chosen-src">{isListSlot(slot) ? null : <ElementSource id={slot.chosen.id} />}</span>
              {!slot.locked && <button type="button" className="pick-x" aria-label={`Clear ${slot.chosen.name}`} onClick={() => onClear(slot.id)}><Icon name="x" /></button>}
            </div>
          </div>
        )}
      </FocusBar>
    </div>
  );
}
