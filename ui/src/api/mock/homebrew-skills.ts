import type { HomebrewForm } from "../types";
import { MOCK_BLOCK_CATALOG } from "./homebrew-catalog";

// The web mock's skills that content adds (HC1, HC16): a Skill element with an ability is a character skill keyed by its
// name (`borealis_engine::content_skills`). The creator's skill blocks offer their names after the eighteen
// (`borealis_authoring::form::skill_names`, the "Skill Name" picker). The backend reads loaded content; the mock reads the
// packs' saved elements (it has no Skill elements of its own).

/** The skill blocks' word picker (`borealis_authoring::blocks::SKILL_NAME_PICKER`). */
export const SKILL_NAME_PICKER = "Skill Name";

/** Statistics a skill can't be named like (the engine's built-in groups that are not skills, `content_skills::skill_key`). */
const NOT_SKILL_NAMES = ["strength", "dexterity", "constitution", "intelligence", "wisdom", "charisma", "hp", "level", "proficiency", "ac",
  "speed", "innate speed", "initiative", "ability checks"];

/** The six abilities by full name and abbreviation (`ids::ability_index`). */
const ABILITY_WORDS = ["strength", "dexterity", "constitution", "intelligence", "wisdom", "charisma"].flatMap((a) => [a, a.slice(0, 3)]);

/** The eighteen, lower case (the "Bonus to skills" block's choices). */
const EIGHTEEN: readonly string[] = (MOCK_BLOCK_CATALOG.find((b) => b.kind === "skill_bonus")?.fields.find((f) => f.key === "skills")?.choices ?? [])
  .map((c) => c.value.toLowerCase());

const singleSpaced = (s: string): string => s.trim().split(/\s+/).join(" ");

/** The statistic key of a skill named `name` (lower case, single spaced), or null for a name no skill can take. */
export function skillKey(name: string): string | null {
  const key = singleSpaced(name).toLowerCase();
  return key && !key.includes(":") && !NOT_SKILL_NAMES.includes(key) ? key : null;
}

/** The names of the skills saved Skill forms add, single spaced, each key once (the first wins), the eighteen left out, sorted. */
export function contentSkillNames(forms: readonly HomebrewForm[]): string[] {
  const byKey = new Map<string, string>();
  for (const f of forms) {
    if (f.kind !== "skill" || typeof f.fields.name !== "string") continue;
    const ability = typeof f.fields.ability === "string" ? f.fields.ability.trim().toLowerCase() : "";
    const key = skillKey(f.fields.name);
    if (!key || !ABILITY_WORDS.includes(ability) || EIGHTEEN.includes(key) || byKey.has(key)) continue;
    byKey.set(key, singleSpaced(f.fields.name));
  }
  return [...byKey.values()].sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
}
