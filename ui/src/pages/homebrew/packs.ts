import type { Ruleset } from "../../api/contract";

/** A pack's edition in the grid's words (IM17: a pack of both editions says so). */
export function editionLabel(rulesets: readonly Ruleset[]): string {
  if (rulesets.includes("dnd2014") && rulesets.includes("dnd2024")) return "Both editions";
  return rulesets.includes("dnd2014") ? "2014" : rulesets.includes("dnd2024") ? "2024" : "";
}

/** The pack worked on last in this session (File > New homebrew element... goes there). */
let last: string | null = null;

export function rememberPack(packId: string): void {
  last = packId;
}

export const lastPackId = (): string | null => last;
