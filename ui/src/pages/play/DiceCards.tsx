import type { PlayHitDice, PlayResource } from "../../api/contract";
import { Expander } from "../../components/controls";
import { type DiceExpr, tryParseNotation } from "../../lib/dice";
import { useCharacter } from "../../state/CharacterContext";
import { PressButton } from "./rolls/RollButton";
import { useRoller } from "./rolls/RollContext";

/** Most hit dice the menu offers to roll at once. */
const MAX_HIT_DICE_AT_ONCE = 6;

/** `count` dice of the same notation, rolled together (two hit dice = the notation twice). */
const times = (expr: DiceExpr, count: number): DiceExpr => ({ terms: Array.from({ length: count }, () => expr.terms).flat() });

function HitDiceRow({ h }: { h: PlayHitDice }) {
  const roller = useRoller();
  const expr = tryParseNotation(h.notation);
  const label = `${h.className} hit die`;
  const roll = (n: number) => expr && roller.dice({ label: n === 1 ? label : `${n} ${h.className} hit dice`, expr: times(expr, n) });
  const counts = Array.from({ length: Math.min(h.total, MAX_HIT_DICE_AT_ONCE) }, (_, i) => i + 1);
  return (
    <li className="v5-line pr-line">
      <span className="v5-row static grow"><span className="grow">{h.className}</span><span className="meta">{h.total} × d{h.die}</span></span>
      <span className="v5-ctrls pr-rolls">
        {expr && (
          <PressButton label={`Roll a ${label}, ${h.notation}`} menuLabel={`Roll ${h.className} hit dice`}
            items={counts.map((n) => ({ label: n === 1 ? `One die (${h.notation})` : `${n} dice`, run: () => roll(n) }))} onTap={() => roll(1)}>
            {h.notation}
          </PressButton>
        )}
      </span>
    </li>
  );
}

/** Hit dice per class (GitHub #1): a tap rolls one die plus the Constitution modifier, the menu rolls several. */
export function HitDiceCard({ hitDice }: { hitDice: PlayHitDice[] }) {
  if (hitDice.length === 0) return null;
  return (
    <Expander title="Hit Dice">
      <ul className="v5-list" aria-label="Hit dice">{hitDice.map((h) => <HitDiceRow key={h.className} h={h} />)}</ul>
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
