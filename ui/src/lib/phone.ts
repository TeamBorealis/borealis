import { useSyncExternalStore } from "react";
import type { ThemeMode } from "../api/contract";

/**
 * B-48 phone scope (mobile-facelift-spec sec. 0, U9b). The facelift layer applies only to iOS and Android builds (and the
 * web mock's `?mock=mobile`): on a phone always, in either orientation; on a tablet while the narrow shell is showing.
 * A desktop window at any width never gets it, so the desktop (Aurora-exact, U7) look cannot change.
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

/** U9b: a device whose shortest side is below this (CSS px) is a phone in either orientation; larger ones are tablets. */
export const PHONE_SHORT_SIDE = 600;

/** The parts of `window` the shortest side is read from. */
export interface DeviceShape {
  screen?: { width: number; height: number };
  innerWidth: number;
  innerHeight: number;
}

/**
 * The device's shortest side in CSS px. The screen is the source (WKWebView reports it in portrait, Android WebView and
 * device emulation per orientation; the minimum is the same either way, and the keyboard never shrinks it); the viewport
 * stands in only when the screen reports nothing.
 */
export function deviceShortSide(w: DeviceShape): number {
  const s = w.screen;
  if (s && s.width > 0 && s.height > 0) return Math.min(s.width, s.height);
  return Math.min(w.innerWidth, w.innerHeight);
}

/**
 * Adds `ios` / `android` once, and `phone` while the phone layout applies (U9b): always on a phone (shortest side under
 * 600 px, in either orientation), and on a tablet while the narrow shell shows. Returns a cleanup. On desktop it does nothing.
 */
export function installPlatformClasses(root: HTMLElement, env: PlatformEnv, matchMedia: MatchMedia, shortSide: () => number = () => Infinity): () => void {
  const found = detectPhoneOs(env);
  if (!found) return () => undefined;
  os = found;
  root.classList.add(found);
  const mq = matchMedia(NARROW_QUERY);
  const sync = () => {
    const next = shortSide() < PHONE_SHORT_SIDE || mq.matches;
    root.classList.toggle("phone", next);
    setPhone(next);
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
