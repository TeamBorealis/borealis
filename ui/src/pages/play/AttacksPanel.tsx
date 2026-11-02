import type { Attack, CharacterSnapshot } from "../../api/contract";
import { EmptyState, signed } from "../../components";
import { Expander } from "../../components/controls";
import { type DiceExpr, damageDice } from "../../lib/dice";
import { Tag } from "../ai/Panes";
import { ElementDescription } from "./PlaySplit";
import { D20Button, PressButton } from "./rolls/RollButton";
import { type DiceRequest, useRoller } from "./rolls/RollContext";

const critText = (a: Attack) => (a.critRange && a.critRange < 20 ? `Critical hit on ${a.critRange}-20` : null);

function DamageButton({ a, expr, offhand }: { a: Attack; expr: DiceExpr; offhand: DiceExpr | null }) {
  const roller = useRoller();
  const req = (e: DiceExpr, what: string): DiceRequest => ({ label: what, expr: e, floor: a.dieFloor });
  const main = req(expr, `${a.name} damage`);
  const items = [
    { label: "Normal", run: () => roller.dice(main) },
    { label: "Critical hit", run: () => roller.dice({ ...main, label: `${a.name} damage, critical`, critical: true }) },
    ...(offhand ? [
      { label: "Off-hand", run: () => roller.dice(req(offhand, `${a.name} off-hand damage`)) },
      { label: "Off-hand critical", run: () => roller.dice({ ...req(offhand, `${a.name} off-hand damage, critical`), critical: true }) },
    ] : []),
  ];
  return (
    <PressButton label={`Roll ${a.name} damage, ${a.damage}`} menuLabel={`Roll ${a.name} damage`} items={items} onTap={() => roller.dice(main)} className="pr-damage">
      {a.damage}
    </PressButton>
  );
}

function AttackRow({ a, selected, onSelect }: { a: Attack; selected: boolean; onSelect: (id: string) => void }) {
  const expr = damageDice(a.damage);
  const offhand = a.offhandDamage ? damageDice(a.offhandDamage) : null;
  const damage = expr ? { label: `${a.name} damage`, expr, floor: a.dieFloor } : null;
  const hasRoll = a.toHit !== null && a.toHit !== undefined;
  return (
    <li className={`v5-line pr-line pr-attack ${selected ? "is-selected" : ""}`}>
      <button type="button" className="v5-row grow pr-name" aria-current={selected} aria-label={`${a.name}, description`} onClick={() => onSelect(a.id)}>
        <span className="grow">{a.name}</span>
        {a.mastery?.mastered && <Tag tone="ok">{a.mastery.name}</Tag>}
        <span className="meta">{a.range}</span>
      </button>
      <span className="v5-ctrls pr-rolls">
        {hasRoll
          ? <D20Button what={`${a.name} attack`} modifier={a.toHit!} critRange={a.critRange} damage={damage} prefix="Hit" />
          : <span className="pr-static" aria-label={`${a.name}: ${a.attackBonus || "no attack roll"}`}>{a.attackBonus}</span>}
        {expr ? <DamageButton a={a} expr={expr} offhand={offhand} /> : <span className="pr-static">{a.damage}</span>}
      </span>
    </li>
  );
}

/** Attacks sub-tab (GitHub #1, #14): each row's name opens its description; Hit rolls the attack, the damage rolls damage. */
export function AttacksPanel({ s, selectedId, onSelect }: { s: CharacterSnapshot; selectedId: string | null; onSelect: (id: string) => void }) {
  const rows = s.attacks.filter((a) => a.showInCombat);
  return (
    <Expander title="Attacks" headerExtra={<span>{rows.length}</span>}>
      {rows.length === 0 ? <EmptyState title="No attacks">Add attacks on Manage &gt; Attacks &amp; Spellcasting, or equip a weapon.</EmptyState> : (
        <ul className="v5-list" aria-label="Attacks">
          {rows.map((a) => <AttackRow key={a.id} a={a} selected={a.id === selectedId} onSelect={onSelect} />)}
        </ul>
      )}
    </Expander>
  );
}

/** Description pane body for an attack: its numbers, then the weapon's own description when it is an item. */
export function AttackDetail({ s, attack }: { s: CharacterSnapshot; attack: Attack | undefined }) {
  if (!attack) return (<><h1 className="v5-desc-title">Attacks</h1><p className="v5-lead">Select an attack to read its details. Tap Hit or the damage to roll.</p></>);
  const item = s.inventory.items.find((i) => i.id === attack.id);
  const facts = [
    attack.toHit !== null && attack.toHit !== undefined ? `Attack roll ${signed(attack.toHit)}` : attack.attackBonus,
    `Damage ${attack.damage}`,
    attack.offhandDamage ? `Off-hand ${attack.offhandDamage}` : null,
    critText(attack),
    attack.dieFloor ? `Damage dice below ${attack.dieFloor} count as ${attack.dieFloor}` : null,
    attack.ammunition ? `Fires ${attack.ammunition}` : null,
  ].filter((x): x is string => !!x);
  return (
    <>
      <h1 className="v5-desc-title">{attack.name}</h1>
      <p className="v5-lead">{[attack.range, attack.auto ? "Computed from the character" : "Typed on Manage"].filter(Boolean).join(" · ")}</p>
      <ul className="pr-facts">{facts.map((f) => <li key={f}>{f}</li>)}</ul>
      {attack.mastery && <><h2>Mastery: {attack.mastery.name}</h2><p>{attack.mastery.text}</p></>}
      {attack.description && <p>{attack.description}</p>}
      {item && <><h2>{item.displayName}</h2><ElementDescription elementId={item.itemId} /></>}
    </>
  );
}
