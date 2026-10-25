import { useSyncExternalStore } from "react";
import type { ThemeMode } from "../api/contract";

/**
 * B-48 phone scope (mobile-facelift-spec sec. 0). The facelift layer applies only to iOS and Android builds (and the web
 * mock's `?mock=mobile`), and only while the narrow shell is showing. A desktop window at any width never gets it, so the
 * desktop (Aurora-exact, U7) look cannot change.
 */
export type PhoneOs = "ios" | "android";

export interface PlatformEnv {
  userAgent: string;
  maxTouchPoints: number;
  /** `location.search` */
  search: string;
  isTauri: boolean;
}

/** Same breakpoint as the narrow shell (`useNarrow`). */
const NARROW_QUERY = "(max-width: 899px)";

/** The phone OS of this build, or null on desktop builds and plain browsers. */
export function detectPhoneOs(env: PlatformEnv): PhoneOs | null {
  const android = /Android/i.test(env.userAgent);
  const ios = /iPhone|iPad|iPod/i.test(env.userAgent) || (env.maxTouchPoints > 1 && /Mac/.test(env.userAgent));
  if (env.isTauri) return android ? "android" : ios ? "ios" : null;
  if (new URLSearchParams(env.search).get("mock") !== "mobile") return null;
  return android ? "android" : "ios";
}

let phone = false;
let os: PhoneOs | null = null;
const listeners = new Set<() => void>();
function setPhone(next: boolean): void {
  if (next === phone) return;
  phone = next;
  listeners.forEach((l) => l());
}

/** True while the phone facelift is active (`html.phone`). */
export const isPhone = (): boolean => phone;

export function subscribePhone(listener: () => void): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

/** React view of `isPhone()`. */
export function usePhone(): boolean {
  return useSyncExternalStore(subscribePhone, isPhone, isPhone);
}

/** The phone OS once the platform classes are installed (iOS and Android builds, the web mock), else null. */
export const phoneOs = (): PhoneOs | null => os;

/** Tests pick a phone OS without a real platform. */
export function setPhoneOsForTests(next: PhoneOs | null): void {
  os = next;
}

/** Tests switch the phone behaviour on without a real platform. */
export function setPhoneForTests(next: boolean): void {
  setPhone(next);
}

/** The part of `MediaQueryList` the class setup uses. */
export interface NarrowQuery {
  readonly matches: boolean;
  addEventListener(type: "change", listener: () => void): void;
  removeEventListener(type: "change", listener: () => void): void;
}
export type MatchMedia = (query: string) => NarrowQuery;

/**
 * Adds `ios` / `android` once, and `phone` while the narrow shell shows. Returns a cleanup. On desktop it does nothing.
 */
export function installPlatformClasses(root: HTMLElement, env: PlatformEnv, matchMedia: MatchMedia): () => void {
  const found = detectPhoneOs(env);
  if (!found) return () => undefined;
  os = found;
  root.classList.add(found);
  const mq = matchMedia(NARROW_QUERY);
  const sync = () => {
    root.classList.toggle("phone", mq.matches);
    setPhone(mq.matches);
  };
  sync();
  mq.addEventListener("change", sync);
  return () => mq.removeEventListener("change", sync);
}

/** The phone's light or dark setting (`prefers-color-scheme`); false where the WebView cannot tell. */
export function systemPrefersLight(): boolean {
  return typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia("(prefers-color-scheme: light)").matches;
}

/** The `data-theme` value for a theme setting; null leaves the default (dark). O9: "System" follows the phone's setting. */
export function resolveThemeAttr(mode: ThemeMode, o: { phone: boolean; prefersLight: boolean }): "light" | "dark" | null {
  if (mode !== "system") return mode;
  return o.phone && o.prefersLight ? "light" : null;
}
