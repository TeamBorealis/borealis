import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

// U11 (GitHub #6): pick lists show every row at full height and only the tab page scrolls (a deliberate exception to
// U7: Aurora scrolls a long list such as Deity inside the page). The U10 filter bar sticks to the top of the page while
// its list scrolls past, so it stays usable on a long list; it leaves with the list's expander.
const css = (file: string) => readFileSync(resolve(__dirname, "..", file), "utf8");
const rule = (text: string, selector: string) => {
  const at = text.indexOf(`${selector} {`);
  return at < 0 ? "" : text.slice(at, text.indexOf("}", at) + 1);
};

describe("U11 full-height pick lists", () => {
  it("no pick list is a scroll area of its own, on desktop or at narrow widths", () => {
    const builder = css("pages/builder/builder.css");
    expect(builder).not.toMatch(/\.pick-list \{[^}]*(max-height|overflow)/);
    expect(builder).not.toMatch(/\.pick-list \{ max-height/);
    expect(css("styles/phone/lists.css")).toMatch(/html\.phone \.pick-list \{ max-height: none;/);
  });

  it("the filter bar sticks to the top of the page with an opaque background", () => {
    const bar = rule(css("styles/components.css"), ".pick-fbar");
    expect(bar).toMatch(/position: sticky; top: 0;/);
    expect(bar).toMatch(/background: var\(--pv-bar\)/);
    expect(rule(css("styles/phone/lists.css"), "html.phone .pick-fbar")).toMatch(/background: var\(--ph-bg\)/);
  });

  it("below 900 px the picker column is no scroll box of its own, so the bar sticks to the page", () => {
    expect(css("styles/components.css")).not.toMatch(/\.bp-left(, [.\w-]+)* \{ overflow-y: scroll; \}/);
    expect(css("pages/builder/builder.css")).toMatch(/@media \(min-width: 900px\) \{[^@]*\.bp-left \{ position: relative; overflow-y: scroll;/);
  });

  it("a row focused at the top edge stops below the stuck bar (review L7)", () => {
    expect(css("styles/components.css")).toMatch(/\.pick-fbar ~ \.pick-list \* \{ scroll-margin-top: 40px; \}/);
    expect(css("styles/phone/lists.css")).toMatch(/html\.phone \.pick-fbar ~ \.pick-list \* \{ scroll-margin-top: 112px; \}/);
  });
});
