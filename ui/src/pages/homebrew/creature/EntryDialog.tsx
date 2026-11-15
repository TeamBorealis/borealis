import { useId, useState } from "react";
import type {
  CreatureAbility, HomebrewAttack, HomebrewAttackKind, HomebrewCreatureEntry, HomebrewDamagePart, HomebrewSaveEffect, HomebrewUsage,
} from "../../../api/contract";
import { Button, CheckBox, ComboBox, Dialog, Icon, NumericUpDown, TextBox, useConfirm } from "../../../components";
import { useCreatorEnv } from "../env";
import { ABILITIES, ATTACK_KINDS, attackText, capitalised, DAMAGE_TYPES, diceAverage, newDamage } from "./creature";
import { OptionalNumber } from "./StatParts";
import { SpellcastingPart } from "./SpellcastingPart";

const USES = [
  { value: "", label: "Any time" }, { value: "recharge", label: "Recharge on a d6" }, { value: "per_day", label: "Times per day" },
  { value: "per_day_in_lair", label: "Times per day, more in its lair" }, { value: "short_or_long", label: "Recharges after a Short or Long Rest" },
  { value: "long", label: "Recharges after a Long Rest" },
];

const useKey = (u: HomebrewUsage | undefined): string => (!u ? "" : "recharge" in u ? "recharge" : "per_day" in u ? "per_day" : "per_day_in_lair" in u ? "per_day_in_lair" : u.on_rest);

function usageFor(key: string): HomebrewUsage | undefined {
  switch (key) {
    case "recharge": return { recharge: { min: 5, max: 6 } };
    case "per_day": return { per_day: 1 };
    case "per_day_in_lair": return { per_day_in_lair: { normal: 3, lair: 4 } };
    case "short_or_long": case "long": return { on_rest: key };
    default: return undefined;
  }
}

/** Damage parts: average, dice, type; the dice give the average. */
function DamageParts({ label, parts, onChange }: { label: string; parts: readonly HomebrewDamagePart[]; onChange: (p: HomebrewDamagePart[]) => void }) {
  const set = (i: number, patch: Partial<HomebrewDamagePart>) => onChange(parts.map((d, j) => (j === i ? { ...d, ...patch } : d)));
  return (
    <div className="hb-cr-part" role="group" aria-label={label}>
      {parts.map((d, i) => (
        <span key={i} className="hb-row-controls">
          <TextBox aria-label={`${label} ${i + 1}: dice`} placeholder="1d6+2" value={d.dice ?? ""}
            onChange={(e) => { const dice = e.target.value; const avg = diceAverage(dice); set(i, { dice: dice || undefined, ...(avg !== null ? { average: avg } : {}) }); }} />
          <NumericUpDown label={`${label} ${i + 1}: average`} value={d.average} min={0} max={999} compact onChange={(n) => set(i, { average: n })} />
          <ComboBox aria-label={`${label} ${i + 1}: type`} value={d.type} options={DAMAGE_TYPES.map((t) => ({ value: t, label: capitalised(t) }))} onChange={(type) => set(i, { type })} />
          <button type="button" className="hb-chip-x" aria-label={`Remove ${label.toLowerCase()} ${i + 1}`} onClick={() => onChange(parts.filter((_, j) => j !== i))}><Icon name="x" /></button>
        </span>
      ))}
      <button type="button" className="hb-pick-btn" onClick={() => onChange([...parts, newDamage()])}><Icon name="plus" />Add damage</button>
    </div>
  );
}

function AttackPart({ attack, onChange }: { attack: HomebrewAttack; onChange: (a: HomebrewAttack) => void }) {
  const set = (patch: Partial<HomebrewAttack>) => onChange({ ...attack, ...patch });
  const melee = attack.kind.startsWith("melee");
  const ranged = attack.kind.includes("ranged");
  return (
    <div className="hb-cr-part" role="group" aria-label="Attack">
      <span className="hb-row-controls">
        <ComboBox aria-label="Attack kind" value={attack.kind} options={ATTACK_KINDS.map((k) => ({ value: k.value, label: k.label }))} onChange={(k) => set({ kind: k as HomebrewAttackKind })} />
        <span className="hb-inline-label">To hit</span>
        <NumericUpDown label="To hit" value={attack.to_hit} min={-5} max={30} compact onChange={(n) => set({ to_hit: n })} />
        {melee && <><span className="hb-inline-label">Reach</span><OptionalNumber label="Reach (feet)" value={attack.reach} onChange={(n) => set({ reach: n })} /></>}
        {ranged && (
          <>
            <span className="hb-inline-label">Range</span><OptionalNumber label="Range (feet)" value={attack.range} onChange={(n) => set({ range: n })} />
            <span className="hb-inline-label">Long range</span><OptionalNumber label="Long range (feet)" value={attack.long_range} onChange={(n) => set({ long_range: n })} />
          </>
        )}
      </span>
      <DamageParts label="Damage" parts={attack.damage ?? []} onChange={(damage) => set({ damage })} />
    </div>
  );
}

function SavePart({ save, onChange }: { save: HomebrewSaveEffect; onChange: (s: HomebrewSaveEffect) => void }) {
  const set = (patch: Partial<HomebrewSaveEffect>) => onChange({ ...save, ...patch });
  return (
    <div className="hb-cr-part" role="group" aria-label="Saving throw">
      <span className="hb-row-controls">
        <ComboBox aria-label="Saving throw ability" value={save.ability} options={ABILITIES.map((a) => ({ value: a.key, label: a.label }))} onChange={(a) => set({ ability: a as CreatureAbility })} />
        <span className="hb-inline-label">DC</span>
        <NumericUpDown label="Saving throw DC" value={save.dc} min={1} max={30} compact onChange={(n) => set({ dc: n })} />
        <label className="hb-cr-cell"><CheckBox checked={save.half_on_success === true} onChange={(e) => set({ half_on_success: e.target.checked || undefined })} />Half damage on a success</label>
      </span>
      <DamageParts label="Failure damage" parts={save.damage ?? []} onChange={(damage) => set({ damage })} />
    </div>
  );
}

export interface EntryDialogProps {
  /** "Action", "Trait", "Legendary action". */
  noun: string;
  entry: HomebrewCreatureEntry;
  isNew: boolean;
  /** A legendary action has a cost. */
  costs?: boolean;
  /** The draft's sentences for this entry. */
  problems: readonly string[];
  onOk: (entry: HomebrewCreatureEntry) => void;
  onCancel: () => void;
}

/**
 * One trait, action, bonus action, reaction or legendary action (spec 4.11), laid out like Aurora's Attack editor (13.6):
 * name, when it is used, its text, then the parts the stat block reads from it: an attack (kind, to-hit, reach or
 * range, damage), a saving throw, a spellcasting block. "Write the text" fills the text from the attack.
 */
export function EntryDialog({ noun, entry, isNew, costs = false, problems, onOk, onCancel }: EntryDialogProps) {
  const env = useCreatorEnv();
  const confirm = useConfirm();
  const ids = useId();
  const [e, setE] = useState<HomebrewCreatureEntry>(entry);
  const [tried, setTried] = useState(false);
  const set = (patch: Partial<HomebrewCreatureEntry>) => setE((x) => ({ ...x, ...patch }));
  const drop = (key: "attack" | "save" | "spellcasting" | "usage") => setE((x) => { const { [key]: _d, ...rest } = x; return rest as HomebrewCreatureEntry; });
  const missing = !e.name.trim() || !e.text.trim();
  const dirty = JSON.stringify(e) !== JSON.stringify(entry);
  const ok = () => {
    if (missing) { setTried(true); return; }
    onOk(e);
  };
  const cancel = async () => {
    if (dirty && (await confirm({ title: "Discard changes?", body: `Discard the changes made to this ${noun.toLowerCase()}?`, confirmLabel: "Discard", danger: true })) !== "confirm") return;
    onCancel();
  };
  const u = e.usage;
  return (
    <Dialog title={isNew ? `New ${noun.toLowerCase()}` : `${noun}: ${entry.name}`} onClose={() => void cancel()} className="hb-block-dialog hb-cr-entry-dialog"
      footer={<><Button onClick={() => void cancel()}>Cancel</Button><Button variant="primary" onClick={ok}>OK</Button></>}>
      <div className="hb-block-body">
        <div className="mg-f hb-field">
          <label htmlFor={`${ids}-name`}>Name<span className="hb-req" aria-hidden="true"> *</span></label>
          <TextBox id={`${ids}-name`} value={e.name} invalid={tried && !e.name.trim()} onChange={(ev) => set({ name: ev.target.value })} />
        </div>
        <div className="mg-f hb-field">
          <label htmlFor={`${ids}-cond`}>Only in a form</label>
          <TextBox id={`${ids}-cond`} placeholder="Humanoid Form Only" value={e.condition ?? ""} onChange={(ev) => set({ condition: ev.target.value || undefined })} />
        </div>
        <div className="mg-f hb-field hb-bf-wide">
          <label htmlFor={`${ids}-use`}>Used</label>
          <span className="hb-row-controls">
            <ComboBox id={`${ids}-use`} value={useKey(u)} options={USES} onChange={(k) => { const next = usageFor(k); if (next) set({ usage: next }); else drop("usage"); }} />
            {u && "recharge" in u && <NumericUpDown label="Recharges on" value={u.recharge.min} min={2} max={6} compact onChange={(n) => set({ usage: { recharge: { min: n, max: 6 } } })} />}
            {u && "per_day" in u && <NumericUpDown label="Times per day" value={u.per_day} min={1} max={9} compact onChange={(n) => set({ usage: { per_day: n } })} />}
            {u && "per_day_in_lair" in u && (
              <>
                <NumericUpDown label="Times per day" value={u.per_day_in_lair.normal} min={1} max={9} compact onChange={(n) => set({ usage: { per_day_in_lair: { ...u.per_day_in_lair, normal: n } } })} />
                <NumericUpDown label="Times per day in its lair" value={u.per_day_in_lair.lair} min={1} max={9} compact onChange={(n) => set({ usage: { per_day_in_lair: { ...u.per_day_in_lair, lair: n } } })} />
              </>
            )}
          </span>
        </div>
        {costs && (
          <div className="mg-f hb-field">
            <label htmlFor={`${ids}-cost`}>Costs (actions)</label>
            <span id={`${ids}-cost`}><NumericUpDown label="Costs (actions)" value={e.cost ?? 1} min={1} max={5} onChange={(n) => (n === 1 ? setE((x) => { const { cost: _c, ...rest } = x; return rest as HomebrewCreatureEntry; }) : set({ cost: n }))} /></span>
          </div>
        )}
        <div className="mg-f hb-field hb-bf-wide">
          <label htmlFor={`${ids}-text`}>Text<span className="hb-req" aria-hidden="true"> *</span></label>
          <textarea id={`${ids}-text`} className="hb-multiline" aria-invalid={(tried && !e.text.trim()) || undefined} value={e.text} onChange={(ev) => set({ text: ev.target.value })} />
          {e.attack && <button type="button" className="hb-pick-btn" onClick={() => e.attack && set({ text: attackText(e.attack, env.edition ?? "") })}><Icon name="pen" />Write the text from the attack</button>}
        </div>
        <div className="mg-f hb-field hb-bf-wide">
          <label><CheckBox checked={!!e.attack} onChange={(ev) => (ev.target.checked ? set({ attack: { kind: env.edition === "dnd2014" ? "melee_weapon" : "melee", to_hit: 3, reach: 5, damage: [newDamage()] } }) : drop("attack"))} />Attack</label>
          {e.attack && <AttackPart attack={e.attack} onChange={(attack) => set({ attack })} />}
        </div>
        <div className="mg-f hb-field hb-bf-wide">
          <label><CheckBox checked={!!e.save} onChange={(ev) => (ev.target.checked ? set({ save: { ability: "dex", dc: 12 } }) : drop("save"))} />Saving throw</label>
          {e.save && <SavePart save={e.save} onChange={(save) => set({ save })} />}
        </div>
        <div className="mg-f hb-field hb-bf-wide">
          <label><CheckBox checked={!!e.spellcasting} onChange={(ev) => (ev.target.checked ? set({ spellcasting: { lists: [] } }) : drop("spellcasting"))} />Spellcasting</label>
          {e.spellcasting && <SpellcastingPart value={e.spellcasting} onChange={(spellcasting) => set({ spellcasting })} />}
        </div>
        {problems.map((p, i) => <p key={i} className="hb-block-msg">{p}</p>)}
        {tried && missing && <p className="hb-error-line" role="alert">Give the {noun.toLowerCase()} a name and its text before OK.</p>}
      </div>
    </Dialog>
  );
}
