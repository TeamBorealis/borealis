/**
 * B-48 (mobile-facelift-spec sec. 5.9, O9) on Android. MainActivity installs `window.BorealisSystemBars` (SystemBars.kt);
 * everywhere else (desktop, iOS, browsers, the web mock) it is absent and these do nothing.
 *
 * - The status and navigation bar icons follow the app's effective theme, including a Settings > Appearance choice that
 *   differs from the phone's (`applySystemBars`).
 * - The phone's dark mode comes from Android itself (`bridgeSystemNight`): the System WebView keeps answering
 *   `prefers-color-scheme: light` with the phone in dark mode, and the activity handles `uiMode` changes itself, so
 *   MainActivity sends `SYSTEM_THEME_EVENT` when the setting changes.
 */
interface SystemBarsBridge { setLight?: (light: boolean) => void; isSystemNight?: () => boolean }
type BarsWindow = Window & { BorealisSystemBars?: SystemBarsBridge };

/** Fired on `window` by MainActivity when the phone's dark mode changes. */
export const SYSTEM_THEME_EVENT = "borealis-system-theme";

const bridgeOf = (w: Window): SystemBarsBridge | undefined => (w as BarsWindow).BorealisSystemBars;

/** Dark bar icons for a light page (`light`), light icons for a dark one. */
export function applySystemBars(light: boolean, w: Window = window): void {
  const bridge = bridgeOf(w);
  if (!bridge || typeof bridge.setLight !== "function") return;
  try {
    bridge.setLight(light);
  } catch {
    // A failed bridge call leaves the bars as they were; the page itself is already themed.
  }
}

/** Android's own dark-mode answer, or null where there is no bridge (then `prefers-color-scheme` decides). */
export function bridgeSystemNight(w: Window = window): boolean | null {
  const bridge = bridgeOf(w);
  if (!bridge || typeof bridge.isSystemNight !== "function") return null;
  try {
    return bridge.isSystemNight() === true;
  } catch {
    return null;
  }
}
