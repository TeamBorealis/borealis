# Borealis plugin skills

Skills shipped in the Borealis AI profile (`claude/plugin`). They are laid down read-only with the rest of the profile and verified by the profile manifest (SHA-256 of every file, computed from the directory contents; adding a skill needs no list update).

| Skill | Purpose |
|---|---|
| `borealis-content-authoring` | Element model, rule kinds, IDs, supports, progression tables, description HTML, checklist. |
| `borealis-class-and-subclass` | Full 1-20 classes, spellcasting, multiclass node, subclasses, `class_sweep` loop. |
| `borealis-editions` | 2014 vs 2024 authoring differences, `ruleset`, `maps_to`. |
| `borealis-balance` | Power heuristics and the validate-and-cite rule. |
| `borealis-tools` | When and how to use each `mcp__borealis__*` tool. |

Authoring rules for maintainers: write guidance in original words, cite SRD concepts only, keep each SKILL.md under 250 lines, and re-check examples against `app/content/` when engine rule kinds change.
