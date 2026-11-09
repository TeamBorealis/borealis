import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { parseRules } from "./cssRules";
import { renderApp, seededId } from "./utils";

// B-144 (tester eylonke, 2026-11-02; decision TV2 C11/C16): on a phone, Generate Preview seemed to do nothing because the
// preview rendered under the form, out of sight, and the button row jumped while it generated. On phones the preview now
// opens full-screen with Close, Save and Share (no nested scroller); on a narrow tablet the pane is scrolled into view;
// and the Generate button keeps its width while its label reads "Generating...".

function asPhone(os: "ios" | "android" = "ios") {
  document.documentElement.classList.add("phone");
  setPhoneForTests(true);
  setPhoneOsForTests(os);
}

afterEach(() => {
  setPhoneForTests(false);
  setPhoneOsForTests(null);
  document.documentElement.classList.remove("phone");
  vi.restoreAllMocks();
  Reflect.deleteProperty(window, "matchMedia");
  Reflect.deleteProperty(navigator, "share");
  Reflect.deleteProperty(navigator, "canShare");
});

async function openSheet(api = new MockApi({ rng: () => 0.5 })) {
  const id = await seededId(api, "Brannoc");
  renderApp("/sheet", { api, openCharacterId: id });
  await screen.findByRole("button", { name: /^(Generate Preview|Generating\.\.\.)$/ });
  return { api, id };
}

const previewDialog = () => screen.findByRole("dialog", { name: "Character sheet preview" });

describe("B-144 phone preview opens full-screen", () => {
  it("has no inline preview under the form, and Generate Preview opens the sheet full-screen in the same tap", async () => {
    asPhone();
    const user = userEvent.setup();
    const { api } = await openSheet();
    const render = vi.spyOn(api, "renderSheet");
    expect(document.querySelector(".v4-sheet-pane")).toBeNull();
    expect(screen.queryByTitle("Character sheet preview")).toBeNull();

    await user.click(screen.getByRole("button", { name: "Generate Preview" }));

    const dialog = await previewDialog();
    const frame = (await within(dialog).findByTitle("Character sheet preview")) as HTMLIFrameElement;
    expect(frame.getAttribute("sandbox")).toBe("");
    expect(render).toHaveBeenCalledTimes(1);
    const foot = dialog.querySelector(".dialog-foot") as HTMLElement;
    expect(within(foot).getAllByRole("button").map((b) => b.textContent)).toEqual(["Save", "Share", "Close"]);
  });

  it("Close returns to the sheet page and Save exports the PDF", async () => {
    asPhone();
    const user = userEvent.setup();
    const { api, id } = await openSheet();
    const exportSheet = vi.spyOn(api, "exportSheet");
    await user.click(screen.getByRole("button", { name: "Generate Preview" }));
    const dialog = await previewDialog();
    await within(dialog).findByTitle("Character sheet preview");
    await user.click(within(dialog.querySelector(".dialog-foot") as HTMLElement).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(exportSheet).toHaveBeenCalledWith(expect.objectContaining({ characterId: id, options: expect.objectContaining({ format: "pdf" }) })));
    await user.click(within(dialog.querySelector(".dialog-foot") as HTMLElement).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Character sheet preview" })).toBeNull());
    expect(screen.getByRole("button", { name: "Generate Preview" })).toBeTruthy();
  });

  it("Share on Android opens the system share sheet from the backend", async () => {
    asPhone("android");
    const user = userEvent.setup();
    const { api, id } = await openSheet();
    const share = vi.spyOn(api, "shareSheet").mockResolvedValue({ fileName: "Brannoc.pdf", shared: true, mime: "application/pdf", dataBase64: null });
    await user.click(screen.getByRole("button", { name: "Generate Preview" }));
    const dialog = await previewDialog();
    await within(dialog).findByTitle("Character sheet preview");
    await user.click(within(dialog).getByRole("button", { name: "Share" }));
    await waitFor(() => expect(share).toHaveBeenCalledWith(expect.objectContaining({ characterId: id })));
  });

  it("Share elsewhere hands the PDF, fetched as the preview opened, to the web view's share sheet within the tap", async () => {
    asPhone("ios");
    const shared: File[] = [];
    Object.defineProperty(navigator, "canShare", { configurable: true, value: () => true });
    Object.defineProperty(navigator, "share", { configurable: true, value: vi.fn(async (d: { files: File[] }) => { shared.push(...d.files); }) });
    const user = userEvent.setup();
    const { api } = await openSheet();
    const backend = vi.spyOn(api, "shareSheet");
    const save = vi.spyOn(api, "exportSheet");
    await user.click(screen.getByRole("button", { name: "Generate Preview" }));
    const dialog = await previewDialog();
    await within(dialog).findByTitle("Character sheet preview");
    const share = within(dialog).getByRole("button", { name: "Share" });
    await waitFor(() => expect((share as HTMLButtonElement).disabled).toBe(false));
    await user.click(share);
    await waitFor(() => expect(shared.map((f) => f.name)).toEqual([expect.stringMatching(/^Brannoc.*\.(pdf|html)$/)]));
    // B-148: the bytes come from shareSheet once, as the preview opened; exportSheet is Save (a picker) and is not called.
    expect(backend).toHaveBeenCalledTimes(1);
    expect(save).not.toHaveBeenCalled();
  });
});

describe("B-144 tablet and desktop", () => {
  it("a narrow tablet keeps the inline pane and scrolls it into view after Generate Preview", async () => {
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: (q: string) => ({ matches: q.includes("max-width: 899px"), media: q, addEventListener: () => undefined, removeEventListener: () => undefined }),
    });
    const scrolled: Element[] = [];
    // jsdom has no layout, so no scrollIntoView of its own
    Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, writable: true, value: () => undefined });
    vi.spyOn(Element.prototype, "scrollIntoView").mockImplementation(function (this: Element) { scrolled.push(this); });
    const user = userEvent.setup();
    await openSheet();
    await screen.findByTitle("Character sheet preview");
    scrolled.length = 0; // the automatic preview on open does not move the page
    await user.click(screen.getByRole("button", { name: "Generate Preview" }));
    await waitFor(() => expect(scrolled.some((el) => el.classList.contains("v4-sheet-pane"))).toBe(true));
    expect(screen.queryByRole("dialog", { name: "Character sheet preview" })).toBeNull();
  });

  it("the Generate button holds both labels in one cell, so its width does not change while generating", async () => {
    const api = new MockApi({ rng: () => 0.5, latencyMs: 30 });
    const user = userEvent.setup();
    await openSheet(api);
    await screen.findByTitle("Character sheet preview"); // the automatic preview on open has finished
    const button = screen.getByRole("button", { name: "Generate Preview" });
    const labels = [...button.querySelectorAll(".sh-lbl > span")];
    expect(labels.map((l) => l.textContent)).toEqual(["Generate Preview", "Generating..."]);
    expect(labels[1]?.getAttribute("aria-hidden")).toBe("true");
    await user.click(button);
    expect(screen.getByRole("button", { name: "Generating..." })).toBe(button);
    expect([...button.querySelectorAll(".sh-lbl > span")].map((l) => l.getAttribute("aria-hidden"))).toEqual(["true", null]);
    const css = parseRules(readFileSync(resolve(__dirname, "../pages/sheet/sheet.css"), "utf8"));
    expect(css.find((r) => r.selectors.includes(".sh-lbl"))?.body).toMatch(/display:\s*inline-grid/);
    expect(css.find((r) => r.selectors.includes(".sh-lbl > span"))?.body).toMatch(/grid-area:\s*1\s*\/\s*1/);
    expect(css.find((r) => r.selectors.includes('.sh-lbl > [aria-hidden="true"]'))?.body).toMatch(/visibility:\s*hidden/);
  });
});
