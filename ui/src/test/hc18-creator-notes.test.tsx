import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { creatorStatus } from "../pages/homebrew/problems";
import { noteStatus } from "../pages/homebrew/statusNote";
import { getStatus, setStatus } from "../state/status";
import { renderApp } from "./utils";
import { PROBLEMS_LINE, editor, expectKeptThenCounted, lateChecks, line, nameBox, problemsButton } from "./hc18-notes-harness";
// The workspace is a lazy route (App.tsx): loading it at collect time keeps its cold transform out of the first wait.
import "../pages/homebrew/PackWorkspace";

// Homebrew HC18 for the creator's other messages (item I3v_g): "Made a copy of 'X' to edit" and every confirmation the
// editor writes (rules, entries, features, spellcasting, starting equipment, proficiencies, class options, Copy as JSON)
// were replaced by "Checking..." and the problems line as soon as a check ran. Each now stays until the form on screen
// changes, and the count is added after it once the check answers, as the save message does.

type User = ReturnType<typeof userEvent.setup>;
const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };

async function saved(api: MockApi, packId: string, kind: string, fields: Record<string, unknown>): Promise<void> {
  const v = await api.homebrewNewDraft({ packId, kind });
  const u = await api.homebrewUpdateDraft({ draftId: v.draftId, form: { ...v.form, fields: { ...v.form.fields, ...fields } } });
  await api.homebrewSaveDraft({ draftId: u.draftId });
}

/** A workspace on a new pack, with the checks held back on demand, and a new element of `radio`'s kind open. */
async function newElement(user: User, radio: RegExp, name: string) {
  const api = new MockApi({ creatorEnabled: true });
  const pack = await api.homebrewCreatePack({ request: PACK });
  const checks = lateChecks(api);
  renderApp(`/homebrew/${pack.id}`, { api });
  await user.click(await screen.findByRole("button", { name: "New element" }));
  const flyout = await screen.findByRole("dialog", { name: "New element" });
  await user.click(within(flyout).getByRole("radio", { name: radio }));
  await user.type(within(flyout).getByLabelText("Name"), name);
  await user.click(within(flyout).getByRole("button", { name: "Create" }));
  await waitFor(() => expect(nameBox().value).toBe(name));
  return { api, pack, checks };
}

/** Opens PROBLEMS, which runs the engine checks (spec 8.1 step 6). */
async function runCheck(user: User): Promise<void> {
  if (problemsButton().getAttribute("aria-expanded") === "true") await user.click(problemsButton());
  await user.click(problemsButton());
}

/** An edit whose check is held: the next message is written while a check is running. */
async function editWhileChecking(user: User, checks: ReturnType<typeof lateChecks>): Promise<void> {
  checks.hold();
  await user.type(nameBox(), "!");
  await waitFor(() => expect(checks.pending()).toBeGreaterThan(0));
}

describe("HC18 creatorStatus for a message about the pack", () => {
  it("keeps the message alone when it is not about the open form", () => {
    expect(creatorStatus("Moved 2 elements to Ash Wastes.", false, { errors: 1, warnings: 1 }, false))
      .toEqual({ message: "Moved 2 elements to Ash Wastes.", detail: "" });
    expect(creatorStatus("Moved 2 elements to Ash Wastes.", true, { errors: 0, warnings: 0 }, false))
      .toEqual({ message: "Moved 2 elements to Ash Wastes.", detail: "" });
  });
});

describe("HC18 noteStatus", () => {
  afterEach(() => setStatus(""));

  it("with no workspace open, is the plain status message", () => {
    noteStatus("Rule added.");
    expect(getStatus()).toBe("Rule added.");
  });
});

describe("HC18 creator messages stay until the form changes", () => {
  it("Made a copy of 'Wizard' to edit: stays when the copy opens, takes the count, and an edit drops it", async () => {
    // Arrange: WHERE USED of Fireball names the Wizard class, a loaded element (Edit makes a copy, HC5)
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const checks = lateChecks(api);
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "Spell Lists" }));
    const view = await screen.findByRole("region", { name: "Spell lists" });
    await user.selectOptions(within(view).getByRole("combobox", { name: "Spell list" }), "Wizard");
    const grid = await within(view).findByRole("table", { name: "Spells" });
    await user.click(within(grid).getByRole("button", { name: "Fireball" }));
    const where = await screen.findByRole("region", { name: "Where used" });
    await within(where).findByRole("button", { name: "Wizard" });

    // Act
    checks.hold();
    await user.click(within(where).getByRole("button", { name: "Edit Wizard" }));
    await waitFor(() => expect(nameBox().value).toBe("Wizard (copy)"));

    // Assert
    await expectKeptThenCounted(checks, "Made a copy of 'Wizard' to edit. The original stays as it is.", () => runCheck(user));
    await user.type(nameBox(), "!");
    await waitFor(() => expect(line().textContent).toMatch(PROBLEMS_LINE));
  });

  it("Rule added (twice), Rule moved up, Rule removed: each stays while its check runs, then takes the count", async () => {
    const user = userEvent.setup();
    const { checks } = await newElement(user, /^Spell A spell/, "Ember Bolt");
    await user.selectOptions(screen.getByRole("combobox", { name: /^School/ }), "Evocation");
    await user.click(within(editor()).getByRole("tab", { name: "RULES" }));
    const addRule = async () => {
      await user.click(within(editor()).getByRole("button", { name: "Add rule" }));
      await user.click(within(await screen.findByRole("dialog", { name: "Add a rule" })).getByRole("button", { name: "Advantage on a roll" }));
      await user.click(within(await screen.findByRole("dialog", { name: "New rule: Advantage on a roll" })).getByRole("button", { name: "OK" }));
    };

    checks.hold();
    await addRule();
    await expectKeptThenCounted(checks, "Rule added.");

    checks.hold();
    await addRule();
    await expectKeptThenCounted(checks, "Rule added.");

    checks.hold();
    await user.click(within(editor()).getByRole("button", { name: "Move selected rule up" }));
    await expectKeptThenCounted(checks, "Rule moved up.");

    checks.hold();
    await user.keyboard("{Delete}");
    await expectKeptThenCounted(checks, "Rule removed. Undo brings it back.");
  });

  it("a class proficiency picked under another kind: the line saying where it went stays, then takes the count", async () => {
    const user = userEvent.setup();
    const { api, checks } = await newElement(user, /^Class A class/, "Gloom Knight");
    const options = api.homebrewReferenceOptions.bind(api);
    vi.spyOn(api, "homebrewReferenceOptions").mockImplementation(async (args) => (args.target.types.includes("Proficiency")
      ? { total: 1, items: [{ id: "ID_PROFICIENCY_TOOL_PROFICIENCY_THIEVES_TOOLS", name: "Thieves' Tools", type: "Proficiency", summary: "", prerequisites: null, sourceName: "Core", edition: null, draft: false }] }
      : options(args)));
    await user.click(within(editor()).getByRole("tab", { name: "PROFICIENCIES" }));

    checks.hold();
    await user.click(within(editor()).getByRole("button", { name: "Pick: Armor training" }));
    const picker = await screen.findByRole("dialog", { name: /Choose/ });
    await user.dblClick(await within(picker).findByText("Thieves' Tools"));
    const ok = within(picker).queryByRole("button", { name: "OK" });
    if (ok) await user.click(ok);

    await expectKeptThenCounted(checks, "Thieves' Tools is listed under Tool and other proficiencies: a class lists its proficiencies by kind.");
  });

  it("LEVELS: Feature added at level 2, then Feature removed, each stays and takes the count", async () => {
    const user = userEvent.setup();
    const { checks } = await newElement(user, /^Class A class/, "Gloom Knight");
    await user.click(within(editor()).getByRole("tab", { name: "LEVELS" }));
    const grid = await screen.findByRole("table", { name: "Features by level" });

    checks.hold();
    await user.click(within(grid.querySelector<HTMLElement>('tr[data-level="2"]')!).getByRole("button", { name: "Add a feature at level 2" }));
    const dialog = await screen.findByRole("dialog", { name: /^Class feature:/ });
    await expectKeptThenCounted(checks, "Feature added at level 2.");

    checks.hold();
    await user.click(within(dialog).getByRole("button", { name: "Remove from Gloom Knight" }));
    await expectKeptThenCounted(checks, "Feature removed. Undo brings it back.");
  });

  it("SPELLCASTING: Spellcasting added stays, then takes the count", async () => {
    const user = userEvent.setup();
    const { checks } = await newElement(user, /^Class A class/, "Warden");
    await user.click(within(editor()).getByRole("tab", { name: "SPELLCASTING" }));

    checks.hold();
    await user.click(screen.getByRole("button", { name: "Add spellcasting" }));

    // a class's spellcasting starts at level 1
    await expectKeptThenCounted(checks, "Spellcasting added at level 1.");
  });

  it("EQUIPMENT: Starting equipment added stays, then takes the count", async () => {
    const user = userEvent.setup();
    const { checks } = await newElement(user, /^Class A class/, "Gloom Knight");
    await user.click(within(editor()).getByRole("tab", { name: "EQUIPMENT" }));

    checks.hold();
    await user.click(within(editor()).getByRole("button", { name: "Add starting equipment" }));

    await expectKeptThenCounted(checks, "Starting equipment added.");
  });

  it("a creature's ACTIONS: Action added stays, then takes the count", async () => {
    const user = userEvent.setup();
    const { checks } = await newElement(user, /^Creature A stat block/, "Cinder Boar");
    await user.click(within(editor()).getByRole("tab", { name: "ACTIONS" }));

    checks.hold();
    await user.click(within(editor()).getByRole("button", { name: "Add action" }));
    const dialog = await screen.findByRole("dialog", { name: "New action" });
    await user.type(within(dialog).getByRole("textbox", { name: /^Name/ }), "Tusk");
    await user.type(within(dialog).getByRole("textbox", { name: /^Text/ }), "The boar gores one creature.");
    await user.click(within(dialog).getByRole("button", { name: "OK" }));

    await expectKeptThenCounted(checks, "Action added.");
  });

  it("class options: Saved the option, then Deleted it from the pack on Cancel, each kept with the count", async () => {
    const user = userEvent.setup();
    const { checks } = await newElement(user, /^Class A class/, "Ember Knight");
    await editWhileChecking(user, checks);
    await user.click(within(editor()).getByRole("tab", { name: "RULES" }));
    await user.click(screen.getByRole("button", { name: "Add rule" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Add a rule" })).getByRole("button", { name: "Lets the player choose ..." }));
    const dialog = await screen.findByRole("dialog", { name: "New rule: Lets the player choose ..." });
    await user.type(within(dialog).getByRole("combobox", { name: "What to choose" }), "Option");
    const panel = await within(dialog).findByRole("region", { name: "Class options of this choice" });
    await user.type(within(panel).getByRole("textbox", { name: "Group" }), "Ember Style");
    await user.type(within(panel).getByRole("textbox", { name: "New option" }), "Ash Guard");
    await user.click(within(panel).getByRole("button", { name: "Add option" }));

    await expectKeptThenCounted(checks, "Saved the option Ash Guard to the pack. Open it under Class options to give it rules.");
    const count = line().lastElementChild!.textContent;

    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Discard changes?" })).getByRole("button", { name: "Discard" }));
    await waitFor(() => expect(line().textContent).toBe(`Deleted Ash Guard from the pack.${count}`));
  });

  it("DATA: Copied the element as JSON stays while a check runs, then takes the count", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    await saved(api, pack.id, "language", { name: "Gloomspeak" });
    const checks = lateChecks(api);
    Object.defineProperty(navigator, "clipboard", { value: { writeText: vi.fn().mockResolvedValue(undefined) }, configurable: true });
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: /^Languages \(1\)/ }));
    await user.click(await screen.findByRole("button", { name: "Gloomspeak" }));
    await waitFor(() => expect(nameBox().value).toBe("Gloomspeak"));

    await editWhileChecking(user, checks);
    await user.click(within(editor()).getByRole("tab", { name: "DATA" }));
    await user.click(await screen.findByRole("button", { name: "Copy as JSON" }));

    await expectKeptThenCounted(checks, "Copied the element as JSON.");
  });
});
