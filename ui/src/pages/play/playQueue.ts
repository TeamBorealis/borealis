import { useCallback } from "react";
import type { Api, CharacterSnapshot } from "../../api/contract";
import { useCharacter } from "../../state/CharacterContext";

/** The last queued Play change; every Play press waits for the one before it. */
let tail: Promise<unknown> = Promise.resolve();

/**
 * `mutate` for the Play tab's quick presses (pips, -/+, rests, concentration; GitHub #2 review M1): the calls run one after
 * another in press order, so a pip pressed right after a rest reaches the host after the rest, and the snapshots arrive
 * in order. Together with relative commands (one more, one fewer) no press is lost.
 */
export function usePlayMutate(): (fn: (api: Api, id: string) => Promise<CharacterSnapshot>) => Promise<CharacterSnapshot | null> {
  const { mutate } = useCharacter();
  return useCallback((fn) => {
    const run = tail.then(() => mutate(fn));
    tail = run.catch(() => null);
    return run;
  }, [mutate]);
}
