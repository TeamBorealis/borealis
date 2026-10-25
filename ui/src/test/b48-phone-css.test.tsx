import { screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { SRC, cssFiles, declaredVars, parseRules, readCss } from "./cssRules";
import { renderApp } from "./utils";

// B-48 WP1-3 CSS contract (mobile-facelift-spec sec. 7 Test strategy): the phone layer lives under html.phone only, so the
// desktop (U7, Aurora-exact) shell cannot change; its tokens exist in both themes; toasts and the Compendium flyout respect
// the safe areas.
const PHONE = cssFiles().filter((f) => f.startsWith("styles/phone/"));
const phoneRules = () => PHONE.flatMap((f) => parseRules(readCss(f)).map((r) => ({ ...r, file: f })));
const COLOUR_ROLES = ["bg", "surface", "raised", "fill", "line", "field-line", "text", "text-2", "text-3", "accent", "on-accent", "accent-soft", "gold", "gold-soft", "danger", "danger-soft", "success", "scrim", "aurora"];

/** Variables declared by the rules whose selector is exactly `sel`. */
const varsOf = (sel: string) => {
  const out = new Map<string, string>();
  phoneRules().filter((r) => r.selectors.includes(sel) && r.atRules.length === 0).forEach((r) => declaredVars(r.body).forEach((v, k) => out.set(k, v)));
  return out;
};

describe("phone CSS layer (B-48)", () => {
  it("has its own files, imported after every desktop stylesheet", () => {
    expect(PHONE).toEqual(expect.arrayContaining(["styles/phone/tokens.css", "styles/phone/base.css", "styles/phone/shell.css", "styles/phone/overlays.css"]));
    const main = readFileSync(join(SRC, "main.tsx"), "utf8");
    const imports = [...main.matchAll(/^import "\.\/styles\/([\w/.-]+\.css)";$/gm)].map((m) => m[1]!);
    const firstPhone = imports.findIndex((f) => f.startsWith("phone/"));
    expect(firstPhone).toBeGreaterThan(0);
    expect(imports.slice(firstPhone).every((f) => f.startsWith("phone/"))).toBe(true);
    expect(imports.slice(firstPhone)).toEqual(["phone/tokens.css", "phone/base.css", "phone/shell.css", "phone/overlays.css"]);
  });

  it("every phone rule is scoped to html.phone (a desktop window at any width never matches)", () => {
    const loose = phoneRules().flatMap((r) => r.selectors.filter((s) => !/^html\.phone(?=[\s.:[>]|$)/.test(s)).map((s) => `${r.file}: ${s}`));
    expect(loose).toEqual([]);
  });

  it("no phone rule styles hover, uses all caps, or sets text under 11px", () => {
    const rules = phoneRules();
    expect(rules.filter((r) => r.selectors.some((s) => s.includes(":hover"))).map((r) => r.selectors.join())).toEqual([]);
    expect(rules.filter((r) => /text-transform:\s*uppercase/.test(r.body)).map((r) => r.selectors.join())).toEqual([]);
    const tiny = rules.flatMap((r) => [...r.body.matchAll(/font(?:-size)?:[^;]*?(\d+(?:\.\d+)?)px/g)].filter((m) => Number(m[1]) < 11).map(() => r.selectors.join()));
    expect(tiny).toEqual([]);
  });

  it("defines every colour role for the dark default and the light theme", () => {
    const dark = varsOf("html.phone");
    const light = varsOf('html.phone[data-theme="light"]');
    const missing = COLOUR_ROLES.flatMap((role) => [dark.has(`--ph-${role}`) ? [] : [`dark --ph-${role}`], light.has(`--ph-${role}`) ? [] : [`light --ph-${role}`]].flat());
    expect(missing).toEqual([]);
    // The spec's values (sec. 3.3).
    expect(dark.get("--ph-bg")).toBe("#11171a");
    expect(light.get("--ph-bg")).toBe("#f1f4f4");
    expect(dark.get("--ph-accent")).toBe("#6cc9b6");
    expect(light.get("--ph-accent")).toBe("#17766a");
    expect(dark.get("--ph-scrim")).toBe("rgb(4 8 10 / 0.56)");
  });

  it("Android gets its own sizes and the gesture-bar floor", () => {
    const android = varsOf("html.phone.android");
    expect(android.get("--ph-hit")).toBe("48px");
    expect(android.get("--ph-tabbar")).toBe("64px");
    expect(varsOf("html.phone").get("--ph-hit")).toBe("44px");
  });

  it("turns off the tap flash, rubber-band overscroll and long-press callouts on controls", () => {
    const html = phoneRules().find((r) => r.selectors.includes("html.phone") && /tap-highlight/.test(r.body));
    expect(html?.body).toMatch(/-webkit-tap-highlight-color: transparent/);
    expect(html?.body).toMatch(/overscroll-behavior: none/);
    const controls = phoneRules().find((r) => r.selectors.some((s) => s.startsWith("html.phone button")));
    expect(controls?.body).toMatch(/-webkit-touch-callout: none/);
    expect(controls?.body).toMatch(/touch-action: manipulation/);
  });

  it("toasts are placed with variables defined on the root, where the toast host renders (the --bottom-chrome bug)", () => {
    const toast = phoneRules().find((r) => r.selectors.includes("html.phone .toasts"));
    expect(toast?.body).toMatch(/bottom: calc\(/);
    const used = [...(toast?.body.match(/var\((--[\w-]+)/g) ?? [])].map((v) => v.slice(4));
    const root = new Map([...varsOf("html.phone"), ...varsOf(":root")]);
    // --safe-* come from the desktop tokens on :root; everything else must be defined on html.phone itself.
    const desktopRoot = new Map(parseRules(readCss("styles/tokens.css")).filter((r) => r.selectors.includes(":root") && r.atRules.length === 0).flatMap((r) => [...declaredVars(r.body)]));
    expect(used.filter((v) => !root.has(v) && !desktopRoot.has(v))).toEqual([]);
    expect(used).not.toContain("--bottom-chrome");
  });

  it("the toast host renders outside the shell window, so a variable set on .aur-window cannot reach it", async () => {
    renderApp("/characters");
    await screen.findAllByRole("button", { name: /new character/i });
    const toasts = document.querySelector(".toasts");
    expect(toasts).toBeTruthy();
    expect(toasts!.closest(".aur-window")).toBeNull();
  });

  it("the Compendium flyout keeps clear of the safe areas on phones (compendium.css zeroes the flyout padding)", () => {
    expect(readCss("pages/compendium/compendium.css")).toMatch(/\.flyout-overlay\.bottom \.flyout \{[^}]*padding: 0/);
    const overlay = phoneRules().find((r) => r.selectors.includes("html.phone .flyout-overlay.bottom"));
    expect(overlay?.body).toMatch(/padding: calc\(var\(--safe-t\) \+ 8px\) 0 0/);
    const sheet = phoneRules().find((r) => r.selectors.includes("html.phone .flyout-overlay.bottom .flyout"));
    expect(sheet?.body).toMatch(/padding-bottom: calc\(var\(--safe-b\) \+ var\(--kb\)\)/);
  });

  it("sheets keep their footer above the home indicator and the on-screen keyboard", () => {
    for (const sel of ["html.phone .dialog", "html.phone .flyout", "html.phone .aur-set-win"]) {
      const rule = phoneRules().find((r) => r.selectors.includes(sel) && /padding-bottom/.test(r.body));
      expect(rule?.body, sel).toMatch(/padding-bottom: calc\(var\(--safe-b\) \+ var\(--kb\)\)/);
    }
  });
});
