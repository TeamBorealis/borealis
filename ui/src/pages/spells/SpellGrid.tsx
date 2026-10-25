import type { ClassSpellcasting, SpellEntry, SpellFacts } from "../../api/contract";
import { RowDetail } from "../../components/RowDetail";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useAsync } from "../../state/useAsync";
import { Glyph } from "../equipment/Glyph";
import { arrowNav } from "./rowNav";
import "../equipment/equipment.css";

const ORIGIN_SOURCE: Record<Exclude<SpellEntry["origin"], "race">, string> = { class: "", feat: "Feat", item: "Item" };
export const levelText = (s: SpellEntry) => `${s.level === 0 ? "Cantrip" : `Level ${s.level}`} · ${s.school}`;

/** Source cell text; a racial spell is labelled with the ruleset's own word for race (2014 "Race", 2024 "Species"). */
export function useSourceText(): (s: SpellEntry) => string {
  const api = useApi();
  const { snapshot } = useCharacter();
  const ruleset = snapshot?.ruleset;
  const profile = useAsync(() => (ruleset ? api.getRulesetProfile({ ruleset }) : Promise.resolve(null)), [api, ruleset]);
  const race = profile.data?.terminology.species ?? "Race";
  return (s) => s.sourceName || (s.origin === "race" ? race : ORIGIN_SOURCE[s.origin]);
}
const isKnown = (s: SpellEntry) => s.level === 0 || s.alwaysPrepared || s.prepared;

export function Mark({ on, label, title }: { on: boolean; label: string; title?: string }) {
  return on ? <span className="sp-mark" title={title} aria-label={title ?? label} role="img"><Glyph name="tick" /></span> : null;
}

/** The spell's components as the rules abbreviate them ("V, S, M"); the phone row detail (O14) shows them. */
export const componentText = (c: SpellFacts | null): string => [c?.verbal && "V", c?.somatic && "S", c?.material && "M"].filter(Boolean).join(", ");

/** Component cells (verbal, somatic, material) of a spell; blank when the spell has no content element. */
export function ComponentCells({ casting }: { casting: SpellFacts | null }) {
  const mat = casting?.materialText ? `Material: ${casting.materialText}` : "Material";
  return (
    <>
      <td className="al-c"><Mark on={!!casting?.verbal} label="Verbal" /></td>
      <td className="al-c"><Mark on={!!casting?.somatic} label="Somatic" /></td>
      <td className="al-c"><Mark on={!!casting?.material} label="Material" title={mat} /></td>
    </>
  );
}

/** Classic builder spell DataGrid: registered check, NAME, LEVEL & SCHOOL, VERBAL, SOMATIC, MATERIAL, SOURCE. */
export function SpellGrid({ cls, picked, onPick }: { cls: ClassSpellcasting; picked: string | null; onPick: (id: string) => void }) {
  const { mutate } = useCharacter();
  const sourceText = useSourceText();
  return (
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
          {cls.spells.map((s) => {
            const locked = s.level === 0 || s.alwaysPrepared;
            return (
              <tr key={`${s.id}:${s.slotId ?? ""}`} data-spell-id={s.id} className={picked === s.id ? "sel" : ""} onMouseDown={() => onPick(s.id)} onKeyDown={(e) => arrowNav(e, onPick)}>
                <td className="sp-ck">
                  <input type="checkbox" className="ap-check" aria-label={`Prepare ${s.name}`} checked={locked ? true : s.prepared} disabled={locked}
                    title={locked ? (s.level === 0 ? "Cantrips are always prepared" : "Always prepared") : undefined}
                    onChange={(e) => void mutate((api, id) => api.setSpellPrepared({ characterId: id, classSlotId: cls.classSlotId, spellId: s.id, prepared: e.target.checked }))} />
                </td>
                <td className="sp-name"><button type="button" className="ap-name" onClick={() => onPick(s.id)}>{s.name}</button><RowDetail parts={[levelText(s), componentText(s.casting), sourceText(s)]} /></td>
                <td className="sp-lvl">{levelText(s)}</td>
                <ComponentCells casting={s.casting} />
                <td className="sp-src">{sourceText(s)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

/** The "known spells" list under the main grid (max height 190): cantrips and prepared spells. */
export function KnownSpells({ cls, picked, onPick }: { cls: ClassSpellcasting; picked: string | null; onPick: (id: string) => void }) {
  const known = cls.spells.filter(isKnown);
  return (
    <section className="sp-known" aria-label="Known spells">
      <h3 className="sp-known-h">Known spells</h3>
      <ul>
        {known.length === 0 && <li className="sp-known-empty">No known spells yet.</li>}
        {known.map((s) => (
          <li key={`${s.id}:${s.slotId ?? ""}`} className={picked === s.id ? "sel" : ""}>
            <button type="button" onClick={() => onPick(s.id)}><b>{s.name}</b><span>{levelText(s)}</span></button>
          </li>
        ))}
      </ul>
    </section>
  );
}
