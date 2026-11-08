import { useMemo } from "react";
import type { AbilityKey, CharacterSnapshot, ElementSummary } from "../../api/contract";
import { Button, Expander, PrereqUnmetMark } from "../../components";
import { haptic } from "../../lib/haptics";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useAsync } from "../../state/useAsync";
import type { SplitRow } from "./LevelSplit";

const ABILITY_BY_NAME: Record<string, AbilityKey> = {
  strength: "str", dexterity: "dex", constitution: "con", intelligence: "int", wisdom: "wis", charisma: "cha",
  str: "str", dex: "dex", con: "con", int: "int", wis: "wis", cha: "cha",
};

interface ParsedPrerequisite { names: string[]; score: number; joiner: "or" | "and" }

/** Parses "Strength or Dexterity 13", "Strength 13 or Dexterity 13" and "Wisdom 13"; null when the text is not an ability rule. */
export function parsePrerequisite(text: string): ParsedPrerequisite | null {
  const parts = text.replace(/^prerequisites?:\s*/i, "").split(/\s+(or|and)\s+|,\s*/i).filter((p): p is string => !!p);
  const scores = parts.map((p) => /(\d+)\s*$/.exec(p)?.[1]).filter(Boolean).map(Number);
  if (scores.length === 0) return null;
  const names = parts.map((p) => p.replace(/\s*\d+\s*$/, "").trim()).filter((p) => ABILITY_BY_NAME[p.toLowerCase()]);
  if (names.length === 0) return null;
  return { names, score: Math.max(...scores), joiner: /\sand\s/i.test(text) ? "and" : "or" };
}

/** Classic builder wording: every ability carries its own score, "Strength 13 or Dexterity 13". */
export function formatPrerequisite(text: string): string {
  const p = parsePrerequisite(text);
  return p ? p.names.map((n) => `${n} ${p.score}`).join(` ${p.joiner} `) : text;
}

export function meetsPrerequisite(text: string | null, finals: Record<AbilityKey, number>): boolean {
  const p = text ? parsePrerequisite(text) : null;
  if (!p) return true;
  const ok = p.names.map((n) => finals[ABILITY_BY_NAME[n.toLowerCase()]!] >= p.score);
  return p.joiner === "and" ? ok.every(Boolean) : ok.some(Boolean);
}

interface Row { id: string; name: string; multiId: string | null; prerequisites: string | null }

/** Classes the character does not have yet, each with its multiclass prerequisite (from the compendium's multiclass entry). */
function useMulticlassRows(snapshot: CharacterSnapshot): Row[] {
  const api = useApi();
  const have = useMemo(() => new Set(snapshot.level.classes.map((c) => c.classId).filter(Boolean)), [snapshot.level.classes]);
  const rows = useAsync<Row[]>(async () => {
    const q = { query: "", ruleset: snapshot.ruleset, limit: 100, offset: 0 } as const;
    const classes = (await api.searchCompendium({ query: { ...q, types: ["class"] } })).items;
    return Promise.all(classes.map(async (c: ElementSummary): Promise<Row> => {
      // A class's multiclass entry is generated from it, so it shares its source: a same-name class of another source
      // (an imported twin) must not take that one's prerequisite.
      const multi = (await api.searchCompendium({ query: { ...q, query: c.name, types: ["multiclass"] } })).items.find((m) => m.name === c.name && m.source === c.source);
      return { id: c.id, name: c.name, multiId: multi?.id ?? null, prerequisites: multi?.prerequisites ?? null };
    }));
  }, [api, snapshot.ruleset]);
  return (rows.data ?? []).filter((r) => !have.has(r.id));
}

interface MulticlassExpanderProps {
  snapshot: CharacterSnapshot;
  onAdded?: () => void;
  /** B2a: MOVE LEVELS, a multiclass of the row's class made from the main class's levels (the total level stays). */
  onSplit?: (row: SplitRow) => void;
}

/** Bottom expander of the Level up & multiclassing flyout: one block per eligible class, name, prerequisite in green italics, MOVE LEVELS and NEW MULTICLASS. */
export function MulticlassExpander({ snapshot, onAdded, onSplit }: MulticlassExpanderProps) {
  const { mutate } = useCharacter();
  const rows = useMulticlassRows(snapshot);
  const lv = snapshot.level;
  const ignorePrerequisites = snapshot.options.ignoreMulticlassPrerequisites;
  const mainLevel = lv.classes.find((c) => c.isMain)?.level ?? 0;
  /** Classic builder: NEW MULTICLASS only adds the empty class slot; the class is picked from the Multiclass picker on Build > Class. */
  const add = async () => {
    haptic("levelUp");
    const next = await mutate((api, id) => api.addMulticlass({ characterId: id }));
    if (next) onAdded?.();
  };
  return (
    <div className="lvl-multi">
      <Expander flyout title="Multiclassing">
        <p className="lvl-multi-text">
          Multiclassing allows you to gain levels in multiple classes. In order to qualify for a new class, you must meet the ability score prerequisites for both your current class and your new one.
        </p>
        {lv.multiclassBlockedReason && <p className="lvl-multi-text blocked" role="status">{lv.multiclassBlockedReason}</p>}
        {rows.map((r) => {
          const met = meetsPrerequisite(r.prerequisites, snapshot.abilities.final);
          // HR3: the "Ignore multiclass prerequisites" house rule lets an unmet class through, marked
          const overridden = !met && ignorePrerequisites;
          return (
            <div className="lvl-mc-row" key={r.id} aria-label={`${r.name} multiclass`}>
              <div>
                <h4>{r.name}{overridden && <> <PrereqUnmetMark /></>}</h4>
                {r.prerequisites && <p className="lvl-prereq">Prerequisites: {formatPrerequisite(r.prerequisites)}</p>}
              </div>
              <div className="lvl-mc-actions">
                {onSplit && (
                  <Button disabled={!lv.canMulticlass || !(met || overridden) || mainLevel < 2} aria-label={`Move levels to ${r.name}`}
                    title={met || overridden ? `Make some of your levels ${r.name} levels. Your level stays the same.` : "You do not meet the prerequisite."}
                    onClick={() => onSplit({ name: r.name, multiId: r.multiId })}>Move levels</Button>
                )}
                <Button disabled={!lv.canMulticlass || !(met || overridden)} aria-label={`New multiclass ${r.name}`} title={met || overridden ? undefined : "You do not meet the prerequisite."}
                  onClick={() => void add()}>New multiclass</Button>
              </div>
            </div>
          );
        })}
      </Expander>
    </div>
  );
}
