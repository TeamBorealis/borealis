import { useEffect, useId, useState } from "react";
import type { CharacterDetails } from "../../api/contract";
import { useCharacter } from "../../state/CharacterContext";

function TreasureField({ k, label }: { k: "treasure" | "questItems"; label: string }) {
  const { snapshot, mutate } = useCharacter();
  const id = useId();
  const current = snapshot?.details[k] ?? "";
  const [v, setV] = useState(current);
  useEffect(() => setV(current), [current]);
  const commit = () => {
    if (v === current) return;
    const patch: Partial<CharacterDetails> = { [k]: v };
    void mutate((api, cid) => api.updateDetails({ characterId: cid, patch }));
  };
  return (
    <div className="eq-tfield">
      <label htmlFor={id} className="eq-tlabel">{label}</label>
      <textarea id={id} aria-label={label} className="eq-ttext" value={v} onChange={(e) => setV(e.target.value)} onBlur={commit} />
    </div>
  );
}

/** Classic builder "ADDITIONAL TREASURE" sub-tab: two labelled full-width multi-line boxes. */
export function TreasureTab() {
  return (
    <div className="eq-treasure">
      <TreasureField k="treasure" label="Additional treasure" />
      <TreasureField k="questItems" label="Quest items" />
    </div>
  );
}
