import { installAndroidBack, tauriBackButton } from "./androidBack";
import { installAndroidInsets } from "./androidInsets";
import { installKeyboardInset } from "./keyboardInset";
import { detectPhoneOs, installPlatformClasses, type PlatformEnv } from "./phone";
import { isTauri } from "./tauri";
// B-48 WP6/WP7 phone screen layer (html.phone only). main.tsx imports this module after every stylesheet, so these load last.
import "../styles/phone/bridge.css";
import "../styles/phone/tables.css";
import "../styles/phone/build.css";
import "../styles/phone/screens-a.css";
import "../styles/phone/play-magic.css";
import "../styles/phone/gear.css";
import "../styles/phone/manage-sheet.css";

/** A matchMedia that never matches, for WebViews without one. */
const noMedia = () => ({ matches: false, addEventListener: () => undefined, removeEventListener: () => undefined });

/**
 * B-48 startup (mobile-facelift-spec sec. 0, 5): platform classes, the keyboard inset and, on Android, the system-bar insets
 * (HW1 A2) and the back stack. Does nothing on desktop builds. Returns a cleanup (tests).
 */
export function startPhone(): () => void {
  const env: PlatformEnv = { userAgent: navigator.userAgent, maxTouchPoints: navigator.maxTouchPoints ?? 0, search: window.location.search, isTauri: isTauri() };
  const os = detectPhoneOs(env);
  if (!os) return () => undefined;
  const root = document.documentElement;
  const cleanups = [
    installPlatformClasses(root, env, typeof window.matchMedia === "function" ? (q) => window.matchMedia(q) : noMedia),
    installKeyboardInset(window, root),
    ...(os === "android" && env.isTauri ? [installAndroidBack(tauriBackButton)] : []),
  ];
  // MainActivity calls this hook for the app's lifetime, so it stays installed.
  if (os === "android") installAndroidInsets();
  return () => cleanups.forEach((c) => c());
}
