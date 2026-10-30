import { useState, type CSSProperties, type KeyboardEvent } from "react";
import { useNavigate } from "react-router";
import type { RowHeight, SelectionSlot } from "../../api/contract";
import { Expander, FocusBar, Icon } from "../../components";
import { useSettings } from "../../state/SettingsContext";
import { usePhone } from "../../lib/phone";
import { NO_FILTER, type ShownRow, type VersionLike } from "../../lib/versions";
import { PickerFilterBar } from "./PickerFilterBar";
import { ElementSource, PickerRow } from "./PickerRow";
import { usePickerRows, useSwitchVersion } from "./usePickerRows";
import { VersionMenu } from "./VersionMenu";
import { EmptyListNote } from "./EmptyListNote";
import { useStartOver } from "./useStartOver";
import { useCharacter } from "../../state/CharacterContext";
import { openEditionFlyout } from "../manage/EditionFlyout";

const SOURCES_ROUTE = "/content/sources";

const SKELETON_ROWS = ["skel-1", "skel-2", "skel-3"];

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

/** A row that can still be chosen: phones show "Choose <name>" for it under the list (B-48 O2). */
export interface PickChoice { slotId: string; name: string; choose: () => void }

/**
 * Describe callback: `isElement` is false for list entries, which highlight but have no element description. `choice` is
 * set when the described row can be chosen (not already chosen, not disabled).
 */
export type DescribeFn = (id: string, isElement?: boolean, choice?: PickChoice) => void;

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

/** Up/Down on a row moves to the next or previous row and describes it. */
function onKeyNav(e: KeyboardEvent<HTMLButtonElement>) {
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
  e.preventDefault();
  const li = e.currentTarget.closest("li") as HTMLElement | null;
  const sib = (e.key === "ArrowDown" ? li?.nextElementSibling : li?.previousElementSibling) as HTMLElement | null;
  const btn = sib?.querySelector<HTMLButtonElement>("button.pick-main");
  if (btn) { btn.focus(); btn.click(); }
}

/**
 * One classic builder selection picker: focus bar + expander (filter bar, one row per name) + chosen/remaining footer.
 * B-55: rows group same-name versions; the version column's menu picks the shown version (and switches a chosen row).
 */
export function PickerGroup({ characterId, group, isNext, open, onToggle, describedId, onDescribe, onChoose, onClear }: Props) {
  const navigate = useNavigate();
  const { settings } = useSettings();
  const rowPx = PICKER_ROW_PX[settings.rowHeight] ?? PICKER_ROW_PX.medium;
  const slot = group.slots[0]!;
  const allLocked = slot.locked;
  const list = isListSlot(slot);
  const picker = usePickerRows(characterId, slot, open && !allLocked);
  const switchVersion = useSwitchVersion();
  const ruleset = useCharacter().snapshot?.ruleset ?? "dnd2024";
  const startOver = useStartOver();
  const { options, rows, visible } = picker;
  const showDescription = slot.elementType === "class";
  // Phones read in sentence case (B-48, spec sec. 3.3: same words, only the casing changes); desktop keeps Aurora's capitals.
  const phone = usePhone();
  const title = phone ? group.title : group.title.toUpperCase();
  const [notesFor, setNotesFor] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ key: string; anchor: HTMLElement } | null>(null);
  const menuRow = menu ? visible.find((r) => r.key === menu.key) ?? null : null;

  const choiceOf = (row: ShownRow, v: VersionLike): PickChoice | undefined =>
    row.chosen || v.disabledReason ? undefined : { slotId: slot.id, name: row.name, choose: () => onChoose(slot.id, v.id) };
  const describe = (row: ShownRow, v: VersionLike = row.shown) => onDescribe(v.id, !list, choiceOf(row, v));
  /** Sec. 1.4: the picked version becomes the row's shown version; on the chosen row it also switches the choice. */
  const pickVersion = (row: ShownRow, v: VersionLike) => {
    menu?.anchor.focus();
    setMenu(null);
    picker.remember(row.key, v.id);
    describe(row, v);
    if (row.chosen && row.chosen.id !== v.id) void switchVersion(slot.id, row.name, v);
  };

  return (
    <div className={`pick ${isNext ? "is-next" : ""} ${allLocked ? "is-locked" : ""} ${open && !allLocked ? "is-open" : ""}`} data-slot-id={slot.id}
      style={{ "--pick-row": `${rowPx}px` } as CSSProperties}>
      <FocusBar>
        <Expander title={<>{title}{slot.requiredLevel ? <span className="pick-count">{phone ? ` - Level ${slot.requiredLevel}` : ` - LEVEL ${slot.requiredLevel}`}</span> : null}</>}
          open={open} onToggle={onToggle}
          headerExtra={!allLocked && open ? <button type="button" className="pick-tag" aria-label="Manage Sources" title="Manage Sources" onClick={() => navigate(SOURCES_ROUTE)}><Icon name="tags" /></button> : undefined}>
          {slot.hint && <p className="pick-hint">{slot.hint}</p>}
          {options.error && <p className="pick-empty danger-text" role="alert">{options.error}</p>}
          {allLocked && <p className="pick-empty">Granted by your build; nothing to choose.</p>}
          {options.data && rows.length === 0 && (
            <EmptyListNote slot={slot} ruleset={ruleset} onManageSources={() => navigate(SOURCES_ROUTE)} onStartOver={(ed) => void startOver(ed)} onEditionOptions={openEditionFlyout} />
          )}
          {picker.showBar && (
            <PickerFilterBar title={group.title} filter={picker.filter} onChange={picker.setFilter} sources={picker.sources} shown={visible.length} total={rows.length} />
          )}
          {rows.length > 0 && visible.length === 0 && (
            <p className="pick-empty pick-fempty">Nothing matches these filters. <button type="button" className="pick-fclear" onClick={() => picker.setFilter(NO_FILTER)}>Clear</button></p>
          )}
          <ul className={`pick-list ${showDescription ? "with-desc" : ""}`} aria-label={`${group.title} options`}>
            {/* Phones: skeleton rows in the list's own shape while the options load (spec sec. 3.4 Loading). */}
            {phone && !allLocked && options.loading && !options.data && SKELETON_ROWS.map((k) => <li key={k} className="pick-skel" aria-hidden="true" />)}
            {visible.map((row) => (
              <PickerRow key={row.key} row={row} listSlot={list} showDescription={showDescription} versionColumn={picker.versionColumn}
                highlighted={row.versions.some((v) => v.id === describedId)} menuOpen={menu?.key === row.key}
                notesOpen={notesFor === row.key} notesId={`pick-notes-${slot.id}-${row.shown.id}`}
                onToggleNotes={() => setNotesFor(notesFor === row.key ? null : row.key)}
                onDescribe={() => describe(row)} onChoose={() => onChoose(slot.id, row.shown.id)}
                onOpenMenu={(anchor) => setMenu(menu?.key === row.key ? null : { key: row.key, anchor })} onKeyNav={onKeyNav} />
            ))}
          </ul>
        </Expander>
        {menu && menuRow && (
          <VersionMenu anchor={menu.anchor} rowName={menuRow.name} versions={menuRow.versions} charEd={picker.charEd} shownId={menuRow.shown.id}
            chosenId={menuRow.chosen?.id ?? null} onPick={(v) => pickVersion(menuRow, v)}
            onClose={(restore) => { setMenu(null); if (restore) menu.anchor.focus(); }} />
        )}
        {slot.chosen && (
          <div className="pick-footer">
            <div className="pick-chosen" data-slot-id={slot.id}>
              <Icon name={slot.locked ? "lock" : "check-box"} />
              <span className="pick-chosen-name">{slot.chosen.name}</span>
              <span className="pick-chosen-src">{list ? null : <ElementSource id={slot.chosen.id} />}</span>
              {!slot.locked && <button type="button" className="pick-x" aria-label={`Clear ${slot.chosen.name}`} onClick={() => onClear(slot.id)}><Icon name="x" /></button>}
            </div>
          </div>
        )}
      </FocusBar>
    </div>
  );
}
