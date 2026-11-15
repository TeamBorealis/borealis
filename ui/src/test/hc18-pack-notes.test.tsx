import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { renderApp } from "./utils";
import { expectKeptUncounted, lateChecks, nameBox, problemsButton } from "./hc18-notes-harness";
// The workspace is a lazy route (App.tsx): loading it at collect time keeps its cold transform out of the first wait.
import "../pages/homebrew/PackWorkspace";

// Homebrew HC18 for messages about the pack rather than the open form (item I3v_g): a multi-select edit that reopens the
// open element, and a SPELL LISTS save while an element is open. The message stays until the form on screen changes,
// and has no problems count after it: the count is the open element's, the message is not about it.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };

async function saved(api: MockApi, packId: string, kind: string, fields: Record<string, unknown>): Promise<void> {
  const v = await api.homebrewNewDraft({ packId, kind });
  const u = await api.homebrewUpdateDraft({ draftId: v.draftId, form: { ...v.form, fields: { ...v.form.fields, ...fields } } });
  await api.homebrewSaveDraft({ draftId: u.draftId });
}

/** A pack with two languages, Gloomspeak open in the editor. */
async function packWithLanguages(user: ReturnType<typeof userEvent.setup>) {
  const api = new MockApi({ creatorEnabled: true });
  const pack = await api.homebrewCreatePack({ request: PACK });
  await saved(api, pack.id, "language", { name: "Gloomspeak" });
  await saved(api, pack.id, "language", { name: "Brightspeak" });
  const checks = lateChecks(api);
  renderApp(`/homebrew/${pack.id}`, { api });
  await user.click(await screen.findByRole("button", { name: /^Languages \(2\)/ }));
  await user.click(await screen.findByRole("button", { name: "Gloomspeak" }));
  await waitFor(() => expect(nameBox().value).toBe("Gloomspeak"));
  return { api, pack, checks };
}

describe("HC18 messages about the pack stay until the form changes, without a count", () => {
  it("a multi-select edit that holds the open element: its message stays when the element opens again", async () => {
    // Arrange: Gloomspeak open, Brightspeak added to the choice
    const user = userEvent.setup();
    const { checks } = await packWithLanguages(user);
    await user.keyboard("{Control>}");
    await user.click(screen.getByRole("button", { name: "Brightspeak" }));
    await user.keyboard("{/Control}");
    const bar = await screen.findByRole("toolbar", { name: "2 elements chosen" });

    // Act: Add tag; the open element is opened again from the pack
    checks.hold();
    await user.click(within(bar).getByRole("button", { name: "Add tag..." }));
    const dialog = await screen.findByRole("dialog", { name: "Add a tag" });
    await user.type(within(dialog).getByRole("textbox", { name: "Tag" }), "Secret");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));

    // Assert
    await expectKeptUncounted(checks, "Added the tag Secret to 2 elements.", async () => { await user.click(problemsButton()); });
  });

  it("SPELL LISTS saved while an element's check runs: the message stays when the check answers", async () => {
    // Arrange: an edit to Gloomspeak whose check is held
    const user = userEvent.setup();
    const { checks } = await packWithLanguages(user);
    checks.hold();
    await user.type(nameBox(), "!");
    await waitFor(() => expect(checks.pending()).toBeGreaterThan(0));

    // Act: check Bless onto the Wizard list and save
    await user.click(screen.getByRole("button", { name: "Spell Lists" }));
    const view = await screen.findByRole("region", { name: "Spell lists" });
    await user.selectOptions(within(view).getByRole("combobox", { name: "Spell list" }), "Wizard");
    const grid = await within(view).findByRole("table", { name: "Spells" });
    await user.click(within(grid).getByRole("checkbox", { name: "Choose Bless" }));
    await user.click(within(view).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(within(view).getByRole("status").textContent).toMatch(/^\d+ spells on this list$/));
    const count = /^(\d+)/.exec(within(view).getByRole("status").textContent ?? "")![1];

    // Assert
    await expectKeptUncounted(checks, `Saved the Wizard list: ${count} spells.`);
  });
});
