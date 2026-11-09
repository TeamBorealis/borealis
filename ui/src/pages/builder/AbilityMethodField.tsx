import { useId } from "react";
import type { AbilityMethod, AbilityRecord } from "../../api/contract";
import { ComboBox, useConfirm } from "../../components";
import { abilityMethodOptions, methodChangeAltersScores, methodChangeMessage } from "../../lib/abilityMethods";

export interface AbilityMethodFieldProps {
  method: AbilityMethod;
  base: AbilityRecord<number>;
  onChange: (method: AbilityMethod) => void;
}

/**
 * B-97 (GitHub #12, U16a): the ability generation method on the Abilities tab, with the New Character flyout's label
 * and choices. A switch that would replace base scores asks first; one that keeps them does not.
 */
export function AbilityMethodField({ method, base, onChange }: AbilityMethodFieldProps) {
  const id = useId();
  const confirm = useConfirm();
  const pick = async (next: AbilityMethod) => {
    if (next === method) return;
    if (methodChangeAltersScores(base, next)) {
      const answer = await confirm({ title: "Replace the base scores?", body: <p>{methodChangeMessage(next)}</p>, confirmLabel: "Change" });
      if (answer !== "confirm") return;
    }
    onChange(next);
  };
  return (
    <span className="abil-method">
      <label className="abil-method-label" htmlFor={id}>Abilities generation option</label>
      <ComboBox id={id} className="abil-method-combo" options={[...abilityMethodOptions]} value={method}
        onChange={(v) => void pick(v as AbilityMethod)} />
    </span>
  );
}
