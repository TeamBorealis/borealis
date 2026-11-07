import { useCallback } from "react";
import type { PlayHitDice, PlayResource } from "../../api/contract";
import { Button } from "../../components";
import { Expander } from "../../components/controls";
import { type DiceExpr, type DiceResult, tryParseNotation } from "../../lib/dice";
import { useCharacter } from "../../state/CharacterContext";
import { usePlayMutate } from "./playQueue";
import { PressButton } from "./rolls/RollButton";
import { useRoller } from "./rolls/RollContext";

/** Most hit dice the menu offers to roll at once. */
const MAX_HIT_DICE_AT_ONCE = 6;

/** `count` dice of the same notation, rolled together (two hit dice = the notation twice). */
const times = (expr: DiceExpr, count: number): DiceExpr => ({ terms: Array.from({ length: count }, () => expr.terms).flat() });

/** The face the die showed in a hit die roll (the first dice term's roll). */
export const dieFace = (r: DiceResult): number | null => r.terms.find((t) => t.term.kind === "dice")?.rolls[0] ?? null;

/**
 * Spend one hit die (B-194): roll it with the Constitution modifier (the result card shows it), then the host marks the die
 * spent and heals by the roll plus the modifier. Returns the roll, or null when none is left.
 */
export function useSpendHitDie(): (h: PlayHitDice) => DiceResult | null {
  const roller = useRoller();
  const run = usePlayMutate();
  return useCallback((h: PlayHitDice) => {
    const expr = tryParseNotation(h.notation);
    if (!expr || h.used >= h.total) return null;
    const result = roller.dice({ label: `${h.className} hit die spent`, expr });
    const roll = dieFace(result);
    if (roll !== null) void run((api, id) => api.spendHitDie({ characterId: id, classId: h.classId, roll }));
    return result;
  }, [roller, run]);
}

function HitDiceRow({ h }: { h: PlayHitDice }) {
  const roller = useRoller();
  const run = usePlayMutate();
  const spend = useSpendHitDie();
  const expr = tryParseNotation(h.notation);
  const label = `${h.className} hit die`;
  const left = h.total - h.used;
  const roll = (n: number) => expr && roller.dice({ label: n === 1 ? label : `${n} ${h.className} hit dice`, expr: times(expr, n) });
  const restore = () => void run((api, id) => api.restoreHitDie({ characterId: id, classId: h.classId }));
  const counts = Array.from({ length: Math.min(h.total, MAX_HIT_DICE_AT_ONCE) }, (_, i) => i + 1);
  return (
    <li className="v5-line pr-line">
      <span className="v5-row static grow"><span className="grow">{h.className}</span><span className="meta">{h.total} × d{h.die}</span></span>
      <span className="v5-count" role="status" aria-label={`${h.className} hit dice: ${left} of ${h.total} left`}>{left}/{h.total}</span>
      <span className="v5-ctrls pr-rolls">
        {expr && (
          <PressButton label={`Roll a ${label}, ${h.notation}`} menuLabel={`Roll ${h.className} hit dice`}
            items={counts.map((n) => ({ label: n === 1 ? `One die (${h.notation})` : `${n} dice`, run: () => roll(n) }))} onTap={() => roll(1)}>
            {h.notation}
          </PressButton>
        )}
        <Button small disabled={left <= 0 || !expr} aria-label={`Spend a ${label}`} onClick={() => spend(h)}>Spend</Button>
        <Button small disabled={h.used <= 0} aria-label={`Restore a ${label}`} onClick={restore}>Restore</Button>
      </span>
    </li>
  );
}

/**
 * Hit dice per class (GitHub #1, B-194): dice left of the total; a tap on the notation rolls without spending, Spend rolls
 * one and heals by it, Restore gives a spent die back. A Long Rest returns them by the edition's rule.
 */
export function HitDiceCard({ hitDice }: { hitDice: PlayHitDice[] }) {
  if (hitDice.length === 0) return null;
  return (
    <Expander title="Hit Dice">
      <ul className="v5-list" aria-label="Hit dice">{hitDice.map((h) => <HitDiceRow key={h.classId} h={h} />)}</ul>
    </Expander>
  );
}

/** The die of a resource (Bardic Inspiration): a tap rolls it; the menu can spend a use and roll in one go. */
export function ResourceDieButton({ r }: { r: PlayResource }) {
  const roller = useRoller();
  const { mutate } = useCharacter();
  const expr = r.die ? tryParseNotation(r.die) : null;
  if (!expr || !r.die) return null;
  const roll = () => roller.dice({ label: r.name, expr });
  const spendAndRoll = async () => {
    const next = await mutate((api, id) => api.spendResource({ characterId: id, resourceId: r.id }));
    if (next) roll();
  };
  return (
    <PressButton label={`Roll ${r.name}, ${r.die}`} menuLabel={`Roll ${r.name}`} onTap={roll}
      items={[{ label: `Roll ${r.die}`, run: roll }, { label: "Spend one and roll", run: () => void spendAndRoll(), disabled: r.current <= 0 }]}>
      {r.die}
    </PressButton>
  );
}
