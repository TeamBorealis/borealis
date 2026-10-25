import { type KeyboardEvent, useMemo, useState } from "react";
import type { ClassSpellcasting, SelectionSlot, SlotOption } from "../../api/contract";
import { ConvertedTag, EditionBadge, editionMarks, EditionNotes } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useAsync } from "../../state/useAsync";
import { Glyph } from "../equipment/Glyph";
import { mergeRows } from "../builder/PickerGroup";
import { arrowNav } from "./rowNav";
import { ComponentCells, componentText } from "./SpellGrid";
import { RowDetail } from "../../components/RowDetail";
import "../equipment/equipment.css";

const levelText = (o: Partial<SlotOption>) => (o.level === undefined ? "" : `${o.level === 0 ? "Cantrip" : `Level ${o.level}`}${o.school ? ` · ${o.school}` : ""}`);

/**
 * The SPELLCASTING catalogue while a selection slot is highlighted: the slot's own options (same source as the Build pickers).
 * Click describes; double-click or Enter registers the spell into the highlighted slot.
 */
export function SlotOptionsGrid({ characterId, cls, slot, picked, onPick, onChoose }: {
  characterId: string; cls: ClassSpellcasting; slot: SelectionSlot; picked: string | null;
  onPick: (id: string) => void; onChoose: (slotId: string, elementId: string) => void;
}) {
  const api = useApi();
  const chosenId = slot.chosen?.id ?? "";
  const options = useAsync(
    () => (slot.locked ? Promise.resolve(null) : api.getSlotOptions({ characterId, slotId: slot.id, query: "" })),
    [api, characterId, slot.id, slot.locked, chosenId],
  );
  const rows = useMemo(() => {
    const items = [...(options.data?.items ?? [])].sort((a, b) => a.name.localeCompare(b.name));
    return mergeRows(items, slot.chosen ? [slot.chosen] : []);
  }, [options.data, slot.chosen]);

  const [notesFor, setNotesFor] = useState<string | null>(null);

  const choose = (o: (typeof rows)[number]) => {
    if (o.disabledReason || slot.chosen?.id === o.id) return;
    onChoose(slot.id, o.id);
  };
  const onKey = (e: KeyboardEvent<HTMLButtonElement>, o: (typeof rows)[number]) => {
    if (e.key !== "Enter") return;
    e.preventDefault();
    choose(o);
  };

  return (
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
          </tr>
        </thead>
        <tbody>
          {options.error && <tr className="ap-empty-row"><td colSpan={7} role="alert">{options.error}</td></tr>}
          {!options.error && rows.length === 0 && <tr className="ap-empty-row"><td colSpan={7}>{options.loading ? "Loading..." : "No spells to show."}</td></tr>}
          {rows.map((o) => {
            const sel = slot.chosen?.id === o.id;
            const marks = editionMarks(o);
            const notesId = `sp-notes-${slot.id}-${o.id}`;
            const notesOpen = notesFor === o.id;
            return (
              <tr key={o.id} data-spell-id={o.id} className={picked === o.id ? "sel" : ""} onMouseDown={() => onPick(o.id)} onDoubleClick={() => choose(o)} onKeyDown={(e) => arrowNav(e, onPick)}>
                <td className="sp-ck">{sel && <span className="sp-mark"><Glyph name="tick" label="Registered" /></span>}</td>
                <td className="sp-name">
                  <button type="button" className="ap-name" aria-disabled={!!o.disabledReason || undefined} title={o.disabledReason}
                    onClick={() => onPick(o.id)} onKeyDown={(e) => onKey(e, o)}>{o.name}</button>
                  <RowDetail parts={[levelText(o), componentText(o.spell ?? null)]} />
                </td>
                <td className="sp-lvl">{levelText(o)}</td>
                <ComponentCells casting={o.spell ?? null} />
                <td className="sp-src">
                  <span className="sp-src-line">
                    <span className="sp-src-name">{o.sourceName ?? ""}</span>
                    <EditionBadge marks={marks} />
                    <ConvertedTag marks={marks} open={notesOpen} notesId={notesId} onToggle={() => setNotesFor(notesOpen ? null : o.id)} />
                  </span>
                  <EditionNotes marks={marks} id={notesId} open={notesOpen} />
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
