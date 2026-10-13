import type { ClassSpellcasting, SelectionSlot } from "../../api/contract";
import { Button } from "../../components";
import { setStatus } from "../../state/status";
import { useCharacter } from "../../state/CharacterContext";
import { Glyph } from "../equipment/Glyph";

/**
 * Spellcasting class that learned the slot's spell (for the source column and the status line).
 * Slot ids are opaque (`owner:rule:ordinal`): the class link is the slot's `classSlotId`, never an id prefix.
 */
export const sourceOf = (s: SelectionSlot, classes: ClassSpellcasting[]): string =>
  s.groupLabel
  ?? classes.find((c) => c.classSlotId === s.classSlotId)?.className
  ?? classes.find((c) => c.spells.some((x) => x.slotId === s.id))?.className ?? "";

/**
 * Class block whose catalogue lists a highlighted slot's options: the slot's own class, else (feat or species spells,
 * e.g. Magic Initiate) the class that registered its pick, else the first spellcasting class.
 */
export const catalogueClassOf = (s: SelectionSlot, classes: ClassSpellcasting[]): string | undefined =>
  (classes.find((c) => c.classSlotId === s.classSlotId)
    ?? classes.find((c) => c.spells.some((x) => x.slotId === s.id))
    ?? classes[0])?.classSlotId;

function ReplacedCell({ slot }: { slot: SelectionSlot }) {
  const last = slot.replaced.at(-1);
  if (!last) return <td className="al-c" />;
  const history = slot.replaced.map((r) => `${r.name} (replaced at level ${r.level})`).join("\n");
  return <td className="al-c"><span className="sp-replaced" title={history}>{last.name}</span></td>;
}

/**
 * Selection-slots table of the SPELLCASTING tab: chosen marker, slot name, selected spell, gained on level, replaced, source.
 * Highlighting a filled slot whose rule allows swapping shows "Replace spell": the old pick is cleared and recorded as replaced,
 * and the slot reopens for a new pick at the current level.
 */
export function SlotsTable({ slots, classes, onPick, picked, current, onCurrent }: {
  slots: SelectionSlot[]; classes: ClassSpellcasting[]; onPick: (spellId: string) => void; picked: string | null;
  current: string | null; onCurrent: (slotId: string) => void;
}) {
  const { mutate, busy } = useCharacter();
  const available = slots.filter((s) => !s.chosen).length;
  const highlighted = slots.find((s) => s.id === current) ?? null;

  const replace = async (slot: SelectionSlot) => {
    const old = slot.chosen;
    if (!old) return;
    const source = sourceOf(slot, classes);
    const next = await mutate((api, id) => api.replaceSpell({ characterId: id, slotId: slot.id }));
    if (next) setStatus(`${source ? `${source}: ` : ""}${old.name} is replaced. Choose one from the '${slot.title}' selection option.`);
  };

  return (
    <section className="sp-slots-sec" aria-label="Spell selection slots">
      <div className="ap-band">
        <h2>Selection slots</h2>
        <span className="ap-stats"><span>Available:<b>{available} / {slots.length}</b></span></span>
        {highlighted?.allowReplace && highlighted.chosen && (
          <Button small className="sp-replace" disabled={busy} onClick={() => void replace(highlighted)}>Replace spell</Button>
        )}
      </div>
      <div className="ap-scroll sp-slots-scroll">
        <table className="ap-grid eq-grid sp-grid" aria-label="Selection slots">
          <thead>
            <tr>
              <th scope="col" className="sp-ck"><Glyph name="check" label="Chosen" /></th>
              <th scope="col">Name</th><th scope="col">Selected spell</th><th scope="col" className="al-c">Gained on level</th>
              <th scope="col" className="al-c">Replaced</th><th scope="col" className="grow">Spellcasting source</th>
            </tr>
          </thead>
          <tbody>
            {slots.length === 0 && <tr className="ap-empty-row"><td colSpan={6}>No spells to learn at the current level.</td></tr>}
            {slots.map((s) => (
              <tr key={s.id} aria-selected={s.id === current} onClick={() => onCurrent(s.id)}
                className={s.id === current || (s.chosen && picked === s.chosen.id) ? "sel" : ""}>
                <td className="sp-ck">{s.chosen && <span className="sp-mark"><Glyph name="tick" label="Chosen" /></span>}</td>
                <td>{s.title}{s.of > 1 ? ` (${s.ordinal} of ${s.of})` : ""}</td>
                <td>{s.chosen ? <button type="button" className="ap-name" onClick={() => onPick(s.chosen!.id)}>{s.chosen.name}</button> : <span className="sp-open">not selected{s.required ? "" : " (optional)"}</span>}</td>
                <td className="al-c">{s.gainedOnLevel}</td>
                <ReplacedCell slot={s} />
                <td className="grow">{sourceOf(s, classes)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
