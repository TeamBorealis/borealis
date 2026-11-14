import { describe, expect, it } from "vitest";
import { parseRules, readCss } from "./cssRules";

// H-arI (HC13 3): the white text boxes of the New pack / New element / New character flyouts (`.v4-new
// .aur-textbox.light`) show a subtle 1 px focus ring: their 1 px border turns Highlight (Aurora's focused MetroTextBox
// border), with no outline added. An accessibility exception to U7; desktop only (phones draw their own focused field).

const FILE = "pages/manage/manage.css";
const BOX = ".v4-new .aur-textbox.light input";
const rules = () => parseRules(readCss(FILE)).filter((r) => r.atRules.length === 0);
const declarations = (sel: string) => rules().filter((r) => r.selectors.includes(sel)).map((r) => r.body).join(";");

describe("H-arI flyout text box focus ring", () => {
  it("a keyboard-focused flyout text box gets a 1 px Highlight border, on desktop only, with no outline", () => {
    // Arrange
    const ring = `html:not(.phone) ${BOX}:focus-visible:not([aria-invalid="true"])`; // an invalid box stays red

    // Act
    const body = declarations(ring);

    // Assert
    expect(body).toMatch(/border-color:\s*var\(--accent-base\)/);
    expect(body).not.toMatch(/outline(?!:\s*0)/);
    expect(declarations(`${BOX}:focus`)).toMatch(/outline:\s*0/); // the default 2 px light ring stays off (U7)
  });

  it("the box keeps its 1 px border, so the ring changes no size", () => {
    const base = parseRules(readCss("styles/components.css")).filter((r) => r.selectors.includes(".aur-textbox input")).map((r) => r.body).join(";");
    expect(base).toMatch(/border:\s*1px solid/);
  });
});
