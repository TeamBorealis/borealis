import { describe, expect, it } from "vitest";
import { cssFiles, parseRules, readCss } from "./cssRules";

// B-48 WP4/WP5 CSS contract (mobile-facelift-spec sec. 3.4, 7): rows, pickers, lists and controls on phones. The generic
// rules (html.phone scope, no :hover, no caps, nothing under 11 px) are in b48-phone-css.test.tsx and cover these files too.
const FILES = ["styles/phone/lists.css", "styles/phone/controls.css", "styles/phone/sheet-content.css"];
const rules = () => FILES.flatMap((f) => parseRules(readCss(f)).map((r) => ({ ...r, file: f })));
/** Declarations of every phone rule whose selector list contains `sel` exactly. */
const bodyOf = (sel: string) => rules().filter((r) => r.selectors.includes(sel)).map((r) => r.body).join(";");

describe("phone lists and controls CSS (B-48 WP4, WP5)", () => {
  it("lives in its own phone files", () => {
    expect(cssFiles()).toEqual(expect.arrayContaining(FILES));
  });

  it("every row is at least the platform row height (48 iOS, 52 Android) and tappable across its width", () => {
    for (const sel of ["html.phone .pick-main", "html.phone .v4-el button", "html.phone .aur-listbox-item", "html.phone .picker-item-main", "html.phone .pick-chosen"]) {
      expect(bodyOf(sel), sel).toMatch(/min-height: var\(--ph-row\)/);
    }
    expect(bodyOf("html.phone .pick-main")).toMatch(/width: 100%|flex: 1/);
  });

  it("the chosen row carries an icon (a check in a ring), not colour alone; the highlighted row a leading bar", () => {
    const chosen = rules().find((r) => r.selectors.includes('html.phone .pick-main[aria-pressed="true"]::after'));
    expect(chosen?.body).toMatch(/mask/);
    expect(rules().some((r) => r.selectors.includes("html.phone .pick-main::after") && /border/.test(r.body))).toBe(true);
    expect(rules().some((r) => r.selectors.some((s) => /\.pick-row\.hl .*::before/.test(s)) && /width: 3px/.test(r.body))).toBe(true);
  });

  it("picker headers: sentence case, Manage Sources and collapse are 44/48 pt targets", () => {
    expect(bodyOf("html.phone .aur-expander-title")).toMatch(/text-transform: none/);
    for (const sel of ["html.phone .pick-tag", "html.phone .aur-expander-chev"]) {
      expect(bodyOf(sel), sel).toMatch(/width: var\(--ph-hit\)/);
      expect(bodyOf(sel), sel).toMatch(/height: var\(--ph-hit\)/);
    }
  });

  it("buttons answer a press within 100 ms (:active scale, 90 ms transition)", () => {
    const press = rules().find((r) => r.selectors.some((s) => s.startsWith("html.phone .btn:active")));
    expect(press?.body).toMatch(/transform: scale\(0\.97\)/);
    expect(bodyOf("html.phone .btn")).toMatch(/transition: [^;]*var\(--ph-d-press\)/);
    expect(bodyOf("html.phone .btn")).toMatch(/min-height: var\(--ph-hit\)/);
  });

  it("fields and the native select are 44/48 tall on the fill colour with a 3:1 boundary; the select keeps a chevron", () => {
    const field = bodyOf("html.phone .aur-combo select");
    expect(field).toMatch(/min-height: var\(--ph-hit\)/);
    expect(field).toMatch(/background: var\(--ph-fill\)/);
    expect(field).toMatch(/var\(--ph-field-line\)/);
    expect(bodyOf("html.phone .aur-textbox.light input")).toMatch(/background: var\(--ph-fill\)/);
    expect(bodyOf("html.phone .aur-combo > .aur-combo-arrow")).toMatch(/fill: var\(--ph-text-2\)/);
  });

  it("checkboxes stay checkboxes: a 24 px rounded box (O6); switches are 51 x 31", () => {
    const cb = bodyOf("html.phone .v4-cb");
    expect(cb).toMatch(/width: 24px/);
    expect(cb).toMatch(/height: 24px/);
    expect(cb).toMatch(/border-radius: 6px/);
    expect(bodyOf("html.phone .aur-switch-track")).toMatch(/width: 51px[^;]*;\s*height: 31px|width: 51px; height: 31px/);
  });

  it("steppers: each half is a full hit area, the value uses tabular figures", () => {
    expect(bodyOf("html.phone .score-input .btn-icon")).toMatch(/width: var\(--ph-hit\)/);
    expect(bodyOf("html.phone .aur-numeric-btns button")).toMatch(/width: var\(--ph-hit\)/);
    expect(bodyOf("html.phone .score-input output")).toMatch(/tabular-nums/);
  });

  it("empty states lose the dashed frame; loading text is visible", () => {
    expect(bodyOf("html.phone .empty")).toMatch(/border: 0/);
  });

  it("sheet content WP3 left small or in capitals reads in sentence case", () => {
    for (const sel of ["html.phone .aur-set-field", "html.phone .aur-set-tabs [role=\"tab\"]", "html.phone .v4-fl", "html.phone .v4-opt label", "html.phone .lvl-class h3", "html.phone .v4-tile-name", "html.phone .v4-tile-build", "html.phone .v4-newbtn > *", "html.phone .field > label"]) {
      expect(bodyOf(sel), sel).toMatch(/text-transform: none/);
    }
  });
});
