import { useState } from "react";
import { ABILITY_KEYS, type AbilityKey, type Attack, type AttackAbilityLine } from "../../api/contract";
import { Button, Check, Modal, SelectField, TextArea, TextField } from "../../components";
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

/** Aurora's green "calculated" line under the attack bonus and damage of an automatic row. */
function Calculated({ value, ability }: { value: string | undefined; ability: AbilityKey | null }) {
  if (!value || !ability) return null;
  return <p className="atk-calc">Calculated with {ABILITY_LABEL[ability]}: {value}</p>;
}

/**
 * The attack editor (Aurora's CreateAttackWindow): name, range, attack bonus and damage with the worked-out values,
 * a one-line description, the ability of the automatic calculation (rows from a weapon or the Unarmed Strike), and
 * the combat section and card flags. On an automatic row a cleared text goes back to the worked-out value.
 */
export function AttackDialog({ draft, onSave, onClose }: { draft: AttackDraft; onSave: (a: AttackDraft) => void; onClose: () => void }) {
  const [a, setA] = useState(draft);
  const set = <K extends keyof AttackDraft>(k: K, v: AttackDraft[K]) => setA((c) => ({ ...c, [k]: v }));
  const linked = draft.auto && (draft.abilityLines?.length ?? 0) > 0;
  const line = lineFor(a, a.ability);
  const canSave = draft.auto || a.name.trim() !== "";
  return (
    <Modal title={draft.id ? `Edit ${draft.name}` : "New attack"} onClose={onClose} footer={<><Button onClick={onClose}>Cancel</Button><Button variant="primary" disabled={!canSave} onClick={() => onSave(a)}>Save</Button></>}>
      <div className="stack">
        <TextField label="Name" value={a.name} data-autofocus onChange={(e) => set("name", e.target.value)} />
        <TextField label="Range" value={a.range} onChange={(e) => set("range", e.target.value)} />
        {!draft.auto && <Check label="Calculate automatically" checked={a.auto} onChange={(e) => setA((c) => ({ ...c, auto: e.target.checked, ability: e.target.checked ? c.ability ?? "str" : null }))} />}
        {!draft.auto && a.auto ? (
          <SelectField label="Linked ability" value={a.ability ?? "str"} onChange={(e) => set("ability", e.target.value as AbilityKey)}>
            {ABILITY_KEYS.map((k) => <option key={k} value={k}>{ABILITY_LABEL[k]}</option>)}
          </SelectField>
        ) : (
          <>
            <TextField label="Attack bonus" value={a.attackBonus} placeholder={line?.attackBonus} onChange={(e) => set("attackBonus", e.target.value)} />
            {linked && <Calculated value={line?.attackBonus} ability={a.ability} />}
          </>
        )}
        <TextField label="Damage" value={a.damage} placeholder={line?.damage} onChange={(e) => set("damage", e.target.value)} />
        {linked && <Calculated value={line?.damage} ability={a.ability} />}
        <TextArea label="Short description" hint="Limited to a single line under the attack details in the combat section" value={a.description} onChange={(e) => set("description", e.target.value)} />
        {linked && (
          <SelectField label="Ability (automatic calculation)" value={a.ability ?? "str"} onChange={(e) => setA((c) => followAbility(c, e.target.value as AbilityKey))}>
            {ABILITY_KEYS.map((k) => <option key={k} value={k}>{ABILITY_LABEL[k]}</option>)}
          </SelectField>
        )}
        <Check label="Display in combat section" checked={a.showInCombat} onChange={(e) => set("showInCombat", e.target.checked)} />
        <Check label="Print as a card" checked={a.card} onChange={(e) => set("card", e.target.checked)} />
      </div>
    </Modal>
  );
}
