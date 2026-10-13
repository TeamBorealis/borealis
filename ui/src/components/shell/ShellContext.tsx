import { type ReactNode, createContext, useCallback, useContext, useMemo, useState } from "react";
import { createPortal } from "react-dom";

/** What the bottom Compendium flyout opens with. */
/** `elementId` opens that entry's description directly (e.g. an AI citation). */
export interface CompendiumRequest { query?: string; source?: string; type?: string; elementId?: string }

interface ShellCtx {
  host: HTMLElement | null; setHost: (el: HTMLElement | null) => void; levelUpOpen: boolean; setLevelUpOpen: (v: boolean) => void;
  /** Character options flyout (F8). */
  optionsOpen: boolean; setOptionsOpen: (v: boolean) => void;
  /** Portraits gallery flyout (View > Portraits Gallery, Ctrl+P, portrait click). */
  portraitsOpen: boolean; setPortraitsOpen: (v: boolean) => void;
  /** Application Settings window: the open tab id, or null while closed (View > Application Settings, the cog, `/settings?tab=`). */
  settingsTab: string | null; openSettings: (tab?: string) => void; closeSettings: () => void;
  /** Borealis Settings window (View > Borealis Settings, AI pages). */
  borealisSettingsOpen: boolean; setBorealisSettingsOpen: (v: boolean) => void;
  /** Non-null while the Compendium flyout is open. */
  compendium: CompendiumRequest | null; openCompendium: (req?: CompendiumRequest) => void; closeCompendium: () => void;
}
const Ctx = createContext<ShellCtx>({
  host: null, setHost: () => undefined, levelUpOpen: false, setLevelUpOpen: () => undefined, optionsOpen: false, setOptionsOpen: () => undefined,
  portraitsOpen: false, setPortraitsOpen: () => undefined,
  settingsTab: null, openSettings: () => undefined, closeSettings: () => undefined, borealisSettingsOpen: false, setBorealisSettingsOpen: () => undefined,
  compendium: null, openCompendium: () => undefined, closeCompendium: () => undefined,
});

export function ShellProvider({ children }: { children: ReactNode }) {
  const [host, setHost] = useState<HTMLElement | null>(null);
  const [levelUpOpen, setLevelUpOpen] = useState(false);
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [portraitsOpen, setPortraitsOpen] = useState(false);
  const [settingsTab, setSettingsTab] = useState<string | null>(null);
  const [borealisSettingsOpen, setBorealisSettingsOpen] = useState(false);
  const openSettings = useCallback((tab = "general") => setSettingsTab(tab), []);
  const closeSettings = useCallback(() => setSettingsTab(null), []);
  const [compendium, setCompendium] = useState<CompendiumRequest | null>(null);
  const openCompendium = useCallback((req: CompendiumRequest = {}) => setCompendium(req), []);
  const closeCompendium = useCallback(() => setCompendium(null), []);
  const value = useMemo(() => ({ host, setHost, levelUpOpen, setLevelUpOpen, optionsOpen, setOptionsOpen, portraitsOpen, setPortraitsOpen, settingsTab, openSettings, closeSettings, borealisSettingsOpen, setBorealisSettingsOpen, compendium, openCompendium, closeCompendium }), [host, levelUpOpen, optionsOpen, portraitsOpen, settingsTab, openSettings, closeSettings, borealisSettingsOpen, compendium, openCompendium, closeCompendium]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export const useShell = (): ShellCtx => useContext(Ctx);

/** Renders `children` into the shell's sub-tab band (row 2). Falls back to inline when there is no shell. */
export function ShellSubTabs({ children }: { children: ReactNode }) {
  const { host } = useShell();
  return host ? createPortal(children, host) : <>{children}</>;
}
