import { useEffect, useState } from "react";
import type { Api, CharacterSnapshot, InventoryItem, RolledCharges } from "../../api/contract";
import { Button, TextField } from "../../components";
import { useCharacter } from "../../state/CharacterContext";
import { errorMessage } from "../../state/useAsync";

/** "7 (1d6 + 3)", or "6 (1d6 + 3, average)" while no roll is stored: the inventory row's charges label. */
export const chargesLabel = (r: RolledCharges): string => `${r.value} (${r.dice}${r.rolled ? "" : ", average"})`;

/** "Maximum 7, rolled on 1d6 + 3 (4 to 9)." for the details of the item or of its resource. */
export const chargesSentence = (r: RolledCharges): string =>
  `Maximum ${r.value}, ${r.rolled ? "rolled on" : "the average of"} ${r.dice} (${r.min} to ${r.max}).${r.rolled ? "" : " No roll is stored yet."}`;

/** The rolled charges of the inventory line holding resource `resourceId` (Play resources point back to their item). */
export function rolledChargeOf(snapshot: CharacterSnapshot, resourceId: string): { item: InventoryItem; charge: RolledCharges } | null {
  for (const item of snapshot.inventory.items) {
    const charge = item.rolledCharges?.find((r) => r.resourceId === resourceId);
    if (charge) return { item, charge };
  }
  return null;
}

/**
 * Enter or re-roll the maximum of a dice-valued charge resource (`setRolledMax` / `rerollMax`). A value outside
 * min..max is refused by the backend; its message is shown under the field.
 */
export function RolledChargesEditor({ item, charge }: { item: InventoryItem; charge: RolledCharges }) {
  const { mutate } = useCharacter();
  const [value, setValue] = useState(String(charge.value));
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setValue(String(charge.value)), [charge.value]);
  const run = (fn: (api: Api, characterId: string) => Promise<CharacterSnapshot>) => void mutate(async (api, id) => {
    try {
      const next = await fn(api, id);
      setError(null);
      return next;
    } catch (e) { setError(errorMessage(e)); throw e; }
  });
  const ids = { inventoryId: item.id, resourceId: charge.resourceId };
  const enter = () => {
    const n = Number(value);
    if (!value.trim() || !Number.isInteger(n) || n < 0) { setError("Enter a whole number."); return; }
    run((api, characterId) => api.setRolledMax({ characterId, ...ids, value: n }));
  };
  return (
    <div className="stack" role="group" aria-label={charge.name}>
      <p className="muted" role="status">{chargesSentence(charge)}</p>
      <TextField label={`Maximum ${charge.name}`} type="number" inputMode="numeric" min={charge.min} max={charge.max} value={value}
        error={error} onChange={(e) => setValue(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") enter(); }} />
      <div className="row">
        <Button onClick={enter}>Set maximum</Button>
        <Button icon="dice" onClick={() => run((api, characterId) => api.rerollMax({ characterId, ...ids }))}>Re-roll</Button>
      </div>
    </div>
  );
}
