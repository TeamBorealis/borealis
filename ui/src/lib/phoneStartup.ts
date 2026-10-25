import { installAndroidBack } from "./androidBack";
import { installKeyboardInset } from "./keyboardInset";
import { detectPhoneOs, installPlatformClasses, type PlatformEnv } from "./phone";
import { isTauri } from "./tauri";

/** A matchMedia that never matches, for WebViews without one. */
const noMedia = () => ({ matches: false, addEventListener: () => undefined, removeEventListener: () => undefined });

/**
 * B-48 startup (mobile-facelift-spec sec. 0, 5): platform classes, the keyboard inset and, on Android, the back stack.
 * Does nothing on desktop builds.
 */
export function startPhone(): void {
  const env: PlatformEnv = { userAgent: navigator.userAgent, maxTouchPoints: navigator.maxTouchPoints ?? 0, search: window.location.search, isTauri: isTauri() };
  const os = detectPhoneOs(env);
  if (!os) return;
  const root = document.documentElement;
  installPlatformClasses(root, env, typeof window.matchMedia === "function" ? (q) => window.matchMedia(q) : noMedia);
  installKeyboardInset(window, root);
  if (os === "android" && env.isTauri) {
    installAndroidBack((handler) => import("@tauri-apps/api/app").then((m) => m.onBackButtonPress(handler)));
  }
}
