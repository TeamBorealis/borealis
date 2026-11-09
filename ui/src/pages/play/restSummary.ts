import type { PlayState } from "../../api/contract";

/** Most changes a rest toast names before it says "and N more". */
const MAX_LISTED = 6;

type PlayRest = Pick<PlayState, "resources" | "spellSlots" | "pactSlots" | "trackers" | "hitPoints" | "hitDice" | "exhaustion" | "concentration" | "toggles" | "inspiration">;

const slotsUsed = (slots: readonly { used: number }[]): number => slots.reduce((n, s) => n + s.used, 0);

/** What came back or ended between two play states, one short phrase each ("Rage +1", "Concentration ended"). */
export function restChanges(before: PlayRest, after: PlayRest): string[] {
  const out: string[] = [];
  for (const r of after.resources) {
    const gained = r.current - (before.resources.find((b) => b.id === r.id)?.current ?? r.current);
    if (gained > 0) out.push(`${r.name} +${gained}`);
  }
  const slots = slotsUsed(before.spellSlots) - slotsUsed(after.spellSlots);
  if (slots > 0) out.push(`Spell slots +${slots}`);
  const pact = (before.pactSlots?.used ?? 0) - (after.pactSlots?.used ?? 0);
  if (pact > 0) out.push(`Pact Magic slots +${pact}`);
  for (const t of after.trackers) {
    const was = before.trackers.find((b) => b.id === t.id);
    if (!was || was.used === t.used) continue;
    out.push(t.countsUp ? `${t.name} cleared` : `${t.name} +${was.used - t.used}`);
  }
  const hp = after.hitPoints.current - before.hitPoints.current;
  if (hp > 0) out.push(`Hit points +${hp}`);
  const dice = slotsUsed(before.hitDice) - slotsUsed(after.hitDice);
  if (dice > 0) out.push(`Hit dice +${dice}`);
  const exhaustion = before.exhaustion.level - after.exhaustion.level;
  if (exhaustion > 0) out.push(`Exhaustion -${exhaustion}`);
  if (!before.inspiration && after.inspiration) out.push("Heroic Inspiration");
  for (const t of before.toggles.filter((x) => x.on)) {
    if (!after.toggles.find((a) => a.id === t.id)?.on) out.push(`${t.name} ended`);
  }
  if (before.concentration && !after.concentration) out.push("Concentration ended");
  return out;
}

/**
 * B-142 (tester video C9): a rest says what it did, so a rest with nothing to regain is not silent. "Short Rest: Rage +1.",
 * "Long Rest: Hit points +12, Spell slots +3.", "Short Rest: nothing to regain."
 */
export function restSummary(rest: "Short Rest" | "Long Rest", before: PlayRest, after: PlayRest): string {
  const changes = restChanges(before, after);
  if (changes.length === 0) return `${rest}: nothing to regain.`;
  const listed = changes.slice(0, MAX_LISTED).join(", ");
  const more = changes.length - MAX_LISTED;
  return `${rest}: ${listed}${more > 0 ? ` and ${more} more` : ""}.`;
}
