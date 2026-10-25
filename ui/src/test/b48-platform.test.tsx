import { afterEach, describe, expect, it } from "vitest";
import { detectPhoneOs, installPlatformClasses, isPhone, resolveThemeAttr, setPhoneForTests } from "../lib/phone";

const IOS = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148";
const ANDROID = "Mozilla/5.0 (Linux; Android 15; Pixel 8 Build/AP3A; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/131.0.0.0 Mobile Safari/537.36";
const MAC = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const WIN = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const env = (o: Partial<Parameters<typeof detectPhoneOs>[0]>) => ({ userAgent: WIN, maxTouchPoints: 0, search: "", isTauri: true, ...o });

/** A matchMedia stand-in whose narrow answer the test flips. */
function fakeMedia(initial: boolean) {
  let matches = initial;
  const listeners = new Set<() => void>();
  const mq = { get matches() { return matches; }, addEventListener: (_: string, l: () => void) => listeners.add(l), removeEventListener: (_: string, l: () => void) => listeners.delete(l) };
  return { matchMedia: () => mq, set: (v: boolean) => { matches = v; listeners.forEach((l) => l()); }, listeners };
}

describe("phone platform classes (B-48 WP1, spec sec. 0)", () => {
  afterEach(() => { setPhoneForTests(false); document.documentElement.className = ""; });

  it("only iOS and Android builds count as phones; desktop builds never do", () => {
    expect(detectPhoneOs(env({ userAgent: IOS }))).toBe("ios");
    expect(detectPhoneOs(env({ userAgent: ANDROID }))).toBe("android");
    expect(detectPhoneOs(env({ userAgent: MAC, maxTouchPoints: 5 }))).toBe("ios"); // iPadOS reports a Mac with touch
    expect(detectPhoneOs(env({ userAgent: MAC }))).toBeNull();
    expect(detectPhoneOs(env({ userAgent: WIN }))).toBeNull();
  });

  it("a plain browser is a phone only with ?mock=mobile (the web e2e phone build)", () => {
    expect(detectPhoneOs(env({ isTauri: false, userAgent: IOS }))).toBeNull();
    expect(detectPhoneOs(env({ isTauri: false, userAgent: ANDROID, search: "?mock=mobile" }))).toBe("android");
    expect(detectPhoneOs(env({ isTauri: false, userAgent: WIN, search: "?mock=mobile" }))).toBe("ios");
    expect(detectPhoneOs(env({ isTauri: false, userAgent: WIN, search: "?mock=desktop" }))).toBeNull();
  });

  it("sets html.phone and the OS class on a narrow tablet-sized screen, and follows the narrow query", () => {
    const root = document.documentElement;
    const media = fakeMedia(true);
    const off = installPlatformClasses(root, env({ userAgent: ANDROID }), media.matchMedia);
    expect(root.classList.contains("phone")).toBe(true);
    expect(root.classList.contains("android")).toBe(true);
    expect(root.classList.contains("ios")).toBe(false);
    expect(isPhone()).toBe(true);
    media.set(false); // a tablet (no short side given) turned to a wide landscape gets the wide shell (U9b: phones keep it)
    expect(root.classList.contains("phone")).toBe(false);
    expect(isPhone()).toBe(false);
    media.set(true);
    expect(isPhone()).toBe(true);
    off();
    expect(media.listeners.size).toBe(0);
  });

  it("a desktop window dragged narrow never gets phone styling", () => {
    const root = document.documentElement;
    const media = fakeMedia(true);
    installPlatformClasses(root, env({ userAgent: WIN }), media.matchMedia);
    expect(root.className).toBe("");
    expect(isPhone()).toBe(false);
    expect(media.listeners.size).toBe(0);
  });

  it("O9: on a phone the System theme follows the phone's light or dark setting; desktop keeps today's default", () => {
    expect(resolveThemeAttr("system", { phone: true, prefersLight: true })).toBe("light");
    expect(resolveThemeAttr("system", { phone: true, prefersLight: false })).toBeNull();
    expect(resolveThemeAttr("system", { phone: false, prefersLight: true })).toBeNull();
    expect(resolveThemeAttr("dark", { phone: true, prefersLight: true })).toBe("dark");
    expect(resolveThemeAttr("light", { phone: false, prefersLight: false })).toBe("light");
  });
});
