import { setNativeKeyboardInset } from "./keyboardInset";

/**
 * Android system bars for the safe areas (HW1 A2). MainActivity reports the status bar, navigation bar and
 * display cutout in CSS px; tokens.css takes the larger of those and `env(safe-area-inset-*)`, which on
 * Android only covers the cutout (0 at the top in landscape, under a visible status bar).
 */
const SIDES = [["t", "--sys-inset-t"], ["r", "--sys-inset-r"], ["b", "--sys-inset-b"], ["l", "--sys-inset-l"]] as const;
/** Larger than any system bar; anything above is not an inset. */
const MAX_INSET_PX = 400;
/** Larger than any on-screen keyboard (WP8: the IME inset `k` is the keyboard height). */
const MAX_KEYBOARD_PX = 2000;

interface InsetsBridge { get: () => string }
type InsetsWindow = Window & { BorealisInsets?: InsetsBridge; __borealisApplyInsets?: (raw: unknown) => void };

const isPx = (v: unknown, max: number): v is number => typeof v === "number" && Number.isFinite(v) && v >= 0 && v <= max;
const isInset = (v: unknown): v is number => isPx(v, MAX_INSET_PX);

/**
 * Apply `{t, r, b, l}` (CSS px) to the root, and `k` (the keyboard, WP8) to `--kb`; values that are not small
 * non-negative numbers are ignored.
 */
export function applyInsets(raw: unknown, root: HTMLElement = document.documentElement): void {
  if (!raw || typeof raw !== "object") return;
  const insets = raw as Record<string, unknown>;
  for (const [key, name] of SIDES) {
    const v = insets[key];
    if (isInset(v)) root.style.setProperty(name, `${Math.round(v)}px`);
  }
  if (isPx(insets.k, MAX_KEYBOARD_PX)) setNativeKeyboardInset(insets.k);
}

function parse(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null; // a malformed answer leaves env() in charge
  }
}

/** Install the hook MainActivity calls on every change and read the current insets once. */
export function installAndroidInsets(): void {
  const w = window as InsetsWindow;
  w.__borealisApplyInsets = (raw) => applyInsets(raw);
  const bridge = w.BorealisInsets;
  if (bridge && typeof bridge.get === "function") applyInsets(parse(bridge.get()));
}
