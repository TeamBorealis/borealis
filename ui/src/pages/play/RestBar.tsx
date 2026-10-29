import { Button } from "../../components";
import { haptic } from "../../lib/haptics";
import { usePlayMutate } from "./playQueue";
import "./spell-tracker.css";

/**
 * Short Rest and Long Rest for the whole Play page (GitHub #2 review L8): resources, spell slots, Pact Magic slots, custom
 * trackers and concentration. No confirmation, as before.
 */
export function RestBar() {
  const run = usePlayMutate();
  const rest = (long: boolean) => {
    haptic("rest");
    void run((api, id) => (long ? api.longRest({ characterId: id }) : api.shortRest({ characterId: id })));
  };
  return (
    <div className="st-rests" role="group" aria-label="Rests">
      <Button small icon="sparkles" onClick={() => rest(false)}>Short Rest</Button>
      <Button small variant="primary" icon="star" onClick={() => rest(true)}>Long Rest</Button>
    </div>
  );
}
