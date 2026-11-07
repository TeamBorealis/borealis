import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { DEFAULT_SETTINGS } from "../api/mock/default-settings";
import type { AppSettings } from "../api/contract";
import { setPhoneForTests } from "../lib/phone";
import { formChanged, mergeForm } from "../pages/settings/settingsForm";
import { SETTINGS_TABS, resolveSettingsTab, visibleSettingsTabs } from "../pages/settings/settingsTabs";
import { getStatus } from "../state/status";
import { openSettingsTab, renderApp, seededId } from "./utils";

// GitHub #31 (suggested by Tjop Vaoxad): one settings window. The classic builder's tabs keep their names and order; the
// Borealis-only settings (the old "Borealis Settings" window) are extra tabs in the same window.

const AURORA_TABS = ["General", "Appearance", "Character build", "Data files", "Equipment", "Character sheet"];
const fast = (opts: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5, ...opts });
const dialog = () => screen.getByRole("dialog", { name: "Application Settings" });
const selectedTab = () => within(dialog()).getAllByRole("tab").find((t) => t.getAttribute("aria-selected") === "true")?.textContent;

type Row = { kind: "label" | "button" | "radio"; name: string | RegExp };
const label = (name: string | RegExp): Row => ({ kind: "label", name });
const button = (name: string): Row => ({ kind: "button", name });
const radio = (name: string): Row => ({ kind: "radio", name });

/** Every setting of both former windows, by the tab that now holds it. */
const EVERY_SETTING: [tab: string, rows: Row[]][] = [
  ["General", [label("PLAYER NAME"), label("ABILITY SCORE GENERATION OPTION"), label("ROW HEIGHT"), label("Selection Flow"),
    label("Skills Panel Expanded"), label("Saving Throws Expanded"), label("'Remove Level' Confirmation")]],
  ["Appearance", [radio("Red"), radio("Dark"), label(/CHARACTER COLLECTION PORTRAIT SIZE/)]],
  ["Character build", [label("ABILITY SCORE GENERATION OPTION"), label("Ability Score Maximum (default)")]],
  ["Data files", [label("Documents directory"), label("Automatic Updates"), label("Content Import Notice"), label("Secondary content directory"),
    button("Clear Cache"), button("Open logs folder")]],
  ["Equipment", [label("'Add Weapon to Attacks' Confirmation")]],
  ["Character sheet", [label("Form Fillable"), label("THEME"), label("RULES EDITION"), label("PAPER"), label("Style Formatting (Recommended)"),
    label("Invert Ability Score"), label("Include Non-Prepared Spells"), label("Attack Cards"), label("Legacy Spellcasting Page")]],
  // Former Borealis Settings > Content (ruleset) and > General (rules limits).
  ["Rules", [label("DEFAULT RULESET FOR NEW CHARACTERS"), label("POINT-BUY BUDGET"), label("MINIMUM BASE SCORE"), label("MAXIMUM BASE SCORE"),
    button("Edition & compatibility…")]],
  // Former Borealis Settings > General.
  ["Saving", [label("Autosave"), label("Generate the sheet preview automatically"), label("Open the sheet after saving")]],
  ["Display", [label("High contrast"), label("DENSITY")]],
  // Former Borealis Settings > Content (indexes).
  ["Content", [button("Manage…")]],
  ["Search", [label("Semantic search (optional)"), label("DEVICE TIER")]],
  ["AI assistant", [label("Enable AI assistant (optional)"), label("AI PROVIDER"), button("Browse…"), button("AI setup…")]],
  ["Phones", [label("Share AI with phones and tablets")]],
  ["Devices", []],
  ["Updates", [label("Check for new versions of Borealis"), button("Check now")]],
];

async function findRow(scope: HTMLElement, row: Row): Promise<HTMLElement> {
  const w = within(scope);
  if (row.kind === "button") return w.findByRole("button", { name: row.name });
  if (row.kind === "radio") return w.findByRole("radio", { name: row.name });
  return w.findByLabelText(row.name);
}

afterEach(() => {
  setPhoneForTests(false);
  document.documentElement.classList.remove("phone");
});

describe("#31 one settings window: tabs", () => {
  it("keeps the classic tabs first and in order, adds the Borealis tabs before License and About", () => {
    expect(SETTINGS_TABS.slice(0, 6).map((t) => t.label)).toEqual(AURORA_TABS);
    expect(SETTINGS_TABS.slice(-2).map((t) => t.label)).toEqual(["License", "About"]);
    expect(SETTINGS_TABS.slice(0, 6).every((t) => !t.borealis)).toBe(true);
    expect(SETTINGS_TABS.slice(6, -2).every((t) => t.borealis)).toBe(true);
    expect(new Set(SETTINGS_TABS.map((t) => t.id)).size).toBe(SETTINGS_TABS.length);
  });

  it("shows AI tabs only with the AI CLI integration, Phones only while AI is on, Updates only outside the stores", () => {
    const ids = (g: Parameters<typeof visibleSettingsTabs>[0]) => visibleSettingsTabs(g).map((t) => t.id);
    expect(ids({ aiAvailable: true, aiEnabled: true, storeBuild: false })).toEqual(SETTINGS_TABS.map((t) => t.id));
    expect(ids({ aiAvailable: true, aiEnabled: false, storeBuild: false })).not.toContain("phones");
    const mas = ids({ aiAvailable: false, aiEnabled: false, storeBuild: true });
    for (const hidden of ["ai", "phones", "updates"]) expect(mas).not.toContain(hidden);
    expect(resolveSettingsTab("updates", visibleSettingsTabs({ aiAvailable: false, aiEnabled: false, storeBuild: true }))).toBe("general");
    expect(resolveSettingsTab("search", visibleSettingsTabs({ aiAvailable: false, aiEnabled: false, storeBuild: true }))).toBe("search");
    expect(resolveSettingsTab(undefined, SETTINGS_TABS)).toBe("general");
  });

  it("the View menu has one settings entry, Application Settings", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("menuitem", { name: "View" }));
    const items = within(await screen.findByRole("menu")).getAllByRole("menuitem").map((i) => i.textContent ?? "");
    expect(items.filter((t) => /Settings/.test(t))).toEqual(["Application Settings"]);
  });
});

describe("#31 every former setting is reachable", () => {
  it.each(EVERY_SETTING)("%s", async (tab, rows) => {
    const user = userEvent.setup();
    renderApp("/characters", { api: fast({ aiEnabled: true }) });
    await openSettingsTab(user, tab);
    expect(selectedTab()).toBe(tab);
    const panel = within(dialog()).getByRole("tabpanel");
    for (const row of rows) expect(await findRow(panel, row)).toBeTruthy();
    if (tab === "Devices") expect(await within(panel).findByText(/No devices yet/)).toBeTruthy();
  });
});

describe("#31 each entry point opens the right tab", () => {
  it("View > Application Settings opens General", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("menuitem", { name: "View" }));
    await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
    await screen.findByRole("dialog", { name: "Application Settings" });
    expect(selectedTab()).toBe("General");
  });

  it("the title bar cog opens General", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("button", { name: "Settings" }));
    await screen.findByRole("dialog", { name: "Application Settings" });
    expect(selectedTab()).toBe("General");
  });

  it.each([["sheet", "Character sheet"], ["search", "Search"], ["devices", "Devices"], ["nope", "General"]])("/settings?tab=%s opens %s", async (id, tab) => {
    renderApp(`/settings?tab=${id}`);
    await screen.findByRole("dialog", { name: "Application Settings" });
    expect(selectedTab()).toBe(tab);
  });

  it("the sheet's Character sheet settings link opens Character sheet", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/sheet", { api, openCharacterId: await seededId(api, "Brannoc") });
    await user.click(await screen.findByRole("link", { name: "Character sheet settings" }));
    await screen.findByRole("dialog", { name: "Application Settings" });
    expect(selectedTab()).toBe("Character sheet");
  });

  it("the AI Assistant page opens AI assistant", async () => {
    const user = userEvent.setup();
    renderApp("/ai", { aiEnabled: true });
    await user.click(await screen.findByRole("button", { name: "AI settings" }));
    await screen.findByRole("dialog", { name: "Application Settings" });
    expect(selectedTab()).toBe("AI assistant");
  });

  it("AI setup opens AI assistant", async () => {
    const user = userEvent.setup();
    renderApp("/setup/ai", { api: fast({ aiEnabled: true, aiSetup: { completed: true } }) });
    await user.click(await screen.findByRole("button", { name: "AI settings" }));
    await screen.findByRole("dialog", { name: "Application Settings" });
    expect(selectedTab()).toBe("AI assistant");
  });

  it("the phone's More > Search settings opens Search", async () => {
    const user = userEvent.setup();
    renderApp("/more", { api: fast({ mobile: true }) });
    await user.click(await screen.findByRole("button", { name: "Search settings" }));
    await screen.findByRole("dialog", { name: "Application Settings" });
    expect(selectedTab()).toBe("Search");
  });

  it("the phone layout keeps every tab of the window", async () => {
    document.documentElement.classList.add("phone");
    setPhoneForTests(true);
    const user = userEvent.setup();
    renderApp("/characters", { api: fast({ mobile: true, storeBuild: true }) });
    await user.click(await screen.findByRole("button", { name: "Settings" }));
    await screen.findByRole("dialog", { name: "Application Settings" });
    expect(within(dialog()).getAllByRole("tab").map((t) => t.textContent)).toEqual(
      visibleSettingsTabs({ aiAvailable: false, aiEnabled: false, storeBuild: true }).map((t) => t.label));
    await user.click(within(dialog()).getByRole("tab", { name: "Saving" }));
    expect(await within(dialog()).findByLabelText("Autosave")).toBeTruthy();
  });
});

describe("#31 Borealis tabs follow the window's Save and Cancel", () => {
  it("a Borealis field waits for Save; Cancel discards it", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    await openSettingsTab(user, "Saving");
    await user.click(await screen.findByLabelText("Autosave"));
    expect((await api.getSettings()).autosave).toBe(true);
    await user.click(within(dialog()).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Application Settings" })).toBeNull());
    expect((await api.getSettings()).autosave).toBe(true);

    await openSettingsTab(user, "Display");
    await user.click(await screen.findByLabelText("High contrast"));
    await user.click(within(dialog()).getByRole("tab", { name: "Rules" }));
    await user.selectOptions(await screen.findByLabelText("DEFAULT RULESET FOR NEW CHARACTERS"), "dnd2014");
    await user.click(within(dialog()).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Application Settings" })).toBeNull());
    expect(await api.getSettings()).toMatchObject({ highContrast: true, defaultRuleset: "dnd2014", autosave: true });
    expect(getStatus()).toBe("Your preferences have been saved.");
  });

  it("AI setup saves a just-enabled AI switch before it leaves", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters", { api: fast({ aiSetup: { completed: true } }) });
    await openSettingsTab(user, "AI assistant");
    await user.click(await screen.findByLabelText("Enable AI assistant (optional)"));
    await user.click(await screen.findByRole("button", { name: "AI setup…" }));
    expect(await screen.findByRole("heading", { name: "Set up AI assistance" })).toBeTruthy();
    expect((await api.getSettings()).ai.enabled).toBe(true);
  });

  it("the house rules button opens the open character's Edition & compatibility", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/builder", { api, openCharacterId: await seededId(api, "Brannoc") });
    await openSettingsTab(user, "Rules");
    await user.click(await screen.findByRole("button", { name: "Edition & compatibility…" }));
    const flyout = await screen.findByRole("dialog", { name: "Edition & compatibility" });
    expect(within(flyout).getByRole("checkbox", { name: "Ignore feat prerequisites" })).toBeTruthy();
    expect(within(flyout).getByRole("checkbox", { name: "Ignore multiclass prerequisites" })).toBeTruthy();
  });

  it("without a character the house rules button is off", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await openSettingsTab(user, "Rules");
    expect((await screen.findByRole("button", { name: "Edition & compatibility…" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("#31 stored settings keep their keys", () => {
  const stored: AppSettings = {
    ...DEFAULT_SETTINGS, documentsDir: "/docs", secondaryContentDir: "/extra",
    ai: { ...DEFAULT_SETTINGS.ai, overridePath: "/opt/codex" }, search: { ...DEFAULT_SETTINGS.search, semantic: true },
  };

  it("Save sends the same keys as the stored settings, with the form's fields and the stored rest", () => {
    const form: AppSettings = {
      ...stored, autosave: false, density: "compact", documentsDir: null, search: { ...stored.search, semantic: false },
      ai: { ...stored.ai, enabled: true, overridePath: null }, sheet: { ...stored.sheet, openAfterSave: !stored.sheet.openAfterSave },
    };
    const merged = mergeForm(stored, form);
    expect(Object.keys(merged)).toEqual(Object.keys(stored));
    expect(Object.keys(merged.sheet)).toEqual(Object.keys(stored.sheet));
    expect(Object.keys(merged.ai)).toEqual(Object.keys(stored.ai));
    expect(merged).toMatchObject({ autosave: false, density: "compact", documentsDir: "/docs", search: { semantic: true } });
    expect(merged.ai).toMatchObject({ enabled: true, overridePath: "/opt/codex" });
    expect(merged.sheet.openAfterSave).toBe(form.sheet.openAfterSave);
  });

  it("an untouched form changes nothing", () => {
    expect(formChanged(stored, { ...stored }, false)).toBe(false);
    expect(formChanged(stored, { ...stored }, true)).toBe(true);
    expect(formChanged(stored, { ...stored, highContrast: !stored.highContrast }, false)).toBe(true);
  });
});
