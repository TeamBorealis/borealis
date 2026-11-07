import { useState } from "react";
import type { PlayState } from "../../api/contract";
import { Button } from "../../components";
import { Dialog } from "../../components/controls";
import { useCharacter } from "../../state/CharacterContext";
import { useSpendHitDie } from "./DiceCards";
import "./hit-points.css";

/** After a Short Rest the Play tab offers hit dice when the character is hurt and has a die left (B-194). */
export function offersHitDice(play: PlayState): boolean {
  return play.hitPoints.current < play.hitPoints.max && play.hitDice.some((h) => h.used < h.total);
}

/**
 * The Short Rest's hit dice (SRD 5.1 / 5.2.1 "Short Rest": spend one or more, deciding after each roll). Each Spend rolls
 * one die plus the Constitution modifier and heals by it; the roll shows here because the dialog covers the result card.
 */
export function ShortRestDialog({ onClose }: { onClose: () => void }) {
  const { snapshot } = useCharacter();
  const spend = useSpendHitDie();
  const [last, setLast] = useState<string | null>(null);
  if (!snapshot) return null;
  const { hitPoints: hp, hitDice } = snapshot.play;
  return (
    <Dialog title="Short Rest" onClose={onClose} footer={<Button variant="primary" onClick={onClose}>Done</Button>}>
      <p className="hp-rest-hp" role="status" aria-label={`Hit points: ${hp.current} of ${hp.max}`}>Hit points <strong>{hp.current}</strong> of {hp.max}</p>
      <p className="v5-note">Spend hit dice to regain hit points: each rolls its die plus the Constitution modifier.</p>
      <ul className="v5-list" aria-label="Hit dice to spend">
        {hitDice.map((h) => {
          const left = h.total - h.used;
          const press = () => {
            const r = spend(h);
            if (r) setLast(`${h.className} hit die: ${r.breakdown}`);
          };
          return (
            <li key={h.classId} className="v5-line pr-line">
              <span className="v5-row static grow"><span className="grow">{h.className}</span><span className="meta">d{h.die}</span></span>
              <span className="v5-count">{left}/{h.total}</span>
              <span className="v5-ctrls">
                <Button small disabled={left <= 0} aria-label={`Spend a ${h.className} hit die, ${left} of ${h.total} left`} onClick={press}>Spend</Button>
              </span>
            </li>
          );
        })}
      </ul>
      {last && <p className="v5-note hp-rest-roll" role="status">{last}</p>}
    </Dialog>
  );
}
