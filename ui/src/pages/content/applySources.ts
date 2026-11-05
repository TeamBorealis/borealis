import { createElement } from "react";
import type { Api, CharacterSnapshot } from "../../api/contract";
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
      // The engine never removes a stored pick: these stay chosen, only new choices are limited (manage.rs).
      const body = createElement("div", null,
        createElement("p", null, "These choices come from sources you are switching off. They stay on the character; only new choices are limited to the sources you keep."),
        createElement("ul", null, p.affected.map((a) => createElement("li", { key: a.slotId }, a.element))));
      const r = await confirm({ title: "Picks from these sources stay", body, confirmLabel: "Apply" });
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

/**
 * The saved default as a restriction over the sources installed now (GitHub #22): unknown ids are dropped, and it is
 * null (every source) when nothing is saved, when none of the saved sources is installed any more, or when it allows them all.
 */
export function defaultRestriction(saved: readonly string[] | null, installed: readonly string[]): string[] | null {
  if (!saved) return null;
  const known = new Set(installed);
  const kept = [...new Set(saved)].filter((id) => known.has(id));
  if (saved.length > 0 && kept.length === 0) return null;
  return kept.length === known.size ? null : kept;
}

/** A new character starts on the saved default sources (Aurora's "apply default restrictions on new character"). */
export async function applyDefaultSources(api: Api, snap: CharacterSnapshot): Promise<CharacterSnapshot> {
  const saved = loadDefaultSources();
  if (!saved) return snap;
  const enabled = defaultRestriction(saved, (await api.listSources()).map((s) => s.id));
  return enabled ? api.setSourceRestrictions({ characterId: snap.id, enabledSources: enabled }) : snap;
}
