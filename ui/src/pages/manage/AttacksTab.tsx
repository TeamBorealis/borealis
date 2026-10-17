import { useState } from "react";
import { ABILITY_KEYS, type AbilityKey, type Attack } from "../../api/contract";
import { Button, Check, Modal, SelectField, TextArea, TextField } from "../../components";
import { ABILITY_LABEL } from "../../lib/format";
import { useCharacter } from "../../state/CharacterContext";
import { DetailField } from "./DetailField";
import { MgSection } from "./Field";
import "./manage-fx15.css";
import { MgIcon, type MgIconName } from "./mgIcons";

const SHEET_LIMIT = 4;
type Draft = Omit<Attack, "id"> & { id?: string };
const BLANK: Draft = { name: "", range: "Melee", attackBonus: "+0", damage: "1d6", description: "", ability: null, auto: false, showInCombat: true, card: false };

function AttackDialog({ draft, onSave, onClose }: { draft: Draft; onSave: (a: Draft) => void; onClose: () => void }) {
  const [a, setA] = useState(draft);
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => setA((c) => ({ ...c, [k]: v }));
  return (
    <Modal title={draft.id ? `Edit ${draft.name}` : "New attack"} onClose={onClose} footer={<><Button onClick={onClose}>Cancel</Button><Button variant="primary" disabled={!a.name.trim()} onClick={() => onSave(a)}>Save</Button></>}>
      <div className="stack">
        <TextField label="Name" value={a.name} data-autofocus onChange={(e) => set("name", e.target.value)} />
        <TextField label="Range" value={a.range} onChange={(e) => set("range", e.target.value)} />
        <Check label="Calculate automatically" checked={a.auto} onChange={(e) => setA((c) => ({ ...c, auto: e.target.checked, ability: e.target.checked ? c.ability ?? "str" : null }))} />
        {a.auto ? (
          <SelectField label="Linked ability" value={a.ability ?? "str"} onChange={(e) => set("ability", e.target.value as AbilityKey)}>
            {ABILITY_KEYS.map((k) => <option key={k} value={k}>{ABILITY_LABEL[k]}</option>)}
          </SelectField>
        ) : <TextField label="Attack bonus" value={a.attackBonus} onChange={(e) => set("attackBonus", e.target.value)} />}
        <TextField label="Damage" value={a.damage} onChange={(e) => set("damage", e.target.value)} />
        <TextArea label="Short description" value={a.description} onChange={(e) => set("description", e.target.value)} />
        <Check label="Display in combat section" checked={a.showInCombat} onChange={(e) => set("showInCombat", e.target.checked)} />
        <Check label="Print as a card" checked={a.card} onChange={(e) => set("card", e.target.checked)} />
      </div>
    </Modal>
  );
}

function IconAct({ icon, label, onClick, disabled, pressed }: { icon: MgIconName; label: string; onClick: () => void; disabled?: boolean; pressed?: boolean }) {
  return <button type="button" aria-label={label} title={label} disabled={disabled} aria-pressed={pressed} onClick={(e) => { e.stopPropagation(); onClick(); }}><MgIcon name={icon} /></button>;
}

/** The tag row under an attack line: auto, mastery, this weapon's critical range, ammunition, card, hidden. */
export function AttackTags({ attack: a }: { attack: Attack }) {
  return (
    <span className="ap-tags">
      {a.auto && <span className="ap-tag">Auto</span>}
      {a.mastery && <span className={`ap-tag ${a.mastery.mastered ? "" : "warn"}`} title={a.mastery.text}>{a.mastery.name}{a.mastery.mastered ? "" : " (not mastered)"}</span>}
      {a.critRange !== undefined && a.critRange < 20 && <span className="ap-tag" title="Critical hit range">Crit {a.critRange}-20</span>}
      {a.dieFloor ? <span className="ap-tag" title="Minimum on each damage die">Die min {a.dieFloor}</span> : null}
      {a.offhandDamage && <span className="ap-tag" title="Damage when used off-hand">Off-hand {a.offhandDamage}</span>}
      {a.ammunition && <span className="ap-tag" title="Ammunition bonus included in attack and damage">Ammunition: {a.ammunition}</span>}
      {a.card && a.showInCombat && <span className="ap-tag" title="Printed as an attack card after the sheet pages">Card</span>}
      {!a.showInCombat && <span className="ap-tag">Hidden</span>}
    </span>
  );
}

export function AttacksTab() {
  const { snapshot, mutate } = useCharacter();
  const [draft, setDraft] = useState<Draft | null>(null);
  const [sel, setSel] = useState<string | null>(null);
  if (!snapshot) return null;
  const list = snapshot.attacks;
  const selIdx = list.findIndex((a) => a.id === sel);
  const onSheet = list.filter((a) => a.showInCombat).length;
  const move = (attackId: string, direction: "up" | "down") => void mutate((api, id) => api.moveAttack({ characterId: id, attackId, direction }));
  return (
    <div className="mg-page">
      <div className="mg-attacks">
        <MgSection title="Attacks" sub={<>Up to {SHEET_LIMIT} full attacks fit on the character sheet ({onSheet}/{SHEET_LIMIT} shown). Add one from any weapon in your inventory through its context menu entry for attacks. That route is best, because the bonus and damage are worked out for you. You may also choose to print an attack card on a last page of the sheet. Anything you add here stays with this character and travels with the saved file, so it is there again whenever you reopen the hero, on this device or any other one that you copy the file to.</>}>
          <p className="mg-sub mg-sub-after">Or you can write the attack details yourself.</p>
          <div className="mg-toolbar" role="toolbar" aria-label="Attack actions">
            <button type="button" className="mg-tool wide" onClick={() => setDraft(BLANK)}><MgIcon name="addAttack" />Add Attack</button>
            <button type="button" className="mg-tool" aria-label="Move selected attack up" title="Move selected attack up" disabled={selIdx <= 0} onClick={() => move(sel!, "up")}><MgIcon name="moveUp" /></button>
            <button type="button" className="mg-tool" aria-label="Move selected attack down" title="Move selected attack down" disabled={selIdx < 0 || selIdx >= list.length - 1} onClick={() => move(sel!, "down")}><MgIcon name="moveDown" /></button>
          </div>
          {list.length > 0 && (
            <ul className="atk-list" aria-label="Attacks">
              {list.map((a) => (
                <li key={a.id} className={`atk ${a.showInCombat ? "" : "atk-off"}`} aria-current={a.id === sel} onClick={() => setSel(a.id)}>
                  <div className="info">
                    <h3>{a.name}</h3>
                    <p>Range: {a.range}</p>
                    <p>Attack: {a.attackBonus} vs AC</p>
                    <p>Damage: {a.damage}{a.description ? ` ${a.description}` : ""}</p>
<AttackTags attack={a} />
                  </div>
                  <span className="btns">
                    <IconAct icon="list" label={a.showInCombat ? `Hide ${a.name} from the sheet` : `Show ${a.name} on the sheet`} pressed={a.showInCombat}
                      onClick={() => void mutate((api, id) => api.upsertAttack({ characterId: id, attack: { ...a, showInCombat: !a.showInCombat } }))} />
                    <IconAct icon="edit" label={`Edit ${a.name}`} onClick={() => setDraft(a)} />
                    <IconAct icon="cardMinus" label={`Remove ${a.name}`} onClick={() => { if (sel === a.id) setSel(null); void mutate((api, id) => api.removeAttack({ characterId: id, attackId: a.id })); }} />
                  </span>
                </li>
              ))}
            </ul>
          )}
        </MgSection>
        <MgSection title="Additional details" sub="Below your attacks you have room for a few notes on the fighting abilities of you or your companion.">
          <DetailField k="combatNotes" label="Additional combat details" area />
        </MgSection>
      </div>
      {draft && <AttackDialog draft={draft} onClose={() => setDraft(null)} onSave={(a) => { setDraft(null); void mutate((api, id) => api.upsertAttack({ characterId: id, attack: { ...a, asCard: a.card } })); }} />}
    </div>
  );
}
