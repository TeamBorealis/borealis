import { ABILITY_KEYS, type AbilityKey, type AbilityRecord, type AbilityState, type Api, type CharacterSnapshot, type SuggestedScores } from "../../api/contract";
import { Button } from "../../components";
import { useCharacter } from "../../state/CharacterContext";

const SHORT = (k: AbilityKey): string => k.toUpperCase();

/**
 * Sets the base scores to `target` through the method's own commands: the standard array assigns each value, other
 * methods set each score, lowering scores first so a point buy budget is never overspent on the way.
 */
export async function applySuggested(api: Api, characterId: string, a: Pick<AbilityState, "method" | "base">, target: AbilityRecord<number>): Promise<CharacterSnapshot> {
  const changed = ABILITY_KEYS.filter((k) => a.base[k] !== target[k]);
  let last: CharacterSnapshot | null = null;
  if (a.method === "standard_array") {
    for (const k of ABILITY_KEYS) last = await api.assignStandardArray({ characterId, ability: k, value: target[k] });
  } else {
    const ordered = [...changed.filter((k) => target[k] < a.base[k]), ...changed.filter((k) => target[k] > a.base[k])];
    for (const k of ordered) last = await api.setBaseScore({ characterId, ability: k, value: target[k] });
  }
  return last ?? api.getSnapshot({ characterId });
}

/**
 * Tester B11 (B-135, TV2): "Suggested for <class>", the class's standard array from the ruleset's tables, with Apply. It
 * never applies by itself; once the base scores match it reads "Applied".
 */
export function SuggestedScoresCard({ abilities, suggested }: { abilities: AbilityState; suggested: SuggestedScores }) {
  const { mutate, busy } = useCharacter();
  const title = `Suggested for ${suggested.className}`;
  const applied = ABILITY_KEYS.every((k) => abilities.base[k] === suggested.scores[k]);
  return (
    <section className="abil-suggest" role="region" aria-label={title}>
      <div className="abil-suggest-text">
        <h3 className="abil-suggest-title">{title}</h3>
        <p className="abil-suggest-scores">
          {ABILITY_KEYS.map((k) => <span key={k}>{`${SHORT(k)} ${suggested.scores[k]}`}</span>)}
        </p>
      </div>
      <Button disabled={applied || busy} onClick={() => void mutate((api, id) => applySuggested(api, id, abilities, suggested.scores))}>
        {applied ? "Applied" : "Apply"}
      </Button>
    </section>
  );
}
