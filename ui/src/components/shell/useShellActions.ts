import { useCallback } from "react";
import { useNavigate } from "react-router";
import { tabForSlot } from "../../pages/builder/BuilderPage";
import { useSettings } from "../../state/SettingsContext";
import { useCharacter } from "../../state/CharacterContext";
import { isTauri } from "../../lib/tauri";
import type { ShellAction } from "./sections";
import { useShell } from "./ShellContext";

/** Selection-flow preference (auto-advance to the next choice): the single persisted app setting. */
export function useSelectionFlow(): [boolean, (v: boolean) => void] {
  const { settings, update } = useSettings();
  const set = useCallback((v: boolean) => { void update({ selectionFlow: v }); }, [update]);
  return [settings.selectionFlow, set];
}

/** Shared handlers for menu entries, quick bar, status bar and keyboard shortcuts. */
export function useShellActions() {
  const nav = useNavigate();
  const { snapshot, save, close } = useCharacter();
  const [flow, setFlow] = useSelectionFlow();
  const { openCompendium, setOptionsOpen } = useShell();
  const next = snapshot?.progress.nextSlotId ?? null;
  const nextTab = snapshot?.slots.find((s) => s.id === next)?.tab;
  const goNext = useCallback(() => {
    if (!next || !nextTab) return;
    const tab = tabForSlot(nextTab);
    nav(`${tab === "spells" ? "/spells" : `/builder/${tab}`}?slot=${encodeURIComponent(next)}`);
  }, [nav, next, nextTab]);
  const run = useCallback((a: ShellAction) => {
    if (a === "new") nav("/characters?new=1");
    else if (a === "compendium") openCompendium();
    else if (a === "options") { if (snapshot) setOptionsOpen(true); }
    else if (a === "save") void save();
    else if (a === "next") goNext();
    else if (a === "flow") setFlow(!flow);
    else if (a === "close") void close(false).then(() => nav("/characters"));
    else if (a === "exit" && isTauri()) void import("@tauri-apps/api/window").then((m) => m.getCurrentWindow().close());
  }, [nav, save, goNext, setFlow, flow, close, openCompendium, setOptionsOpen, snapshot]);
  return { run, flow, setFlow, hasNext: !!(next && nextTab), goNext, snapshot };
}
