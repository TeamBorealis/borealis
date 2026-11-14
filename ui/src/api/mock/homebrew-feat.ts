import type { HomebrewExprNode, HomebrewForm } from "../types";

// The web mock's stand-in for the feat form's display sentence (`borealis-authoring` `codec/feat_prereq.rs`): the text
// typed in "Prerequisite as shown", else the sentence the backend writes from the condition, as the 2024 SRD words it
// ("Level 4+, Strength or Dexterity 13+"). Pinned to the backend by `__fixtures__/homebrew-feat-prerequisites.json`
// (written by `crates/borealis-authoring/tests/mock_feat_prerequisites.rs`, Hm2aU).

type Row = Record<string, unknown>;
type Node = HomebrewExprNode<Row>;

const capital = (s: string): string => s.charAt(0).toUpperCase() + s.slice(1);

/** An element's name by id (the lookup the backend's sentence reads for "has" and "lacks" rows), or null. */
export type NameOf = (id: string) => string | null;

function rowPhrase(row: Row, nameOf: NameOf): string | null {
  const min = Number(row.min);
  switch (row.row) {
    case "characterLevel": case "level": return `Level ${min}+`;
    case "ability": return `${capital(String(row.ability))} ${min}+`;
    case "classLevel": return `${capital(String(row.class))} level ${min}+`;
    case "feature": return `${String(row.name ?? "").trim()} Feature`;
    case "spellcasting": return Number(row.level) <= 1 ? "Spellcasting Feature" : `Spell slots of level ${Number(row.level)}`;
    case "statistic": return `${capital(String(row.stat))} ${min}+`;
    case "has": return nameOf(String(row.id ?? ""));
    case "lacks": { const name = nameOf(String(row.id ?? "")); return name === null ? null : `No ${name}`; }
    default: return null;
  }
}

/** Every item an ability row, all with one minimum ("Strength or Dexterity 13+"). */
function abilitiesAny(items: readonly Node[]): string | null {
  const rows = items.map((i) => (i.node === "row" && i.row.row === "ability" ? i.row : null));
  if (rows.length === 0 || rows.some((r) => r === null)) return null;
  const mins = new Set(rows.map((r) => Number(r!.min)));
  if (mins.size !== 1) return null;
  return `${rows.map((r) => capital(String(r!.ability))).join(" or ")} ${[...mins][0]}+`;
}

function phrase(node: Node, nameOf: NameOf): string | null {
  if (node.node === "row") return rowPhrase(node.row, nameOf);
  if (node.items.length === 0) return null;
  if (node.items.length === 1 && node.mode !== "none") return phrase(node.items[0]!, nameOf);
  if (node.mode === "any") {
    const abilities = abilitiesAny(node.items);
    if (abilities) return abilities;
  }
  const parts = node.items.map((i) => phrase(i, nameOf));
  if (parts.some((p) => p === null)) return null;
  if (node.mode === "all") return parts.join(", ");
  return node.mode === "any" ? parts.join(" or ") : `Not ${parts.join(" or ")}`;
}

/** A feat form's prerequisite as shown in pickers and the description pane; `null` without one. */
export function featPrerequisite(form: HomebrewForm, nameOf: NameOf = () => null): string | null {
  if (form.kind !== "feat") return null;
  const typed = typeof form.fields.prerequisiteText === "string" ? form.fields.prerequisiteText.trim() : "";
  if (typed) return typed;
  const value = (form.fields.requirements as { value?: { shape?: string; model?: Node } } | undefined)?.value;
  return value?.shape === "model" && value.model ? phrase(value.model, nameOf) : null;
}
