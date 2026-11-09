import { ABILITY_KEYS, type AbilityKey, type AbilityState, type SelectionSlot } from "../../api/contract";
import { ABILITY_LABEL } from "../../lib/format";
import { PaneShell } from "./DescriptionPane";
import { isListSlot, type PickChoice } from "./PickerGroup";

/**
 * Tester B5 (B-130, TV2): a background's ability-score options are list entries ("Intelligence +1, Charisma +2"), not
 * elements, so they had no description. On a phone, describing one shows what it does to each ability it raises.
 */
export type AbilityIncrease = Partial<Record<AbilityKey, number>>;

const BY_NAME = new Map(ABILITY_KEYS.map((k) => [ABILITY_LABEL[k].toLowerCase(), k]));
const PART = /^\s*([A-Za-z]+)\s*([+-]\d+)\s*$/;

/** The increases an option's text names ("Intelligence +1, Charisma +2"), or null when it is not an ability option. */
export function parseAbilityOption(text: string): AbilityIncrease | null {
  const out: AbilityIncrease = {};
  for (const part of text.split(",")) {
    const m = PART.exec(part);
    const key = m ? BY_NAME.get(m[1]!.toLowerCase()) : undefined;
    if (!m || !key) return null;
    out[key] = (out[key] ?? 0) + Number(m[2]);
  }
  return Object.keys(out).length > 0 ? out : null;
}

export interface AbilityOptionRow { ability: AbilityKey; name: string; from: number; to: number; modFrom: number; modTo: number }

const modifierOf = (score: number): number => Math.floor((score - 10) / 2);

/**
 * Each ability the option raises: its score without the slot's current pick (so every option compares from the same
 * scores) and with this option, capped at the character's maximum, and both modifiers.
 */
export function abilityOptionRows(a: Pick<AbilityState, "final" | "cap">, slot: SelectionSlot, text: string): AbilityOptionRow[] {
  const option = parseAbilityOption(text);
  if (!option) return [];
  const current = (slot.chosen && parseAbilityOption(slot.chosen.name)) || {};
  return ABILITY_KEYS.filter((k) => option[k]).map((k) => {
    const from = a.final[k] - (current[k] ?? 0);
    const to = Math.min(a.cap, from + option[k]!);
    return { ability: k, name: ABILITY_LABEL[k], from, to, modFrom: modifierOf(from), modTo: modifierOf(to) };
  });
}

/**
 * The described ability option of a Build tab: the list row's slot and text, from its Choose action or the slot that holds
 * it; null for an element, or a list entry that raises no ability (an alignment, an ideal), which keep the usual pane.
 */
export function describedAbilityOption(
  described: { id: string; isElement: boolean; choice?: PickChoice } | null,
  slots: SelectionSlot[],
): { slot: SelectionSlot; text: string } | null {
  if (!described || described.isElement) return null;
  const { choice } = described;
  const fromChoice = choice ? slots.find((s) => s.id === choice.slotId) : undefined;
  const holder = slots.find((s) => isListSlot(s) && s.chosen?.id === described.id);
  const found = fromChoice && isListSlot(fromChoice) ? { slot: fromChoice, text: choice!.name }
    : holder?.chosen ? { slot: holder, text: holder.chosen.name } : null;
  return found && parseAbilityOption(found.text) ? found : null;
}

const signed = (n: number): string => (n < 0 ? `−${-n}` : `+${n}`);

/** The pane for a described ability option, or null when the entry is not one (an alignment, an ideal). */
export function AbilityOptionPane({ abilities, slot, text }: { abilities: AbilityState; slot: SelectionSlot; text: string }) {
  const rows = abilityOptionRows(abilities, slot, text);
  if (rows.length === 0) return null;
  const intro = `An option of ${slot.title}. Your scores with it:`;
  const readText = `${text}. ${rows.map((r) => `${r.name} ${r.from} to ${r.to}, modifier ${signed(r.modFrom)} to ${signed(r.modTo)}`).join(". ")}.`;
  return (
    <PaneShell title={text} imageIcon={false} intro={intro} readText={readText}>
      <dl className="abil-opt">
        {rows.map((r) => (
          <div key={r.ability} className="abil-opt-row">
            <dt>{r.name}</dt>
            <dd className="abil-opt-score">{r.from} → {r.to}</dd>
            <dd className="abil-opt-mod">{signed(r.modFrom)} → {signed(r.modTo)}</dd>
          </div>
        ))}
      </dl>
    </PaneShell>
  );
}
