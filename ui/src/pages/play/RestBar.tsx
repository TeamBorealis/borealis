import { useRef, useState } from "react";
import type { PlayHitPoints, PlayState } from "../../api/contract";
import { Button, useConfirm, useToast } from "../../components";
import { haptic } from "../../lib/haptics";
import { useCharacter } from "../../state/CharacterContext";
import { usePlayMutate } from "./playQueue";
import { restSummary } from "./restSummary";
import { ShortRestDialog, offersHitDice } from "./ShortRestDialog";
import "./spell-tracker.css";

/** What a Long Rest at 0 hit points asks first (B-212, decision FU1): both SRDs need at least 1 hit point for the benefit. */
export const LONG_REST_AT_ZERO_HP = "At 0 hit points a long rest gives no benefit by the rules. Rest anyway?";

interface RestBarProps { hp: PlayHitPoints }

/**
 * Short Rest and Long Rest for the whole Play page (GitHub #2 review L8): resources, spell slots, Pact Magic slots, custom
 * trackers, concentration, hit points and hit dice. A rest applies at once. A Short Rest then offers hit dice when the
 * character is hurt and has one left (B-194); a Long Rest restores hit points and returns hit dice by the edition's rule.
 * At 0 hit points a Long Rest asks first and goes ahead only on confirm (B-212). Each rest then says what came back in a
 * toast, or that there was nothing to regain (B-142).
 */
export function RestBar({ hp }: RestBarProps) {
  const run = usePlayMutate();
  const confirm = useConfirm();
  const toast = useToast();
  const { snapshot } = useCharacter();
  // The state the rest starts from is read when the queued call runs, after any press queued before it.
  const latest = useRef<PlayState | null>(null);
  latest.current = snapshot?.play ?? null;
  const [offer, setOffer] = useState(false);
  const rest = async (long: boolean) => {
    if (long && hp.current <= 0) {
      const answer = await confirm({ title: "Long Rest", body: LONG_REST_AT_ZERO_HP, confirmLabel: "Rest anyway" });
      if (answer !== "confirm") return;
    }
    haptic("rest");
    let before: PlayState | null = null;
    const next = await run((api, id) => {
      before = latest.current;
      return long ? api.longRest({ characterId: id }) : api.shortRest({ characterId: id });
    });
    if (!next) return;
    if (before) toast.push(restSummary(long ? "Long Rest" : "Short Rest", before, next.play));
    if (!long && offersHitDice(next.play)) setOffer(true);
  };
  return (
    <div className="st-rests" role="group" aria-label="Rests">
      <Button small icon="sparkles" onClick={() => void rest(false)}>Short Rest</Button>
      <Button small variant="primary" icon="star" onClick={() => void rest(true)}>Long Rest</Button>
      {offer && <ShortRestDialog onClose={() => setOffer(false)} />}
    </div>
  );
}
