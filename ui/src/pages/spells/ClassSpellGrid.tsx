import type { KeyboardEvent } from "react";
import type { ClassSpellcasting, SpellEntry } from "../../api/contract";
import { Button } from "../../components";
import { useCharacter } from "../../state/CharacterContext";
import { setStatus } from "../../state/status";
import { Glyph } from "../equipment/Glyph";
import { arrowNav } from "./rowNav";
import { ComponentCells, Mark, levelText, useSourceText } from "./SpellGrid";
import "../equipment/equipment.css";

const isLocked = (s: SpellEntry) => s.level === 0 || s.alwaysPrepared;
const isKnown = (s: SpellEntry) => isLocked(s) || s.prepared;

/** Info box for the highlighted spell: cantrips and granted spells cannot be prepared. */
function InfoBox({ spell }: { spell: SpellEntry | undefined }) {
  if (!spell || !isLocked(spell)) return null;
  const text = spell.level === 0
    ? `${spell.name} (cantrip) is always ready to cast and doesn't need to be prepared.`
    : `${spell.name} is always prepared for you.`;
  return <p className="sp-info" role="status">{text}</p>;
}

/**
 * Per-class tab: the class's spells with the prepared marker, the hint, Prepare/Unprepare and Refresh.
 * Double-click, Space or the button toggles the highlighted spell. The allowed count comes from the engine snapshot.
 */
export function ClassSpellGrid({ cls, picked, onPick }: { cls: ClassSpellcasting; picked: string | null; onPick: (id: string) => void }) {
  const { mutate, busy } = useCharacter();
  const sourceText = useSourceText();
  const canPrepare = cls.preparedMax > 0;
  const current = cls.spells.find((s) => s.id === picked);

  const toggle = async (s: SpellEntry) => {
    onPick(s.id);
    if (!canPrepare || isLocked(s)) return;
    const prepared = !s.prepared;
    const next = await mutate((api, id) => api.setSpellPrepared({ characterId: id, classSlotId: cls.classSlotId, spellId: s.id, prepared }));
    if (next) setStatus(`${cls.className}: ${s.name} is ${prepared ? "prepared" : "unprepared"}.`);
  };
  const refresh = () => void mutate((api, id) => api.getSnapshot({ characterId: id }));
  const onKey = (e: KeyboardEvent<HTMLTableRowElement>, s: SpellEntry) => {
    if (e.key === " ") { e.preventDefault(); void toggle(s); return; }
    arrowNav(e, onPick);
  };

  return (
    <>
      {canPrepare && (
        <div className="sp-prep-bar">
          <span className="sp-hint">Double click a spell to prepare or unprepare it.</span>
          <Button small disabled={busy || !current || isLocked(current)} onClick={() => current && void toggle(current)}>
            {current && !isLocked(current) && current.prepared ? "Unprepare" : "Prepare"}
          </Button>
          <Button small disabled={busy} onClick={refresh}>Refresh</Button>
        </div>
      )}
      <InfoBox spell={current} />
      <div className="ap-scroll sp-main">
        <table className="ap-grid eq-grid sp-grid" aria-label={`${cls.className} spells`}>
          <thead>
            <tr>
              <th scope="col" className="sp-ck"><Glyph name="check" label="Prepared" /></th>
              <th scope="col" className="sp-name">Name</th>
              <th scope="col" className="sp-lvl">Level &amp; school</th>
              <th scope="col" className="sp-vsm al-c">Verbal</th>
              <th scope="col" className="sp-vsm al-c">Somatic</th>
              <th scope="col" className="sp-vsm al-c">Material</th>
              <th scope="col" className="sp-src">Source</th>
            </tr>
          </thead>
          <tbody>
            {cls.spells.length === 0 && <tr className="ap-empty-row"><td colSpan={7}>No spells to show.</td></tr>}
            {cls.spells.map((s) => (
              <tr key={`${s.id}:${s.slotId ?? ""}`} data-spell-id={s.id} className={picked === s.id ? "sel" : ""} onMouseDown={() => onPick(s.id)}
                onDoubleClick={() => void toggle(s)} onKeyDown={(e) => onKey(e, s)}>
                <td className="sp-ck"><Mark on={isKnown(s)} label={s.prepared ? "Prepared" : "Always prepared"} /></td>
                <td className="sp-name"><button type="button" className="ap-name" onClick={() => onPick(s.id)}>{s.name}</button></td>
                <td className="sp-lvl">{levelText(s)}</td>
                <ComponentCells casting={s.casting} />
                <td className="sp-src">{sourceText(s)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}
