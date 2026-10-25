import { screen } from "@testing-library/react";
import postcss from "postcss";
import { afterEach, describe, expect, it } from "vitest";
import { deviceShortSide, installPlatformClasses, isPhone, setPhoneForTests, type PlatformEnv } from "../lib/phone";
import { phoneLandscapeCss, scopeSelector } from "../lib/phoneLandscapeCss";
import { cssFiles, readCss } from "./cssRules";
import { renderApp } from "./utils";

// U9b: on an iOS or Android build the phone layout is chosen by the device's shortest side (< 600 CSS px), so a phone
// turned sideways keeps it; tablets keep the width rule; desktop builds never get it.
const IOS = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148";
const ANDROID = "Mozilla/5.0 (Linux; Android 15; Pixel 8 Build/AP3A; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/131.0.0.0 Mobile Safari/537.36";
const IPAD = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const WIN = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const env = (userAgent: string, o: Partial<PlatformEnv> = {}): PlatformEnv => ({ userAgent, maxTouchPoints: 0, search: "", isTauri: true, ...o });

/** A window of the given viewport on a device whose screen is the same size (what WebViews and device emulation report). */
const device = (width: number, height: number) => ({ screen: { width, height }, innerWidth: width, innerHeight: height });

/** The narrow-shell query `(max-width: 899px)` for a viewport width, flippable like a rotation. */
function widthMedia(width: number) {
  let w = width;
  const listeners = new Set<() => void>();
  const mq = { get matches() { return w <= 899; }, addEventListener: (_: string, l: () => void) => listeners.add(l), removeEventListener: (_: string, l: () => void) => listeners.delete(l) };
  return { matchMedia: () => mq, rotate: (next: number) => { w = next; listeners.forEach((l) => l()); }, listeners };
}

afterEach(() => { setPhoneForTests(false); document.documentElement.className = ""; });

describe("U9b phone-ness by the device's shortest side", () => {
  it("a Pixel 8 in landscape (915 x 412) keeps the phone layout", () => {
    const root = document.documentElement;
    const media = widthMedia(915);
    const shape = device(915, 412);
    installPlatformClasses(root, env(ANDROID), media.matchMedia, () => deviceShortSide(shape));
    expect(root.classList.contains("phone")).toBe(true);
    expect(root.classList.contains("android")).toBe(true);
    expect(isPhone()).toBe(true);
  });

  it("a 430 x 932 iPhone keeps the phone layout in both orientations", () => {
    const root = document.documentElement;
    const media = widthMedia(430);
    let shape = device(430, 932);
    installPlatformClasses(root, env(IOS), media.matchMedia, () => deviceShortSide(shape));
    expect(isPhone()).toBe(true);
    shape = device(932, 430);
    media.rotate(932);
    expect(root.classList.contains("phone")).toBe(true);
    expect(isPhone()).toBe(true);
  });

  it("an 820 x 1180 tablet keeps today's width rule: phone layout in portrait, wide shell in landscape", () => {
    const root = document.documentElement;
    const media = widthMedia(820);
    let shape = device(820, 1180);
    installPlatformClasses(root, env(IPAD, { maxTouchPoints: 5 }), media.matchMedia, () => deviceShortSide(shape));
    expect(root.classList.contains("ios")).toBe(true);
    expect(isPhone()).toBe(true);
    shape = device(1180, 820);
    media.rotate(1180);
    expect(root.classList.contains("phone")).toBe(false);
    expect(isPhone()).toBe(false);
  });

  it("a desktop build never gets html.phone, even in a 600 px window on a small screen", () => {
    const root = document.documentElement;
    const media = widthMedia(600);
    installPlatformClasses(root, env(WIN), media.matchMedia, () => deviceShortSide(device(600, 400)));
    expect(root.className).toBe("");
    expect(isPhone()).toBe(false);
    expect(media.listeners.size).toBe(0);
  });

  it("the shortest side comes from the screen, and from the viewport only when the screen reports nothing", () => {
    // The keyboard shrinks the viewport, never the screen: a tablet with the keyboard open stays a tablet.
    expect(deviceShortSide({ screen: { width: 1180, height: 820 }, innerWidth: 1180, innerHeight: 380 })).toBe(820);
    expect(deviceShortSide({ screen: { width: 393, height: 852 }, innerWidth: 852, innerHeight: 393 })).toBe(393);
    expect(deviceShortSide({ screen: { width: 0, height: 0 }, innerWidth: 915, innerHeight: 412 })).toBe(412);
    expect(deviceShortSide({ innerWidth: 915, innerHeight: 412 })).toBe(412);
  });
});

/** jsdom has no matchMedia: a wide viewport (landscape) answers the narrow query with false. */
function wideScreen() {
  const original = window.matchMedia;
  window.matchMedia = ((q: string) => ({
    matches: false, media: q, onchange: null, addEventListener: () => undefined, removeEventListener: () => undefined,
    addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false,
  })) as typeof window.matchMedia;
  return () => { window.matchMedia = original; };
}

describe("U9b the shell agrees with html.phone", () => {
  it("a phone in a wide landscape viewport shows the phone shell (title bar, bottom tabs)", async () => {
    const restore = wideScreen();
    try {
      setPhoneForTests(true);
      renderApp("/characters");
      expect(await screen.findByTestId("nav-start")).toBeTruthy();
      expect(document.querySelector(".aur-window.narrow")).toBeTruthy();
      expect(document.querySelector(".aur-bottombar")).toBeTruthy();
      expect(document.querySelector(".aur-title.mobile")).toBeTruthy();
      expect(document.querySelector(".aur-chrome")).toBeNull();
    } finally { restore(); }
  });

  it("a desktop window at the same width keeps the wide desktop shell", async () => {
    const restore = wideScreen();
    try {
      renderApp("/characters");
      expect(await screen.findAllByRole("button", { name: /new character/i })).toBeTruthy();
      expect(document.querySelector(".aur-window.wide")).toBeTruthy();
      expect(document.querySelector(".aur-bottombar")).toBeNull();
      expect(document.querySelector(".aur-title.mobile")).toBeNull();
    } finally { restore(); }
  });
});

const run = (css: string) => postcss([phoneLandscapeCss()]).process(css, { from: undefined }).css.replace(/\s+/g, " ").trim();

describe("U9b narrow-shell CSS for a phone wider than 899 px", () => {
  it("each narrow rule gets a phone-only twin for wider viewports, right after it and with the same specificity", () => {
    const out = run("@media (max-width: 899px) { .a, .b > .c { x: 1; } } .d { y: 2; }");
    expect(out).toBe("@media (max-width: 899px) { .a, .b > .c { x: 1; } } @media (min-width: 900px) { :where(html.phone) .a, :where(html.phone) .b > .c { x: 1; } } .d { y: 2; }");
  });

  it("a narrow rule that also names coarse pointers is bridged the same way", () => {
    expect(run("@media (pointer: coarse), (max-width: 899px) { .a { x: 1; } }")).toContain("@media (min-width: 900px) { :where(html.phone) .a { x: 1; } }");
  });

  it("wide-only rules (900 px and up) skip phones, so a landscape phone never gets the desktop split", () => {
    expect(run("@media (min-width: 900px) { .bp { x: 1; } }")).toBe("@media (min-width: 900px) { :where(html:not(.phone)) .bp { x: 1; } }");
    expect(run("@media (min-width: 900px) and (max-width: 1200px) { .r { x: 1; } }")).toBe("@media (min-width: 900px) and (max-width: 1200px) { :where(html:not(.phone)) .r { x: 1; } }");
  });

  it("a rule on the root itself takes the phone test in the root's own compound", () => {
    expect(run("@media (pointer: coarse), (max-width: 899px) { :root { --t: 44px; } html[data-x] .a { x: 1; } }"))
      .toContain("@media (min-width: 900px) { :root:where(.phone) { --t: 44px; } html:where(.phone)[data-x] .a { x: 1; } }");
    expect(scopeSelector(".html-like", ".phone")).toBe(":where(html.phone) .html-like");
  });

  it("other media rules and plain rules are left alone", () => {
    const css = "@media (max-width: 639px) { .a { x: 1; } } @media (hover: hover) and (pointer: fine) { .b:hover { y: 2; } } .c { z: 3; }";
    expect(run(css)).toBe(css);
  });

  it("every stylesheet of the app goes through it without error", () => {
    for (const file of cssFiles()) expect(() => run(readCss(file)), file).not.toThrow();
  });
});
