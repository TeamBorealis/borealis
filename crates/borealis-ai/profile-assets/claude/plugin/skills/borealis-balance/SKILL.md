---
name: borealis-balance
description: Use when judging or tuning the power of new content (spells, feats, magic items, monsters, class features). Gives rough heuristics by level, rarity and challenge rating, and requires validate_element plus citing its flags.
---
# Balance heuristics

These are rules of thumb for sanity checks, not rules. Compare against real SRD elements first (`search_content`, `get_element`) and prefer the closest
existing example over any number below. Say "roughly" when you use a heuristic, and say which SRD element you compared with.

## Spells
- Cantrip: one-die damage that steps up at character levels 5, 11 and 17 (d8 -> 2d8 -> 3d8 -> 4d8 is typical for a single target). Utility cantrips have no scaling.
- Leveled single-target damage per slot level (average, before save): L1 about 10, L2 about 14, L3 about 25, L4 about 30, L5 about 35, L6 about 45, L7 about 50, L8 about 55, L9 about 60+.
  Area damage on a save is usually a little higher than single target because half damage on success and many targets are both factors; concentration or a rider lowers the number.
- Rule: if the spell adds a condition or control effect, cut the damage by roughly a third; if it is a bonus action or reaction, cut again.
- Upcasting adds about one extra damage die (or one extra target) per slot level.
- Healing: roughly the same dice as damage at the same level, but never more than one level up.
- Concentration is a cost; non-concentration persistent buffs should be weaker or shorter.
- Range 60 feet is the baseline; self, touch and 10 feet deserve a bonus.

## Feats
- A feat is worth about half an ability score increase of value. A half-feat gives +1 to an ability and one modest benefit, or a pure benefit with no increase.
- Origin feats (2024) must work at level 1 and have no prerequisite; keep them narrower than General feats. General feats need character level 4. Epic Boons are level 19+ and may raise a score cap to 30.
- Avoid stacking flat bonuses to attacks, AC and saves in a single feat. Prefer one flat bonus or one situational reaction.
- Limited-use effects (a `resource` of proficiency-bonus uses per long rest) are safer than at-will ones.

## Magic items
Common: flavour or minor utility. Uncommon: +1 weapon or armor-lite effects, a few charges of a level 1-3 spell. Rare: +2 weapon, +1 armor/shield, level 3-5 effects.
Very rare: +3 weapon, +2 armor, level 6-8 effects. Legendary: +3 armor, level 9 effects or campaign-shaping powers.
Attunement should come with the stronger half of each tier. Charges: a level-N spell costs about N charges; recharge at dawn is standard.
Do not give item bonuses to both attack and AC on the same item below very rare.

## Monsters
- Proficiency bonus by CR: 0-4 gives +2, 5-8 +3, 9-12 +4, 13-16 +5, 17-20 +6, 21-24 +7, 25-28 +8, 29-30 +9.
- Rough hit points: CR 1 about 70-85, CR 5 about 130-145, CR 10 about 205-220; AC climbs from about 13 at low CR to the high teens by CR 17+.
- Attack bonus ~ proficiency + ability modifier; save DC = 8 + proficiency + ability modifier. A single attack should land roughly 60-70 percent against a typical AC of its tier.
- Do not trust a hand-picked CR. Compute defensive and offensive averages, then check the resulting CR against 1-2 SRD monsters (`search_content` with type Monster).
- For encounter difficulty use `encounter_evaluate`; report its band and XP values, never invent them.

## Class features
Compare to the same-level feature of an SRD class. Level 1 features should be modest and mostly passive; capstones at 20 should be strong but not bypass the spell/resource economy.
Resource features with PB uses scale automatically; flat uses should rise at about levels 5/9/13/17.

## Required validation
Always run `validate_element` on the final content before answering. Quote every issue it reports (id, stage, message) in your reply and say how you fixed it, or
that it remains. If you used a heuristic, label it as a heuristic, and do not state numbers that no tool or SRD element supports as if they were facts.
