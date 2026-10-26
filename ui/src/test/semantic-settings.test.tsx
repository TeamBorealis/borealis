import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { consentBody } from "../api/mock/semantic";
import { ON_KEYWORDS, READY_LINE } from "../pages/settings/SearchTab";
import { footerText, showSemanticHint } from "../pages/compendium/searchModel";
import { getStatus } from "../state/status";
import { openBorealisSettings, renderApp } from "./utils";

/* The exact spec section 8 wording; src-tauri/tests/semantic_model.rs pins the Rust text to the same strings. */
const CONSENT_DESKTOP = "Borealis will download the EmbeddingGemma 2 model (Apache 2.0, about 310 MB) once, from Hugging Face or the Borealis mirror. Search then also finds rules, lore and session notes by meaning, not only by the words you type. Everything runs on this device; your searches and notes never leave it. Indexing takes a few minutes. You can delete the model at any time in Settings.";
const CONSENT_X86_DESKTOP = "Borealis will download the EmbeddingGemma 2 model (Apache 2.0, about 310 MB) once, from Hugging Face or the Borealis mirror. Search then also finds rules, lore and session notes by meaning, not only by the words you type. Everything runs on this device; your searches and notes never leave it. Indexing takes a few minutes. On slower computers, search may stay keyword-only after the download. Borealis checks the speed once the model has loaded. You can delete the model at any time in Settings.";
const CONSENT_T1_PHONE = "Borealis will download the EmbeddingGemma 2 model (Apache 2.0, about 310 MB; 210 MB on this device) once, from Hugging Face or the Borealis mirror. Search then also finds rules, lore and session notes by meaning, not only by the words you type. Everything runs on this device; your searches and notes never leave it. Indexing can take several hours and runs while your device is charging. On slower phones, search may stay keyword-only after the download. Borealis checks the speed once the model has loaded. You can delete the model at any time in Settings.";
const SWITCH = "Semantic search (optional)";

async function openSearchTab(api: MockApi) {
  const user = userEvent.setup();
  renderApp("/characters", { api });
  await openBorealisSettings(user, "Search");
  const dialog = await screen.findByRole("dialog", { name: "Borealis Settings" });
  await within(dialog).findByText("SEARCH");
  return { user, dialog };
}

const desktop = (opts: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ rng: () => 0.5, ...opts });

describe("Settings > Search: consent", () => {
  it("shows the spec consent text and downloads nothing on Not now", async () => {
    // Arrange
    const api = desktop({ semantic: { backendAvailable: true } });
    const { user, dialog } = await openSearchTab(api);

    // Act
    await user.click(within(dialog).getByRole("checkbox", { name: SWITCH }));
    const consent = await screen.findByRole("dialog", { name: "Turn on semantic search?" });

    // Assert
    expect(within(consent).getByText(CONSENT_DESKTOP)).toBeTruthy();
    expect(within(consent).getAllByRole("button").map((b) => b.textContent)).toContain("Download and turn on");
    await user.click(within(consent).getByRole("button", { name: "Not now" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Turn on semantic search?" })).toBeNull());
    expect(api.semantic.starts).toBe(0);
    expect((within(dialog).getByRole("checkbox", { name: SWITCH }) as HTMLInputElement).checked).toBe(false);
    expect((await api.getSettings()).search.semantic).toBe(false);
  });

  it("Download and turn on downloads once and then semantic search is on", async () => {
    // Arrange
    const api = desktop({ semantic: { stepMs: 5, backendAvailable: true } });
    const { user, dialog } = await openSearchTab(api);

    // Act
    await user.click(within(dialog).getByRole("checkbox", { name: SWITCH }));
    await user.click(await screen.findByRole("button", { name: "Download and turn on" }));

    // Assert
    expect(await within(dialog).findByText(/^On: search also finds/)).toBeTruthy();
    expect(api.semantic.starts).toBe(1);
    expect((await api.getSettings()).search.semantic).toBe(true);
    expect((within(dialog).getByRole("checkbox", { name: SWITCH }) as HTMLInputElement).checked).toBe(true);
    expect(within(dialog).getByRole("button", { name: "Delete model and semantic indexes" })).toBeTruthy();
  });

  it("the mock consent wording is the spec wording on every device kind", () => {
    const q8 = { quant: "Q8_0", fileName: "embeddinggemma-2-Q8_0.gguf", bytes: 309_855_456, sizeText: "about 310 MB" };
    const t1 = { quant: "Q6_K-te-Q4_K", fileName: "embeddinggemma-2-Q6_K-te-Q4_K.gguf", bytes: 211_161_312, sizeText: "210 MB" };
    expect(consentBody(q8, "desktop")).toBe(CONSENT_DESKTOP);
    expect(consentBody(q8, "x86Desktop")).toBe(CONSENT_X86_DESKTOP);
    expect(consentBody(t1, "phone")).toBe(CONSENT_T1_PHONE);
  });

  it("an x86_64 desktop is told before the download that slower computers may stay keyword-only (B-49)", async () => {
    // Arrange
    const api = desktop({ semantic: { x86: true } });
    const { user, dialog } = await openSearchTab(api);

    // Act
    await user.click(within(dialog).getByRole("checkbox", { name: SWITCH }));
    const consent = await screen.findByRole("dialog", { name: "Turn on semantic search?" });

    // Assert
    expect(within(consent).getByText(CONSENT_X86_DESKTOP)).toBeTruthy();
    expect(CONSENT_X86_DESKTOP).toContain("Indexing takes a few minutes.");
    expect(CONSENT_X86_DESKTOP).not.toMatch(/hours|charging|phones/);
    await user.click(within(consent).getByRole("button", { name: "Not now" }));
    expect(api.semantic.starts).toBe(0);
  });
});

describe("Settings > Search in a build without the embedding backend", () => {
  it("the mock defaults to the real app: backend linked, mirror published", () => {
    expect(desktop().semantic.opts.backendAvailable).toBe(true);
    expect(desktop().semantic.opts.mirrorPublished).toBe(true);
  });

  it("the switch is disabled with one plain line, and no download can start", async () => {
    // Arrange
    const api = desktop({ semantic: { backendAvailable: false } });
    const { user, dialog } = await openSearchTab(api);

    // Act
    const sw = within(dialog).getByRole("checkbox", { name: SWITCH }) as HTMLInputElement;
    await user.click(sw);

    // Assert
    expect(sw.disabled).toBe(true);
    expect(sw.checked).toBe(false);
    expect(within(dialog).getByText("Semantic search (search by meaning) arrives in a later update. Search uses keywords until then.")).toBeTruthy();
    expect(screen.queryByRole("dialog", { name: "Turn on semantic search?" })).toBeNull();
    expect(api.semantic.starts).toBe(0);
    await expect(api.semanticDownloadStart()).rejects.toMatchObject({ code: "requirement_failed" });
    expect(api.semantic.starts).toBe(0);
    expect((await api.getSettings()).search.semantic).toBe(false);
    // Tier display and override stay.
    expect(within(dialog).getByLabelText("DEVICE TIER")).toBeTruthy();
  });
});

describe("Settings > Search: download controls", () => {
  it("pause keeps progress, resume continues, cancel removes it and turns semantic search off", async () => {
    // Arrange
    const api = desktop({ semantic: { stepMs: 400, backendAvailable: true } });
    const { user, dialog } = await openSearchTab(api);
    await user.click(within(dialog).getByRole("checkbox", { name: SWITCH }));
    await user.click(await screen.findByRole("button", { name: "Download and turn on" }));
    expect(await within(dialog).findByRole("progressbar", { name: "Model download" })).toBeTruthy();

    // Act / Assert: pause
    await user.click(within(dialog).getByRole("button", { name: "Pause" }));
    expect(await within(dialog).findByText(/^Paused at/)).toBeTruthy();
    // resume
    await user.click(within(dialog).getByRole("button", { name: "Resume" }));
    await waitFor(() => expect(api.semantic.starts).toBe(2));
    // cancel
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(within(dialog).queryByRole("progressbar", { name: "Model download" })).toBeNull());
    expect((await api.getSettings()).search.semantic).toBe(false);
    expect((within(dialog).getByRole("checkbox", { name: SWITCH }) as HTMLInputElement).checked).toBe(false);
  });

  it("Delete model and semantic indexes asks first, then removes the model and turns semantic search off", async () => {
    // Arrange
    const api = desktop({ semantic: { modelPresent: true } });
    api.presetSettings({ search: { semantic: true, tierOverride: "auto", indexOnBattery: false, defaultEdition: "character" } });
    const { user, dialog } = await openSearchTab(api);

    // Act
    await user.click(within(dialog).getByRole("button", { name: "Delete model and semantic indexes" }));
    const confirm = await screen.findByRole("dialog", { name: "Delete model and semantic indexes" });
    await user.click(within(confirm).getByRole("button", { name: "Delete" }));

    // Assert
    await waitFor(() => expect(within(dialog).queryByRole("button", { name: "Delete model and semantic indexes" })).toBeNull());
    expect((await api.semanticStatus()).modelPresent).toBe(false);
    expect((await api.getSettings()).search.semantic).toBe(false);
    expect(getStatus()).toBe("The semantic search model and its indexes were deleted.");
  });

  it("offers only lower tiers than the probe, and Automatic", async () => {
    // Arrange / Act
    const { dialog } = await openSearchTab(desktop());

    // Assert
    const combo = within(dialog).getByLabelText("DEVICE TIER");
    expect(within(combo).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Automatic (Tier 3: full model, most detail)", "Tier 1: small model, light indexing", "Tier 2: full model",
    ]);
    expect(within(dialog).queryByRole("checkbox", { name: "Index on battery" })).toBeNull();
  });
});

describe("Settings > Search on phones", () => {
  it("a T1 phone gets the mirrored file: the consent names its size", async () => {
    // Arrange
    const api = new MockApi({ mobile: true, semantic: { probedTier: "t1" } });
    const user = userEvent.setup();
    renderApp("/more", { api });

    // Act
    await user.click(await screen.findByRole("button", { name: "Search settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Borealis Settings" });
    await user.click(await within(dialog).findByRole("checkbox", { name: SWITCH }));

    // Assert
    const consent = await screen.findByRole("dialog", { name: "Turn on semantic search?" });
    expect(within(consent).getByText(CONSENT_T1_PHONE)).toBeTruthy();
    expect(api.semantic.starts).toBe(0);
  });

  it("a T1 phone whose file no URL serves reports it as not available yet and never opens the consent", async () => {
    // Arrange
    const api = new MockApi({ mobile: true, semantic: { probedTier: "t1", mirrorPublished: false } });
    const user = userEvent.setup();
    renderApp("/more", { api });

    // Act
    await user.click(await screen.findByRole("button", { name: "Search settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Borealis Settings" });
    await within(dialog).findByText("The model for this device is not available yet.");
    await user.click(within(dialog).getByRole("checkbox", { name: SWITCH }));

    // Assert
    expect(screen.queryByRole("dialog", { name: "Turn on semantic search?" })).toBeNull();
    expect(api.semantic.starts).toBe(0);
    expect(within(dialog).getByRole("checkbox", { name: "Index on battery" })).toBeTruthy();
  });

  it("a T0 phone explains that only keyword search is available", async () => {
    const api = new MockApi({ mobile: true, semantic: { probedTier: "t0" } });
    const user = userEvent.setup();
    renderApp("/more", { api });
    await user.click(await screen.findByRole("button", { name: "Search settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Borealis Settings" });
    expect(await within(dialog).findByText(/needs at least 3 GB of memory/)).toBeTruthy();
    expect(within(dialog).queryByRole("checkbox", { name: SWITCH })).toBeNull();
  });
});

describe("search flyout hint and footer", () => {
  async function search(api: MockApi, query: string) {
    const user = userEvent.setup();
    renderApp("/characters", { api });
    await user.type(await screen.findByLabelText("Compendium search"), `${query}{Enter}`);
    const dlg = await screen.findByRole("dialog", { name: "Compendium" });
    await waitFor(() => expect(dlg.querySelectorAll(".v4-el .n").length).toBeGreaterThan(0));
    return { user, dlg };
  }

  it("shows the one-line hint only once semantic search can really work, and it opens Settings > Search", async () => {
    // Arrange
    const api = desktop({ semantic: { backendAvailable: true } });
    const { user, dlg } = await search(api, "fireball");

    // Act
    const link = await within(dlg).findByRole("button", { name: "turn on semantic search" });
    expect(link.parentElement?.textContent).toBe("Better results for descriptive questions: turn on semantic search");
    await user.click(link);

    // Assert
    const settings = await screen.findByRole("dialog", { name: "Borealis Settings" });
    expect(within(settings).getByRole("tab", { name: "Search" }).getAttribute("aria-selected")).toBe("true");
    expect(api.semantic.starts).toBe(0);
  });

  it("no hint without an embedding backend, and the footer stays keyword-only", async () => {
    const { dlg } = await search(desktop({ semantic: { backendAvailable: false } }), "fireball");
    expect(within(dlg).queryByRole("button", { name: "turn on semantic search" })).toBeNull();
    expect(within(dlg).getByRole("note", { name: "Search mode" }).textContent).toBe("Keyword search");
  });

  it("footer and hint rules", () => {
    expect(footerText(null, true)).toBe("Keyword + semantic");
    expect(footerText({ state: "building", done: 1, total: 3, message: null }, false)).toBe("Keyword search - indexing 1/3");
    const base = { tier: "t3", probedTier: "t3", mobile: false, file: null, available: true, unavailableReason: null, modelPresent: false,
      backendAvailable: true, semantic: false, active: false, consent: null, download: { state: "idle", done: 0, total: 0, message: null },
      indexing: { state: "off", done: 0, total: 0, message: null, queryable: false } } as const;
    expect(showSemanticHint(null)).toBe(false);
    expect(showSemanticHint({ ...base, file: null })).toBe(false);
    const file = { quant: "Q8_0", fileName: "f", bytes: 1, sizeText: "about 310 MB" };
    expect(showSemanticHint({ ...base, file })).toBe(true);
    expect(showSemanticHint({ ...base, file, semantic: true })).toBe(false);
    expect(showSemanticHint({ ...base, file, backendAvailable: false })).toBe(false);
    expect(showSemanticHint({ ...base, file, available: false })).toBe(false);
  });
});

describe("semantic index (S3.3)", () => {
  it("the footer claims semantic only once the index is built, showing progress before", async () => {
    // Arrange: semantic on with the model present; the mock index builds in four steps.
    const api = desktop({ semantic: { modelPresent: true, stepMs: 150 } });
    api.presetSettings({ search: { semantic: true, tierOverride: "auto", indexOnBattery: false, defaultEdition: "character" } });
    const user = userEvent.setup();
    renderApp("/characters", { api });

    // Act
    await user.type(await screen.findByLabelText("Compendium search"), "fireball{Enter}");
    const dlg = await screen.findByRole("dialog", { name: "Compendium" });
    const foot = within(dlg).getByRole("note", { name: "Search mode" });

    // Assert
    await waitFor(() => expect(foot.textContent).toMatch(/^Keyword search - semantic indexing \d+%$/));
    await waitFor(() => expect(foot.textContent).toBe("Keyword + semantic"), { timeout: 3000 });
    expect((await api.semanticStatus()).active).toBe(true);
  });

  it("Settings shows the index being built, then the On line", async () => {
    // Arrange
    const api = desktop({ semantic: { modelPresent: true, stepMs: 150 } });
    api.presetSettings({ search: { semantic: true, tierOverride: "auto", indexOnBattery: false, defaultEdition: "character" } });

    // Act
    const { dialog } = await openSearchTab(api);

    // Assert
    expect(await within(dialog).findByText(/^Building the semantic index: \d+ of 1200 passages\./)).toBeTruthy();
    expect(within(dialog).getByText("On: search uses keywords until the semantic index is built.")).toBeTruthy();
    expect(await within(dialog).findByText(/^On: search also finds/, {}, { timeout: 3000 })).toBeTruthy();
    // The status line is read by its sentence, not a label that would hide it from screen readers.
    const status = within(dialog).getAllByRole("status").find((e) => e.textContent === READY_LINE);
    expect(status?.getAttribute("aria-label")).toBeNull();
  });

  it("a device too slow for the 40 ms budget stays keyword-only with the too-slow line (PH1)", async () => {
    // Arrange: the model is on the device, but one query takes 95 ms to embed.
    const api = desktop({ semantic: { modelPresent: true, stepMs: 20, queryMs: 95 } });
    api.presetSettings({ search: { semantic: true, tierOverride: "auto", indexOnBattery: false, defaultEdition: "character" } });

    // Act
    const { dialog } = await openSearchTab(api);

    // Assert
    const line = "Semantic search is too slow on this device: one search takes 95 ms to embed (the budget is 40 ms), so search uses keywords. You can delete the model in Settings.";
    expect(await within(dialog).findByText(line)).toBeTruthy();
    expect(within(dialog).getByText(ON_KEYWORDS)).toBeTruthy();
    expect(within(dialog).queryByText("On: search uses keywords until the semantic index is built.")).toBeNull();
    const st = await api.semanticStatus();
    expect(st.active).toBe(false);
    expect(st.indexing).toMatchObject({ state: "failed", queryable: false, done: 0 });
    expect(within(dialog).queryByText(READY_LINE)).toBeNull();
  });

  it("a device within the budget builds the index as before", async () => {
    // Arrange
    const api = desktop({ semantic: { modelPresent: true, stepMs: 5, queryMs: 25 } });
    api.presetSettings({ search: { semantic: true, tierOverride: "auto", indexOnBattery: false, defaultEdition: "character" } });

    // Act
    const { dialog } = await openSearchTab(api);

    // Assert
    expect(await within(dialog).findByText(READY_LINE, {}, { timeout: 3000 })).toBeTruthy();
    expect((await api.semanticStatus()).active).toBe(true);
  });

  it("footer lines for every index state", () => {
    const i = (state: "loading" | "indexing" | "paused" | "disabled" | "ready", done = 0, total = 0) => ({ state, done, total, message: null, queryable: state === "ready" });
    expect(footerText(null, false, i("loading"))).toBe("Keyword search - loading the semantic model");
    expect(footerText(null, false, i("indexing", 300, 1200))).toBe("Keyword search - semantic indexing 25%");
    expect(footerText(null, true, i("indexing", 1, 3))).toBe("Keyword + semantic - semantic indexing 33%");
    expect(footerText(null, false, i("paused"))).toBe("Keyword search - semantic indexing paused");
    expect(footerText(null, false, i("disabled"))).toBe("Keyword search - semantic search unavailable");
    expect(footerText(null, true, i("ready"))).toBe("Keyword + semantic");
    expect(footerText({ state: "building", done: 1, total: 3, message: null }, false, i("indexing", 1, 2))).toBe("Keyword search - indexing 1/3");
  });
});

describe("Settings > About", () => {
  it("shows the model NOTICE and its licence", async () => {
    const user = userEvent.setup();
    renderApp("/settings?tab=about");
    const notice = await screen.findByLabelText("EmbeddingGemma 2 notice");
    expect(notice.textContent).toContain("Google");
    await user.click(screen.getByRole("button", { name: "Show licence" }));
    expect((await screen.findByLabelText("EmbeddingGemma 2 licence")).textContent).toContain("Apache License");
  });
});
