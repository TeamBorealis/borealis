import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Portrait } from "../components";
import { applyInsets } from "../lib/androidInsets";
import { installKeyboardInset } from "../lib/keyboardInset";
import { setPhoneForTests } from "../lib/phone";
import { renderApp } from "./utils";
import { parseRules, readCss } from "./cssRules";

// B-48 WP8: fixes for what the device review found in the real WebViews (mobile-facelift-spec sec. 7 WP8).
const phoneRules = (file: string) => parseRules(readCss(file)).filter((r) => r.selectors.every((s) => /^html\.(phone|ios|android)\b/.test(s)));

describe("device review fixes (B-48 WP8)", () => {
  it("an inventory item's name stays a full target when the name is short (Torch was 40 px wide on the Pixel)", () => {
    const body = phoneRules("styles/phone/gear.css")
      .filter((r) => r.selectors.includes("html.phone .eq-invgrid td.eq-invname .ap-name"))
      .map((r) => r.body).join(";");
    expect(body).toMatch(/min-width:\s*var\(--ph-hit\)/);
  });

  it("sheets keep their content out of the side insets in landscape (the Pixel's cutout is 52 px on the left)", () => {
    const rules = phoneRules("styles/phone/overlays.css");
    for (const sel of ["html.phone .dialog", "html.phone .flyout-overlay.bottom .flyout"]) {
      const body = rules.filter((r) => r.selectors.includes(sel) && r.atRules.length === 0).map((r) => r.body).join(";");
      expect(body, sel).toMatch(/padding-left:\s*var\(--safe-l\)/);
      expect(body, sel).toMatch(/padding-right:\s*var\(--safe-r\)/);
    }
  });

  it("an alert wraps long element ids instead of running off the screen (200% text on the Pixel)", () => {
    const body = phoneRules("styles/phone/controls.css")
      .filter((r) => r.selectors.includes("html.phone .alert > *")).map((r) => r.body).join(";");
    expect(body).toMatch(/min-width:\s*0/);
    expect(body).toMatch(/overflow-wrap:\s*anywhere/);
  });

  describe("the Android keyboard (edge to edge: the WebView's viewport does not shrink for it)", () => {
    afterEach(() => { document.documentElement.style.removeProperty("--kb"); document.body.innerHTML = ""; });
    const still = () => {
      const vv = Object.assign(new EventTarget(), { height: 915, offsetTop: 0 });
      return Object.assign(new EventTarget(), { innerHeight: 915, visualViewport: vv }) as unknown as Window;
    };

    it("takes --kb from the IME inset MainActivity reports", () => {
      const root = document.documentElement;
      const off = installKeyboardInset(still(), root);
      const input = document.createElement("input");
      document.body.appendChild(input);
      input.focus();
      applyInsets({ t: 24, r: 0, b: 24, l: 0, k: 312 });
      expect(root.style.getPropertyValue("--kb")).toBe("312px");
      applyInsets({ t: 24, r: 0, b: 24, l: 0, k: 0 });
      expect(root.style.getPropertyValue("--kb")).toBe("0px");
      off();
    });

    it("ignores a keyboard height that is not a plausible number", () => {
      const root = document.documentElement;
      const off = installKeyboardInset(still(), root);
      applyInsets({ k: -4 });
      applyInsets({ k: "300px" });
      applyInsets({ k: 50_000 });
      expect(root.style.getPropertyValue("--kb")).toBe("0px");
      off();
    });
  });

  it("screen-reader-only headings are read in their own casing (TalkBack got \"BUILD\" from an uppercase rule)", () => {
    const body = phoneRules("styles/phone/base.css").filter((r) => r.selectors.includes("html.phone .sr-only")).map((r) => r.body).join(";");
    expect(body).toMatch(/text-transform:\s*none/);
  });

  describe("the iOS keyboard (scrollIntoView panned the whole page: the Settings footer went under the keyboard bar)", () => {
    afterEach(() => { document.documentElement.style.removeProperty("--kb"); document.body.innerHTML = ""; });

    it("centres the focused field inside its own scroller and never scrolls the page", () => {
      const root = document.documentElement;
      const vv = Object.assign(new EventTarget(), { height: 874, offsetTop: 0 });
      const scrollTo = vi.fn();
      const win = Object.assign(new EventTarget(), { innerHeight: 874, visualViewport: vv, scrollY: 66, scrollTo }) as unknown as Window;
      const scroller = document.createElement("div");
      scroller.style.overflowY = "auto";
      Object.defineProperty(scroller, "scrollHeight", { value: 1200 });
      Object.defineProperty(scroller, "clientHeight", { value: 400 });
      let top = 0;
      Object.defineProperty(scroller, "scrollTop", { get: () => top, set: (v: number) => { top = v; } });
      scroller.getBoundingClientRect = () => ({ top: 100, height: 400, bottom: 500, left: 0, right: 400, width: 400, x: 0, y: 100, toJSON: () => ({}) });
      const field = document.createElement("input");
      field.getBoundingClientRect = () => ({ top: 450, height: 40, bottom: 490, left: 0, right: 400, width: 400, x: 0, y: 450, toJSON: () => ({}) });
      const into = vi.fn();
      field.scrollIntoView = into;
      scroller.appendChild(field);
      document.body.appendChild(scroller);
      const off = installKeyboardInset(win, root);
      field.focus();
      vv.height = 478; vv.dispatchEvent(new Event("resize"));
      expect(root.style.getPropertyValue("--kb")).toBe("396px");
      expect(into).not.toHaveBeenCalled();
      expect(scroller.scrollTop).toBe(170); // field centre 470 to scroller centre 300
      expect(scrollTo).toHaveBeenCalledWith(0, 0); // a page pan iOS already made is undone
      off();
    });
  });

  describe("the More tab", () => {
    let restore: () => void = () => undefined;
    beforeEach(() => {
      const original = window.matchMedia;
      window.matchMedia = ((q: string) => ({
        matches: q.includes("max-width: 899px"), media: q, onchange: null, addEventListener: () => undefined, removeEventListener: () => undefined,
        addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false,
      })) as typeof window.matchMedia;
      restore = () => { window.matchMedia = original; };
    });
    afterEach(() => { restore(); setPhoneForTests(false); });

    it("on a phone, More is the only tab drawn as current on its screen (Start was drawn current too)", async () => {
      setPhoneForTests(true);
      renderApp("/more");
      const more = await screen.findByTestId("nav-more");
      expect(more.getAttribute("aria-current")).toBe("page");
      expect(more.getAttribute("data-selected")).toBe("true");
      const start = screen.getByTestId("nav-start");
      expect(start.getAttribute("aria-current")).toBeNull();
      expect(start.getAttribute("data-selected")).toBe("false");
    });

    it("a narrow desktop window keeps today's tab marks (U7)", async () => {
      renderApp("/more");
      const start = await screen.findByTestId("nav-start");
      expect(start.getAttribute("data-selected")).toBe("true");
      expect(screen.getByTestId("nav-more").getAttribute("data-selected")).toBe("true");
    });
  });

  it("a portrait that does not decode shows the initials placeholder, not its alt text over the tile", () => {
    // An imported file can carry a truncated image (the Android run showed "Portrait of Test Hero" in 40 px serif).
    render(<Portrait src="data:image/png;base64,iVBORw0KGgo=" name="Test Hero" className="v4-tile-img" />);
    fireEvent.error(screen.getByRole("img", { name: "Portrait of Test Hero" }));
    const placeholder = screen.getByRole("img", { name: "Portrait placeholder for Test Hero" });
    expect(placeholder.textContent).toBe("TH");
    expect(placeholder.className).toBe("v4-tile-img");
  });
});
