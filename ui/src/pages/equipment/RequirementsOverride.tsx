import type { InventoryItem } from "../../api/contract";
import { CheckBox } from "../../components/controls";
import { useCharacter } from "../../state/CharacterContext";

/** The line offers the control: something it grants is withheld, or the override is already on (so it can be switched off). */
export const offersOverride = (item: InventoryItem): boolean => item.overrideRequirements || (item.withheldGrants?.length ?? 0) > 0;

/**
 * U12 (GitHub #3): an equipped item whose grant is withheld because the granted element's own requirements fail (an epic
 * boon added through Additional Feat before level 19) offers "Override requirements". On, the item's grants apply anyway;
 * the line then says "Requirements overridden". Per item line, saved with the character; off is Aurora's behaviour.
 */
export function RequirementsOverride({ item }: { item: InventoryItem }) {
  const { mutate } = useCharacter();
  if (!offersOverride(item)) return null;
  const set = (on: boolean) => void mutate((api, id) => api.setItemRequirementsOverride({ characterId: id, inventoryId: item.id, on }));
  return (
    <span className="eq-stored eq-override">
      <span className="eq-override-note">
        {item.overrideRequirements ? "Requirements overridden" : `Not applied: ${item.withheldGrants.join(", ")} (requirements not met)`}
      </span>
      <label className="check eq-override-toggle" onMouseDown={(e) => e.stopPropagation()}>
        <CheckBox aria-label={`Override requirements for ${item.displayName}`} checked={item.overrideRequirements} onChange={(e) => set(e.target.checked)} />
        <span>Override requirements</span>
      </label>
    </span>
  );
}
