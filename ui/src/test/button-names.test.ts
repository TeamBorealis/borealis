import { describe, expect, test } from "vitest";
import { installButtonNames, nameButtons } from "../lib/buttonNames";

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

  // B-144's Generate button holds both labels and hides one with aria-hidden; macOS read "Generate PreviewGenerating...".
  test("leaves out text hidden with aria-hidden", () => {
    document.body.innerHTML = `<button id="a"><span>Generate Preview</span><span aria-hidden="true">Generating...</span></button>`;
    nameButtons(document);
    expect(document.getElementById("a")!.getAttribute("aria-label")).toBe("Generate Preview");
  });

  test("follows an aria-hidden switch between two labels", async () => {
    document.body.innerHTML = `<button id="a"><span id="g">Generate Preview</span><span id="w" aria-hidden="true">Generating...</span></button>`;
    const dispose = installButtonNames(document);
    document.getElementById("g")!.setAttribute("aria-hidden", "true");
    document.getElementById("w")!.removeAttribute("aria-hidden");
    await new Promise((ok) => requestAnimationFrame(() => ok(undefined)));
    dispose();
    expect(document.getElementById("a")!.getAttribute("aria-label")).toBe("Generating...");
  });
});
