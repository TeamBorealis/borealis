import { createContext, type ReactNode, useContext, useMemo } from "react";
import type {
  Api, HomebrewBlock, HomebrewBlockSpec, HomebrewForm, HomebrewNameScope, HomebrewPickerRow, HomebrewProblem, HomebrewStatGroup, Ruleset,
} from "../../api/contract";
import { useApi } from "../../state/ApiContext";
import { useAsync } from "../../state/useAsync";

/** How many loaded classes the class-name lists read (classes are few; one page holds them all). */
const CLASS_PAGE = 500;

/**
 * What every creator control may need beyond its own value (spec 5-6): the open draft (pickers list its unsaved
 * elements too), the block and statistic catalogs, the loaded classes (class names for conditions, formulas and spell
 * lists), the names in scope (pools, toggles, named choices...) and the expert switch (spec 6.4).
 */
export interface CreatorEnv {
  draftId: string | null;
  /** The open draft's pack (class options made from a choice block go there, spec 4.8). */
  packId?: string | null;
  /** Something wrote the pack outside the draft: the workspace's lists reload. */
  onPackChanged?: () => void;
  /** The element's edition when set, else the pack's single edition, else null (both). */
  edition: Ruleset | null;
  blocks: readonly HomebrewBlockSpec[];
  stats: readonly HomebrewStatGroup[];
  /** Loaded classes (own drafts included), by name. */
  classes: readonly HomebrewPickerRow[];
  /** The open form: names in scope are read from its blocks and children. */
  form: HomebrewForm | null;
  /** The form of the backend's last answer (its block sentences). */
  answer: HomebrewForm | null;
  problems: readonly HomebrewProblem[];
  expert: boolean;
}

const EMPTY_ENV: CreatorEnv = { draftId: null, packId: null, edition: null, blocks: [], stats: [], classes: [], form: null, answer: null, problems: [], expert: false };

const Ctx = createContext<CreatorEnv>(EMPTY_ENV);

export const useCreatorEnv = (): CreatorEnv => useContext(Ctx);

/** Classes the picker data offers; an answer the backend can't give yet (not implemented) is an empty list. */
async function loadClasses(api: Api, draftId: string | null): Promise<HomebrewPickerRow[]> {
  try {
    const page = await api.homebrewReferenceOptions({ target: { types: ["Class"] }, query: "", filters: {}, page: { offset: 0, limit: CLASS_PAGE }, ...(draftId ? { draftId } : {}) });
    return page.items;
  } catch {
    return [];
  }
}

const ignoreFailure = <T,>(p: Promise<T[]>): Promise<T[]> => p.catch(() => []);

export function CreatorEnvProvider({ draftId, packId = null, onPackChanged, edition, form, answer, problems, expert, children }: {
  draftId: string | null; packId?: string | null; onPackChanged?: () => void; edition: Ruleset | null; form: HomebrewForm | null; answer: HomebrewForm | null; problems: readonly HomebrewProblem[]; expert: boolean; children: ReactNode;
}) {
  const api = useApi();
  const blocks = useAsync(() => ignoreFailure(api.homebrewBlockCatalog()), [api]);
  const stats = useAsync(() => ignoreFailure(api.homebrewStatCatalog()), [api]);
  const classes = useAsync(() => loadClasses(api, draftId), [api, draftId]);
  const value = useMemo<CreatorEnv>(() => ({
    draftId, packId, edition, form, answer, problems, expert,
    ...(onPackChanged ? { onPackChanged } : {}),
    blocks: blocks.data ?? [], stats: stats.data ?? [], classes: classes.data ?? [],
  }), [draftId, packId, onPackChanged, edition, form, answer, problems, expert, blocks.data, stats.data, classes.data]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

/** Test and nesting helper: a fixed environment. */
export function CreatorEnvValue({ value, children }: { value: Partial<CreatorEnv>; children: ReactNode }) {
  return <Ctx.Provider value={{ ...EMPTY_ENV, ...value }}>{children}</Ctx.Provider>;
}

/** Every block of a list, nested toggle effects included. */
export function allBlocks(blocks: readonly HomebrewBlock[]): HomebrewBlock[] {
  return blocks.flatMap((b) => {
    const nested = Array.isArray(b.fields.effects) ? (b.fields.effects as unknown[]).filter(isRuleObject).map(ruleAsBlock) : [];
    return [b, ...allBlocks(nested)];
  });
}

const isRuleObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && typeof (v as { kind?: unknown }).kind === "string";

/** A nested rule object (a toggle's effect) seen as a block. */
export function ruleAsBlock(rule: Record<string, unknown>, i = 0): HomebrewBlock {
  const { kind, ...fields } = rule;
  return { key: `nested-${i}`, kind: String(kind), sentence: "", fields };
}

const str = (v: unknown): string => (typeof v === "string" ? v.trim() : "");

/** The block field that names a scope's entries, per block kind (`resource` pools are named by `id`). */
const NAMED_BY: Readonly<Record<string, readonly [HomebrewNameScope, string]>> = {
  resource: ["resource", "id"],
  toggle: ["toggle", "id"],
  select: ["select", "name"],
  list: ["select", "name"],
  select_feat: ["select", "name"],
  select_param: ["choice", "name"],
  extra_damage: ["extraDamage", "name"],
};

/** The names a `name` field can pick in `scope` (spec 6.2 "the picker lists what is in scope"): the draft's own, then loaded ones. */
export function namesInScope(env: CreatorEnv, scope: HomebrewNameScope): string[] {
  const own = allBlocks(env.form?.blocks ?? []).flatMap((b) => {
    const named = NAMED_BY[b.kind];
    return named && named[0] === scope ? [str(b.fields[named[1]])] : [];
  });
  const loaded = scope === "class" || scope === "spellList" ? env.classes.map((c) => c.name) : [];
  const children = scope === "feature" ? (env.form?.children ?? []).map((c) => str(c.fields.name)) : [];
  return [...new Set([...own, ...children, ...loaded].filter(Boolean))].sort((a, b) => a.localeCompare(b));
}

/** Class names (lower case, as conditions and formulas store them) with their shown names. */
export function classChoices(env: CreatorEnv): { value: string; label: string }[] {
  const seen = new Map<string, string>();
  for (const c of env.classes) if (!seen.has(c.name.toLowerCase())) seen.set(c.name.toLowerCase(), c.name);
  return [...seen].map(([value, label]) => ({ value, label })).sort((a, b) => a.label.localeCompare(b.label));
}

/** A lower-case class name in the words the user knows ("warlock" -> "Warlock"). */
export function className(env: CreatorEnv, lower: string): string {
  return env.classes.find((c) => c.name.toLowerCase() === lower.toLowerCase())?.name ?? titleWords(lower);
}

export const titleWords = (s: string): string => s.replace(/(^|[\s_-])([a-z])/g, (_m, sep: string, c: string) => `${sep === "_" ? " " : sep}${c.toUpperCase()}`);

/** The same environment with the expert switch set (a block dialog's own Expert toggle, spec 6.4). */
export function WithExpert({ expert, children }: { expert: boolean; children: ReactNode }) {
  const env = useCreatorEnv();
  const value = useMemo(() => ({ ...env, expert }), [env, expert]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
