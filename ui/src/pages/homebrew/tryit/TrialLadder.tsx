import type { HomebrewLadderRow } from "../../../api/contract";
import { Icon, RowDetail } from "../../../components";

const ORDINALS = ["1st", "2nd", "3rd", "4th", "5th", "6th", "7th", "8th", "9th"];

/** "4 / 3 / 2": the slots per spell level, up to the highest level with slots; "" without any. */
export function slotsText(slots: readonly number[]): string {
  const last = slots.reduce((top, n, i) => (n > 0 ? i : top), -1);
  return last < 0 ? "" : slots.slice(0, last + 1).join(" / ");
}

/** The slots in words for screen readers and phones ("1st 4, 2nd 3"). */
const slotsWords = (slots: readonly number[]): string =>
  slots.map((n, i) => (n > 0 ? `${ORDINALS[i]} ${n}` : "")).filter(Boolean).join(", ");

const gainedText = (r: HomebrewLadderRow): string => r.features.join(", ");
const choicesText = (r: HomebrewLadderRow): string => (r.choices.length === 0 ? "" : `${r.choices.length === 1 ? "1 choice" : `${r.choices.length} choices`}: ${r.choices.join(", ")}`);

/**
 * The level ladder (spec 9): one row per level 1-20 with the features gained, the new choices, the spell slots and the
 * sweep's problems. A row's level steps the test character to it (the current level is the selected row). Phones show
 * one-line rows with the rest as the detail line (spec 16).
 */
export function TrialLadder({ ladder, level, onLevel }: { ladder: readonly HomebrewLadderRow[]; level: number; onLevel: (level: number) => void }) {
  return (
    <div className="ap-scroll hb-try-ladder">
      <table className="ap-grid" aria-label="Level ladder">
        <thead><tr><th>Level</th><th>Gained</th><th>Slots</th></tr></thead>
        <tbody>
          {ladder.map((r) => {
            const slots = slotsText(r.slots);
            const choices = choicesText(r);
            return (
              <tr key={r.level} className={r.level === level ? "sel" : ""} aria-current={r.level === level || undefined} data-level={r.level}>
                <td className="hb-try-lvl">
                  <button type="button" className="ap-name" aria-label={`Level ${r.level}`} onClick={() => onLevel(r.level)}>{r.level}</button>
                  <RowDetail parts={[gainedText(r), choices, slots && `Slots ${slotsWords(r.slots)}`, r.problems.length > 0 && `${r.problems.length} ${r.problems.length === 1 ? "problem" : "problems"}`]} />
                </td>
                <td className="hide-narrow grow">
                  {gainedText(r) && <span className="hb-try-gained">{gainedText(r)}</span>}
                  {choices && <span className="hb-try-choices">{choices}</span>}
                  {r.problems.map((p, i) => (
                    <span key={`${p.code}-${i}`} className={`hb-try-problem hb-${p.severity}`}><Icon name={p.severity === "error" ? "x" : "alert"} />{p.message}</span>
                  ))}
                </td>
                <td className="hide-narrow hb-try-slotcell">
                  {slots && <span aria-label={slotsWords(r.slots)}>{slots}</span>}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
