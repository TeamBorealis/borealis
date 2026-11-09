import { useState } from "react";
import { ABILITY_KEYS, type AbilityKey, type Attack, type AttackAbilityLine } from "../../api/contract";
import { Button, Check, Modal, SelectField, TextArea, TextField } from "../../components";
import { calculatedAttack } from "../../lib/attackCalc";
import { ABILITY_LABEL } from "../../lib/format";

export type AttackDraft = Omit<Attack, "id"> & { id?: string };
export const BLANK_ATTACK: AttackDraft = { name: "", range: "Melee", attackBonus: "+0", damage: "1d6", description: "", ability: null, auto: false, showInCombat: true, card: false };

const lineFor = (a: AttackDraft, k: AbilityKey | null): AttackAbilityLine | undefined => a.abilityLines?.find((l) => l.ability === k);

/** A field that still shows the worked-out value of the old ability follows the new one. */
function followAbility(a: AttackDraft, to: AbilityKey): AttackDraft {
  const was = lineFor(a, a.ability);
  const now = lineFor(a, to);
  if (!now) return { ...a, ability: to };
  const keep = (value: string, old: string | undefined, next: string) => (!value.trim() || value === old ? next : value);
  return { ...a, ability: to, attackBonus: keep(a.attackBonus, was?.attackBonus, now.attackBonus), damage: keep(a.damage, was?.damage, now.damage) };
}

/** Aurora's green "calculated" line under the attack bonus and damage of a worked-out row. */
function Calculated({ value, ability }: { value: string | undefined; ability: AbilityKey | null }) {
  if (!value || !ability) return null;
  return <p className="atk-calc">Calculated with {ABILITY_LABEL[ability]}: {value}</p>;
}

/** Ability modifiers and proficiency bonus: what a hand-made row worked out from an ability reads (GitHub #29). */
export interface AttackCalcBasis { modifiers: Record<AbilityKey, number>; proficiency: number }

/**
 * The attack editor (Aurora's CreateAttackWindow): name, range, attack bonus and damage with the worked-out values,
 * a one-line description, the ability of the automatic calculation (rows from a weapon or the Unarmed Strike), and
 * the combat section and card flags. On an automatic row a cleared text goes back to the worked-out value. A
 * hand-made row may be worked out from an ability too (GitHub #29): the modifier plus proficiency, and its dice
 * plus the modifier.
 */
export function AttackDialog({ draft, basis, onSave, onClose }: { draft: AttackDraft; basis: AttackCalcBasis; onSave: (a: AttackDraft) => void; onClose: () => void }) {
  const [a, setA] = useState(draft);
  const set = <K extends keyof AttackDraft>(k: K, v: AttackDraft[K]) => setA((c) => ({ ...c, [k]: v }));
  const linked = draft.auto && (draft.abilityLines?.length ?? 0) > 0;
  const custom = !draft.auto && !!a.calculated && !!a.ability;
  const line = linked ? lineFor(a, a.ability) : custom ? calculatedAttack(a.damage, a.ability!, basis.modifiers, basis.proficiency) : undefined;
  const canSave = draft.auto || a.name.trim() !== "";
  const pickAbility = (k: AbilityKey) => setA((c) => (linked ? followAbility(c, k) : { ...c, ability: k }));
  return (
    <Modal title={draft.id ? `Edit ${draft.name}` : "New attack"} onClose={onClose} footer={<><Button onClick={onClose}>Cancel</Button><Button variant="primary" disabled={!canSave} onClick={() => onSave(a)}>Save</Button></>}>
      <div className="stack">
        <TextField label="Name" value={a.name} data-autofocus onChange={(e) => set("name", e.target.value)} />
        <TextField label="Range" value={a.range} onChange={(e) => set("range", e.target.value)} />
        {custom ? null : <TextField label="Attack bonus" value={a.attackBonus} placeholder={line?.attackBonus} onChange={(e) => set("attackBonus", e.target.value)} />}
        <Calculated value={line?.attackBonus} ability={a.ability} />
        <TextField label="Damage" hint={custom ? "The dice; the ability modifier is added" : undefined} value={a.damage} placeholder={line?.damage} onChange={(e) => set("damage", e.target.value)} />
        <Calculated value={line?.damage} ability={a.ability} />
        <TextArea label="Short description" hint="Limited to a single line under the attack details in the combat section" value={a.description} onChange={(e) => set("description", e.target.value)} />
        {!draft.auto && <Check label="Calculate automatically" checked={!!a.calculated} onChange={(e) => setA((c) => ({ ...c, calculated: e.target.checked, ability: e.target.checked ? c.ability ?? "str" : c.ability }))} />}
        {(linked || custom) && (
          <SelectField label="Ability (automatic calculation)" value={a.ability ?? "str"} onChange={(v) => pickAbility(v as AbilityKey)}
            options={ABILITY_KEYS.map((k) => ({ value: k, label: ABILITY_LABEL[k] }))} />
        )}
        <Check label="Display in combat section" checked={a.showInCombat} onChange={(e) => set("showInCombat", e.target.checked)} />
        <Check label="Print as a card" checked={a.card} onChange={(e) => set("card", e.target.checked)} />
      </div>
    </Modal>
  );
}
