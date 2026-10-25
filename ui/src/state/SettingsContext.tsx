import { type ReactNode, createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { AppSettings } from "../api/contract";
import { DEFAULT_SETTINGS } from "../api/mock/default-settings";
import { applyAccent } from "../lib/accent";
import { isPhone, resolveThemeAttr, usePhone } from "../lib/phone";
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

const LIGHT_QUERY = "(prefers-color-scheme: light)";
const prefersLight = (): boolean => typeof window.matchMedia === "function" && window.matchMedia(LIGHT_QUERY).matches;

/** Phone status and navigation bars take the app background (spec sec. 5.9). */
function applyThemeColor(root: HTMLElement): void {
  let meta = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]');
  if (!meta) {
    meta = document.createElement("meta");
    meta.name = "theme-color";
    document.head.appendChild(meta);
  }
  meta.content = getComputedStyle(root).getPropertyValue("--ph-bg").trim() || "#11171a";
}

export function applyTheme(s: AppSettings): void {
  const root = document.documentElement;
  // B-48 O9: on a phone, "System" follows the phone's light or dark setting; desktop keeps the dark default.
  const theme = resolveThemeAttr(s.theme, { phone: isPhone(), prefersLight: prefersLight() });
  if (theme) root.setAttribute("data-theme", theme);
  else root.removeAttribute("data-theme");
  if (s.highContrast) root.setAttribute("data-contrast", "high");
  else root.removeAttribute("data-contrast");
  applyAccent(s.accentColor);
  root.setAttribute("data-row-height", s.rowHeight);
  if (isPhone()) applyThemeColor(root);
}

/** Re-applies the theme when the phone's light/dark setting changes while "System" is chosen (O9). */
function useSystemThemeFollow(s: AppSettings, phone: boolean): void {
  useEffect(() => {
    if (!phone || s.theme !== "system" || typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia(LIGHT_QUERY);
    const on = () => applyTheme(s);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, [s, phone]);
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
  const phone = usePhone();
  useEffect(() => applyTheme(settings), [settings, phone]);
  useSystemThemeFollow(settings, phone);
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
