import { createElement } from "react";
import { useConfirm } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";

/**
 * Commit a per-character source restriction (null = every source): preview what would be removed, confirm, then apply.
 * Same flow as manage/OptionsTab SourceRestrictions; can delete that copy once the Edition flyout drops the section.
 */
export function useApplySourceRestrictions() {
  const api = useApi();
  const confirm = useConfirm();
  const { snapshot, mutate } = useCharacter();
  return async (next: string[] | null): Promise<boolean> => {
    if (!snapshot) return false;
    const p = await api.previewSourceRestrictions({ characterId: snapshot.id, enabledSources: next });
    if (p.affected.length) {
      const body = createElement("ul", null, p.affected.map((a) => createElement("li", { key: a.slotId }, a.element)));
      const r = await confirm({ title: "Selections will be removed", body, confirmLabel: "Apply anyway", danger: true });
      if (r !== "confirm") return false;
    }
    await mutate((a, id) => a.setSourceRestrictions({ characterId: id, enabledSources: next }));
    return true;
  };
}

const DEFAULT_KEY = "borealis.defaultSources";

/** Saved default source selection (per-user convenience; null when none saved). */
export function loadDefaultSources(): string[] | null {
  try {
    const raw = localStorage.getItem(DEFAULT_KEY);
    const v: unknown = raw ? JSON.parse(raw) : null;
    return Array.isArray(v) && v.every((x) => typeof x === "string") ? (v as string[]) : null;
  } catch { return null; }
}

export function saveDefaultSources(ids: string[]): void {
  try { localStorage.setItem(DEFAULT_KEY, JSON.stringify(ids)); } catch { /* storage unavailable: nothing to save */ }
}
