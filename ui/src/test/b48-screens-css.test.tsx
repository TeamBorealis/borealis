import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { SRC, cssFiles, declaredVars, parseRules, readCss } from "./cssRules";

// B-48 WP6/WP7 CSS contract (mobile-facelift-spec sec. 3.5, 7; O9, O14): the screen layer lives in phone files under
// html.phone (checked for every phone file in b48-phone-css.test.tsx), reads its colours from the theme tokens so the light
// theme is complete, and turns the data tables into one-line rows.
const SCREENS = ["styles/phone/bridge.css", "styles/phone/tables.css", "styles/phone/build.css", "styles/phone/screens-a.css"];
const rules = (files = SCREENS) => files.flatMap((f) => parseRules(readCss(f)).map((r) => ({ ...r, file: f })));
const bodyOf = (sel: string) => rules().filter((r) => r.selectors.includes(sel)).map((r) => r.body).join(";");

/** Desktop colour tokens that pages read directly; on a phone each must come from a --ph- role in both themes. */
const BRIDGED = ["--text", "--text-body", "--text-primary-header", "--text-secondary", "--text-muted", "--light-1", "--light-2", "--light-3", "--light-4", "--light-5",
  "--dark-1", "--dark-2", "--dark-3", "--dark-4", "--gray-1", "--gray-3", "--gray-5", "--gray-6", "--description-bg", "--flyout-bg", "--menu-bg", "--border", "--border-strong", "--input-border",
  "--gold", "--link", "--success", "--danger", "--warning"];

describe("phone screens CSS (B-48 WP6, WP7)", () => {
  it("has its own phone files", () => {
    expect(cssFiles()).toEqual(expect.arrayContaining(SCREENS));
  });

  it("loads after every desktop and earlier phone stylesheet: main.tsx imports the phone startup last, which imports these", () => {
    const main = readFileSync(join(SRC, "main.tsx"), "utf8");
    const lastCss = Math.max(...[...main.matchAll(/^import "\.\/styles\/[\w/.-]+\.css";$/gm)].map((m) => m.index!));
    expect(main.indexOf('from "./lib/phoneStartup"')).toBeGreaterThan(lastCss);
    const startup = readFileSync(join(SRC, "lib/phoneStartup.ts"), "utf8");
    const imported = [...startup.matchAll(/^import "\.\.\/(styles\/phone\/[\w.-]+\.css)";$/gm)].map((m) => m[1]!);
    expect(imported).toEqual(SCREENS);
  });

  it("bridges every desktop colour token to a phone role, strongly enough to win over the desktop light theme", () => {
    const bridge = rules(["styles/phone/bridge.css"]).filter((r) => r.atRules.length === 0);
    const vars = new Map<string, string>();
    bridge.forEach((r) => declaredVars(r.body).forEach((v, k) => vars.set(k, v)));
    const missing = BRIDGED.filter((k) => !/var\(--ph-/.test(vars.get(k) ?? ""));
    expect(missing).toEqual([]);
    // :root[data-theme="light"] is (0,2,0); the bridge needs an attribute on top of html.phone to override it.
    const sel = bridge.find((r) => declaredVars(r.body).has("--text"))!.selectors;
    expect(sel.some((s) => /^html\.phone:is\([^)]*\[data-theme\]/.test(s))).toBe(true);
  });

  it("reads colours only from tokens (no fixed colours that break one theme), apart from text on photos", () => {
    const fixed = rules(SCREENS.filter((f) => !f.endsWith("bridge.css"))).flatMap((r) =>
      [...r.body.matchAll(/(?:^|;)\s*([\w-]+)\s*:\s*([^;]*)/g)]
        .filter(([, prop, value]) => /color|background|border|fill|outline|shadow/.test(prop!) && /#[0-9a-f]{3,8}\b|rgba?\(/i.test(value!))
        .map(([, prop]) => `${r.file}: ${r.selectors.join()} { ${prop} }`));
    expect(fixed).toEqual([]);
  });

  it("O14: spells, selection slots, the catalogue and Sources lose their header row and read as rows with a detail line", () => {
    expect(bodyOf("html.phone .ap-grid thead")).toMatch(/display: none/);
    expect(bodyOf("html.phone .v4-grid-head")).toMatch(/display: none/);
    expect(bodyOf("html.phone .ap-grid tbody tr")).toMatch(/display: grid/);
    expect(bodyOf("html.phone .ap-grid tbody tr")).toMatch(/min-height: var\(--ph-row\)/);
    expect(bodyOf("html.phone .ph-row-detail")).toMatch(/font: [^;]*var\(--ph-t-foot\)/);
    expect(bodyOf("html.phone .ph-row-detail")).toMatch(/color: var\(--ph-text-2\)/);
  });

  it("Play keeps its details pane and Magic its concentration filter (U9: same elements)", () => {
    const all = rules();
    const hides = (re: RegExp) => all.filter((r) => r.selectors.some((s) => re.test(s)) && /display: none/.test(r.body));
    expect(hides(/\.v5-desc\b/)).toEqual([]);
    expect(hides(/\.sp-tog|\.sp-levels|\.sp-filter/)).toEqual([]);
  });
});
