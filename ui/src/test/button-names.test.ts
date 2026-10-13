import { describe, expect, test } from "vitest";
import { nameButtons } from "../lib/buttonNames";

describe("nameButtons", () => {
  test("mirrors visible text into aria-label and keeps explicit labels", () => {
    document.body.innerHTML = `<button id="a">Skip <b>now</b></button><button id="b" aria-label="Settings">Cog</button><button id="c"><svg></svg></button>`;
    nameButtons(document);
    expect(document.getElementById("a")!.getAttribute("aria-label")).toBe("Skip now");
    expect(document.getElementById("b")!.getAttribute("aria-label")).toBe("Settings");
    expect(document.getElementById("c")!.hasAttribute("aria-label")).toBe(false);
  });

  test("follows text changes of an auto-labelled button", () => {
    document.body.innerHTML = `<button id="a">Generate Preview</button>`;
    nameButtons(document);
    document.getElementById("a")!.textContent = "Generating...";
    nameButtons(document);
    expect(document.getElementById("a")!.getAttribute("aria-label")).toBe("Generating...");
  });
});
