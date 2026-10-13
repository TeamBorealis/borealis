import { type ReactNode, createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { AppSettings } from "../api/contract";
import { DEFAULT_SETTINGS } from "../api/mock/default-settings";
import { applyAccent } from "../lib/accent";
import { useApi } from "./ApiContext";

interface SettingsCtx {
  settings: AppSettings;
  /** False until the first `getSettings` answer arrived (the defaults are shown until then). */
  loaded: boolean;
  update: (patch: Partial<AppSettings>) => Promise<void>;
  /** Adopt settings returned by another command (e.g. `setAiOverride`). */
  replace: (settings: AppSettings) => void;
}

const Ctx = createContext<SettingsCtx | null>(null);

export function applyTheme(s: AppSettings): void {
  const root = document.documentElement;
  if (s.theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", s.theme);
  if (s.highContrast) root.setAttribute("data-contrast", "high");
  else root.removeAttribute("data-contrast");
  applyAccent(s.accentColor);
  root.setAttribute("data-row-height", s.rowHeight);
}

export function SettingsProvider({ children }: { children: ReactNode }) {
  const api = useApi();
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    let alive = true;
    api.getSettings().then((s) => { if (alive) { setSettings(s); setLoaded(true); } }).catch(() => alive && setLoaded(true));
    return () => { alive = false; };
  }, [api]);
  useEffect(() => applyTheme(settings), [settings]);
  const update = useCallback(async (patch: Partial<AppSettings>) => {
    setSettings(await api.saveSettings({ patch }));
  }, [api]);
  const value = useMemo(() => ({ settings, loaded, update, replace: setSettings }), [settings, loaded, update]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useSettings(): SettingsCtx {
  const v = useContext(Ctx);
  if (!v) throw new Error("useSettings must be used inside <SettingsProvider>");
  return v;
}
