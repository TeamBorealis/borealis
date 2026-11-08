import { useState } from "react";
import type { PlayHitPoints } from "../../api/contract";
import { Button, useConfirm } from "../../components";
import { haptic } from "../../lib/haptics";
import { usePlayMutate } from "./playQueue";
import { ShortRestDialog, offersHitDice } from "./ShortRestDialog";
import "./spell-tracker.css";

/** What a Long Rest at 0 hit points asks first (B-212, decision FU1): both SRDs need at least 1 hit point for the benefit. */
export const LONG_REST_AT_ZERO_HP = "At 0 hit points a long rest gives no benefit by the rules. Rest anyway?";

interface RestBarProps { hp: PlayHitPoints }

/**
 * Short Rest and Long Rest for the whole Play page (GitHub #2 review L8): resources, spell slots, Pact Magic slots, custom
 * trackers, concentration, hit points and hit dice. A rest applies at once. A Short Rest then offers hit dice when the
 * character is hurt and has one left (B-194); a Long Rest restores hit points and returns hit dice by the edition's rule.
 * At 0 hit points a Long Rest asks first and goes ahead only on confirm (B-212).
 */
export function RestBar({ hp }: RestBarProps) {
  const run = usePlayMutate();
  const confirm = useConfirm();
  const [offer, setOffer] = useState(false);
  const rest = async (long: boolean) => {
    if (long && hp.current <= 0) {
      const answer = await confirm({ title: "Long Rest", body: LONG_REST_AT_ZERO_HP, confirmLabel: "Rest anyway" });
      if (answer !== "confirm") return;
    }
    haptic("rest");
    const next = await run((api, id) => (long ? api.longRest({ characterId: id }) : api.shortRest({ characterId: id })));
    if (!long && next && offersHitDice(next.play)) setOffer(true);
  };
  return (
    <div className="st-rests" role="group" aria-label="Rests">
      <Button small icon="sparkles" onClick={() => void rest(false)}>Short Rest</Button>
      <Button small variant="primary" icon="star" onClick={() => void rest(true)}>Long Rest</Button>
      {offer && <ShortRestDialog onClose={() => setOffer(false)} />}
    </div>
  );
}
