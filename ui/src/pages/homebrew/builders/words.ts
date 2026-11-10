import type {
  HomebrewAbility, HomebrewBuilderProblem, HomebrewBuilt, HomebrewExprNode, HomebrewExprValue, HomebrewFormulaDialect, HomebrewFormulaNode,
  HomebrewGroupMode, HomebrewProblem, HomebrewSlot, HomebrewSlotTest,
} from "../../../api/contract";

export const ABILITIES: readonly HomebrewAbility[] = ["strength", "dexterity", "constitution", "intelligence", "wisdom", "charisma"];
export const ABILITY_WORDS: Readonly<Record<HomebrewAbility, string>> = {
  strength: "Strength", dexterity: "Dexterity", constitution: "Constitution", intelligence: "Intelligence", wisdom: "Wisdom", charisma: "Charisma",
};
export const ABILITY_CHOICES = ABILITIES.map((a) => ({ value: a, label: ABILITY_WORDS[a] }));

export const GROUP_MODES: readonly { value: HomebrewGroupMode; label: string }[] = [
  { value: "all", label: "All of" }, { value: "any", label: "Any of" }, { value: "none", label: "None of" },
];

export const SLOT_WORDS: Readonly<Record<HomebrewSlot, string>> = { armor: "Armor", shield: "Shield", mainHand: "Main hand", offHand: "Off hand" };

/** What each slot can be tested for (hc-wb02 Equipped). */
export const SLOT_TESTS: Readonly<Record<HomebrewSlot, readonly HomebrewSlotTest["kind"][]>> = {
  armor: ["any", "none", "light", "medium", "heavy", "item"],
  shield: ["any", "none", "item"],
  mainHand: ["any", "none", "weapon", "versatile", "item"],
  offHand: ["any", "none", "weapon", "item"],
};
export const TEST_WORDS: Readonly<Record<HomebrewSlotTest["kind"], string>> = {
  any: "Anything", none: "Nothing", light: "Light armor", medium: "Medium armor", heavy: "Heavy armor", weapon: "A weapon",
  versatile: "A versatile weapon held in two hands", item: "A specific item",
};

/** A new builder value: an empty All of group (writes nothing). The stored text, if any, travels unchanged. */
export function emptyGroup<R>(): HomebrewExprNode<R> {
  return { node: "group", mode: "all", items: [] };
}

/** The value as a model tree, or null while it is kept as is. */
export function modelOf<M>(v: HomebrewExprValue<M> | null | undefined): M | null {
  return v && v.value.shape === "model" ? v.value.model : null;
}

/** A builder value with a new model; the original text travels unchanged (hc-wb02: the backend decides what to write). */
export function withModel<M>(v: HomebrewExprValue<M> | null | undefined, model: M): HomebrewExprValue<M> {
  return { value: { shape: "model", model } as HomebrewBuilt<M>, ...(v?.original !== undefined ? { original: v.original } : {}) };
}

/** A form value that looks like a builder value. */
export function asExprValue<M>(v: unknown): HomebrewExprValue<M> | null {
  if (typeof v !== "object" || v === null) return null;
  const value = (v as { value?: { shape?: unknown } }).value;
  return value && (value.shape === "model" || value.shape === "kept") ? v as HomebrewExprValue<M> : null;
}

/** A group with no rows at any depth writes nothing (an empty condition). */
export function isEmptyTree<R>(n: HomebrewExprNode<R> | null | undefined): boolean {
  return !n || (n.node === "group" && n.items.every((i) => isEmptyTree(i)));
}

/** Builder problems of a field, relative to its model: `fields.requirements.items.1.min` -> `items.1.min`. */
export function relativeProblems(problems: readonly HomebrewProblem[], fieldPath: string): HomebrewBuilderProblem[] {
  const prefix = `${fieldPath}.`;
  return problems.filter((p) => p.severity !== "info" && p.path && p.path.startsWith(prefix)).map((p) => ({ path: p.path!.slice(prefix.length), message: p.message }));
}

/** Problems at a path or under it. */
export const problemsAt = (problems: readonly HomebrewBuilderProblem[], path: string): HomebrewBuilderProblem[] =>
  problems.filter((p) => p.path === path || p.path.startsWith(`${path}.`));

/* ---------- formulas ---------- */

export type FormulaTerm = HomebrewFormulaNode["term"];

/** The terms each dialect's reader reads (hc-wb02 "Formula dialects"); the builder offers only these. */
export const DIALECT_TERMS: Readonly<Record<HomebrewFormulaDialect, readonly FormulaTerm[]>> = {
  formula: ["number", "proficiency", "ability", "classLevel", "binary", "max", "min", "floor", "ceilDiv"],
  saveDc: ["number", "proficiency", "ability", "binary", "classSaveDc"],
  breathDc: ["number", "proficiency", "ability", "binary"],
  noteDc: ["number", "proficiency", "ability", "binary"],
  value: ["number", "proficiency", "halfProficiency", "ability"],
  valueOrClassLevel: ["number", "ability", "classLevel"],
};

/** Sums only: the DC dialects add terms and nothing else. */
export const SUM_ONLY: ReadonlySet<HomebrewFormulaDialect> = new Set(["saveDc", "breathDc", "noteDc"]);

export const TERM_WORDS: Readonly<Record<FormulaTerm, string>> = {
  number: "A number", proficiency: "Proficiency bonus", halfProficiency: "Half the proficiency bonus", ability: "Ability modifier",
  classLevel: "Class level", classSaveDc: "The class's spell save DC", binary: "Two parts combined", max: "The highest of",
  min: "The lowest of", floor: "Rounded down", ceilDiv: "Divided, rounded up",
};

export const OP_WORDS = { add: "plus", sub: "minus", mul: "times", div: "divided by" } as const;
export const OP_SIGNS = { add: "+", sub: "−", mul: "×", div: "÷" } as const;

/** A new node of a term, filled with sensible parts. */
export function newTerm(term: FormulaTerm, dialect: HomebrewFormulaDialect): HomebrewFormulaNode {
  switch (term) {
    case "number": return { term, value: SUM_ONLY.has(dialect) ? 8 : 1 };
    case "proficiency": case "halfProficiency": return { term };
    case "ability": return { term, ability: "wisdom" };
    case "classLevel": case "classSaveDc": return { term, class: "" };
    case "binary": return { term, op: "add", left: { term: "number", value: SUM_ONLY.has(dialect) ? 8 : 1 }, right: { term: "proficiency" } };
    case "max": case "min": return { term, args: [{ term: "number", value: 1 }, { term: "ability", ability: "wisdom" }] };
    case "floor": return { term, arg: { term: "number", value: 1 } };
    case "ceilDiv": return { term, num: { term: "classLevel", class: "" }, den: { term: "number", value: 2 } };
  }
}

export interface FormulaPreset { label: string; build: (cls: string) => HomebrewFormulaNode; dialects: readonly HomebrewFormulaDialect[] }

/** One-tap shapes (spec 6.3, hc-wb02 presets), per dialect. */
export const FORMULA_PRESETS: readonly FormulaPreset[] = [
  { label: "N × class level", dialects: ["formula"], build: (cls) => ({ term: "binary", op: "mul", left: { term: "number", value: 5 }, right: { term: "classLevel", class: cls } }) },
  { label: "Ability modifier (minimum 1)", dialects: ["formula"], build: () => ({ term: "max", args: [{ term: "number", value: 1 }, { term: "ability", ability: "wisdom" }] }) },
  { label: "8 + proficiency + ability", dialects: ["formula", "saveDc", "breathDc", "noteDc"], build: () => ({ term: "binary", op: "add", left: { term: "binary", op: "add", left: { term: "number", value: 8 }, right: { term: "proficiency" } }, right: { term: "ability", ability: "wisdom" } }) },
  { label: "Half class level, rounded up", dialects: ["formula"], build: (cls) => ({ term: "ceilDiv", num: { term: "classLevel", class: cls }, den: { term: "number", value: 2 } }) },
  { label: "Half class level, rounded down", dialects: ["formula"], build: (cls) => ({ term: "binary", op: "div", left: { term: "classLevel", class: cls }, right: { term: "number", value: 2 } }) },
];

/** A formula in words ("Wisdom modifier + proficiency bonus"); `cls` names a class in its shown spelling. */
export function formulaWords(n: HomebrewFormulaNode, cls: (lower: string) => string): string {
  const w = (x: HomebrewFormulaNode): string => formulaWords(x, cls);
  switch (n.term) {
    case "number": return String(n.value);
    case "proficiency": return "proficiency bonus";
    case "halfProficiency": return "half the proficiency bonus";
    case "ability": return `${ABILITY_WORDS[n.ability]} modifier`;
    case "classLevel": return n.class ? `${cls(n.class)} level` : "class level";
    case "classSaveDc": return n.class ? `${cls(n.class)} spell save DC` : "spell save DC";
    case "binary": return `${w(n.left)} ${OP_SIGNS[n.op]} ${n.right.term === "binary" ? `(${w(n.right)})` : w(n.right)}`;
    case "max": return `the highest of ${n.args.map(w).join(", ")}`;
    case "min": return `the lowest of ${n.args.map(w).join(", ")}`;
    case "floor": return `${w(n.arg)} rounded down`;
    case "ceilDiv": return `${w(n.num)} ÷ ${w(n.den)}, rounded up`;
  }
}
