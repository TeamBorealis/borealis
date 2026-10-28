import { type KeyboardEvent, useState } from "react";
import type { ClassSpellcasting, SelectionSlot } from "../../api/contract";
import { ConvertedTag, editionMarks, EditionNotes, EditionSuffix } from "../../components";
import { NO_FILTER, type ShownRow, type VersionLike } from "../../lib/versions";
import { Glyph } from "../equipment/Glyph";
import { PickerFilterBar } from "../builder/PickerFilterBar";
import { usePickerRows, useSwitchVersion } from "../builder/usePickerRows";
import { VersionButton, VersionMenu } from "../builder/VersionMenu";
import { arrowNav } from "./rowNav";
import { ComponentCells, componentText } from "./SpellGrid";
import { RowDetail } from "../../components/RowDetail";
import "../equipment/equipment.css";

const levelText = (o: VersionLike) => (o.level === undefined ? "" : `${o.level === 0 ? "Cantrip" : `Level ${o.level}`}${o.school ? ` · ${o.school}` : ""}`);

const COLUMNS = 7;

/**
 * The SPELLCASTING catalogue while a selection slot is highlighted: the slot's own options (same source as the Build pickers),
 * one row per name (B-55) with the version column and the filter bar. Click describes; double-click or Enter registers the
 * row's shown version into the highlighted slot.
 */
export function SlotOptionsGrid({ characterId, cls, slot, picked, onPick, onChoose }: {
  characterId: string; cls: ClassSpellcasting; slot: SelectionSlot; picked: string | null;
  onPick: (id: string) => void; onChoose: (slotId: string, elementId: string) => void;
}) {
  const picker = usePickerRows(characterId, slot, !slot.locked);
  const switchVersion = useSwitchVersion();
  const { options, rows, visible } = picker;
  const [notesFor, setNotesFor] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ key: string; anchor: HTMLElement } | null>(null);
  const menuRow = menu ? visible.find((r) => r.key === menu.key) ?? null : null;
  const cols = COLUMNS + (picker.versionColumn ? 1 : 0);

  const choose = (row: ShownRow) => {
    if (row.shown.disabledReason || row.chosen) return;
    onChoose(slot.id, row.shown.id);
  };
  const onKey = (e: KeyboardEvent<HTMLButtonElement>, row: ShownRow) => {
    if (e.key !== "Enter") return;
    e.preventDefault();
    choose(row);
  };
  const pickVersion = (row: ShownRow, v: VersionLike) => {
    menu?.anchor.focus();
    setMenu(null);
    picker.remember(row.key, v.id);
    onPick(v.id);
    if (row.chosen && row.chosen.id !== v.id) void switchVersion(slot.id, row.name, v);
  };

  return (
    <>
      {picker.showBar && (
        <PickerFilterBar title="spells" filter={picker.filter} onChange={picker.setFilter} sources={picker.sources} shown={visible.length} total={rows.length} />
      )}
      <div className="ap-scroll sp-main">
        <table className="ap-grid eq-grid sp-grid" aria-label={`${cls.className} spells`} data-slot-id={slot.id}>
          <thead>
            <tr>
              <th scope="col" className="sp-ck"><Glyph name="check" label="Registered" /></th>
              <th scope="col" className="sp-name">Name</th>
              <th scope="col" className="sp-lvl">Level &amp; school</th>
              <th scope="col" className="sp-vsm al-c">Verbal</th>
              <th scope="col" className="sp-vsm al-c">Somatic</th>
              <th scope="col" className="sp-vsm al-c">Material</th>
              <th scope="col" className="sp-src">Source</th>
              {picker.versionColumn && <th scope="col" className="sp-ver"><span className="sr-only">Versions</span></th>}
            </tr>
          </thead>
          <tbody>
            {options.error && <tr className="ap-empty-row"><td colSpan={cols} role="alert">{options.error}</td></tr>}
            {!options.error && rows.length === 0 && <tr className="ap-empty-row"><td colSpan={cols}>{options.loading ? "Loading..." : "No spells to show."}</td></tr>}
            {rows.length > 0 && visible.length === 0 && (
              <tr className="ap-empty-row"><td colSpan={cols}>Nothing matches these filters. <button type="button" className="pick-fclear" onClick={() => picker.setFilter(NO_FILTER)}>Clear</button></td></tr>
            )}
            {visible.map((row) => {
              const o = row.shown;
              const marks = editionMarks(o);
              const notesId = `sp-notes-${slot.id}-${o.id}`;
              const notesOpen = notesFor === row.key;
              const multi = row.versions.length > 1;
              const hl = row.versions.some((v) => v.id === picked);
              return (
                <tr key={row.key} data-spell-id={o.id} className={hl ? "sel" : ""} onMouseDown={() => onPick(o.id)} onDoubleClick={() => choose(row)} onKeyDown={(e) => arrowNav(e, onPick)}>
                  <td className="sp-ck">{row.chosen && <span className="sp-mark"><Glyph name="tick" label="Registered" /></span>}</td>
                  <td className="sp-name">
                    <button type="button" className="ap-name" aria-disabled={!!o.disabledReason || undefined} title={o.disabledReason}
                      onClick={() => onPick(o.id)} onKeyDown={(e) => onKey(e, row)}>{row.name}</button>
                    <RowDetail parts={[levelText(o), componentText(o.spell ?? null)]} />
                  </td>
                  <td className="sp-lvl">{levelText(o)}</td>
                  <ComponentCells casting={o.spell ?? null} />
                  <td className="sp-src">
                    <span className="sp-src-line">
                      <span className="sp-src-name">{o.sourceName ?? ""}<EditionSuffix marks={marks} sourceName={o.sourceName} /></span>
                      <ConvertedTag marks={marks} open={notesOpen} notesId={notesId} onToggle={() => setNotesFor(notesOpen ? null : row.key)} />
                    </span>
                    <EditionNotes marks={marks} id={notesId} open={notesOpen} />
                  </td>
                  {picker.versionColumn && (
                    <td className="sp-ver" onMouseDown={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()}>
                      {multi && <VersionButton count={row.versions.length} rowName={row.name} open={menu?.key === row.key}
                        onOpen={(anchor) => setMenu(menu?.key === row.key ? null : { key: row.key, anchor })} />}
                    </td>
                  )}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {menu && menuRow && (
        <VersionMenu anchor={menu.anchor} rowName={menuRow.name} versions={menuRow.versions} charEd={picker.charEd} shownId={menuRow.shown.id}
          chosenId={menuRow.chosen?.id ?? null} onPick={(v) => pickVersion(menuRow, v)}
          onClose={(restore) => { setMenu(null); if (restore) menu.anchor.focus(); }} />
      )}
    </>
  );
}
