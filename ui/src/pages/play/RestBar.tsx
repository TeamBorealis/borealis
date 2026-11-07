import { useState } from "react";
import { Button } from "../../components";
import { haptic } from "../../lib/haptics";
import { usePlayMutate } from "./playQueue";
import { ShortRestDialog, offersHitDice } from "./ShortRestDialog";
import "./spell-tracker.css";

/**
 * Short Rest and Long Rest for the whole Play page (GitHub #2 review L8): resources, spell slots, Pact Magic slots, custom
 * trackers, concentration, hit points and hit dice. No confirmation, as before: the rest applies at once. A Short Rest
 * then offers hit dice when the character is hurt and has one left (B-194); a Long Rest restores hit points and returns
 * hit dice by the edition's rule.
 */
export function RestBar() {
  const run = usePlayMutate();
  const [offer, setOffer] = useState(false);
  const rest = async (long: boolean) => {
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
