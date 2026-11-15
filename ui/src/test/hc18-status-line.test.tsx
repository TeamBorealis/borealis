import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { HomebrewProblem } from "../api/contract";
import { MockApi } from "../api/mock";
import { creatorStatus } from "../pages/homebrew/problems";
import { getStatus, setStatus } from "../state/status";
import { renderApp } from "./utils";
// The workspace is a lazy route (App.tsx): loading it here, at collect time, keeps the cold transform of the creator's
// modules out of the first test's 10 s wait (under load, at ~50, it ran out on the first element button).
import "../pages/homebrew/PackWorkspace";

// Homebrew HC18 (item I2rZs): after a save the status line shows the confirmation and the element's problems,
// "Saved 'Ember Bolt' to Ember Isles · 2 problems", with no suffix when there are none. The check that runs after the save
// never replaces the confirmation: it only adds its count. The next edit brings the plain problems line back.

const warning = (message: string): HomebrewProblem => ({ severity: "warning", message, path: null, elementId: null, code: "sweep" });
const LATE = [warning("At level 7 the choice has nothing to offer."), warning("At level 9 the choice has nothing to offer.")];

describe("HC18 creatorStatus", () => {
  it("shows the problems line, or Checking... while the checks run, when nothing was just saved", () => {
    expect(creatorStatus(null, false, { errors: 0, warnings: 0 })).toEqual({ message: "No problems", detail: "" });
    expect(creatorStatus(null, false, { errors: 1, warnings: 1 })).toEqual({ message: "2 problems", detail: "" });
    expect(creatorStatus(null, true, { errors: 1, warnings: 0 })).toEqual({ message: "Checking...", detail: "" });
  });

  it("keeps the save confirmation and adds the count after it, with no suffix when there are none", () => {
    const saved = "Saved 'Ember Bolt' to Ember Isles";
    expect(creatorStatus(saved, false, { errors: 0, warnings: 0 })).toEqual({ message: saved, detail: "" });
    expect(creatorStatus(saved, false, { errors: 0, warnings: 1 })).toEqual({ message: saved, detail: " · 1 problem" });
    expect(creatorStatus(saved, false, { errors: 1, warnings: 1 })).toEqual({ message: saved, detail: " · 2 problems" });
    // a check running after the save never replaces the confirmation with "Checking..."
    expect(creatorStatus(saved, true, { errors: 0, warnings: 0 })).toEqual({ message: saved, detail: "" });
  });
});

describe("HC18 status store", () => {
  afterEach(() => setStatus(""));

  it("reads a message and its detail as one line", () => {
    setStatus("Saved 'Ember Bolt' to Ember Isles", " · 2 problems");
    expect(getStatus()).toBe("Saved 'Ember Bolt' to Ember Isles · 2 problems");
    setStatus("No problems");
    expect(getStatus()).toBe("No problems");
  });
});

describe("HC18 creator status line after a save", () => {
  it("a check that finishes after the save adds its count to the confirmation; an edit brings the problems line back", async () => {
    // Arrange: an Ember Isles pack holding the spell Ember Bolt, opened in the creator
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
    const d = await api.homebrewNewDraft({ packId: pack.id, kind: "spell" });
    await api.homebrewUpdateDraft({ draftId: d.draftId, form: { ...d.form, fields: { ...d.form.fields, name: "Ember Bolt", school: "Evocation" } } });
    await api.homebrewSaveDraft({ draftId: d.draftId });
    // The checks answer at once until `hold`; then they wait until `release` and find two warnings.
    const check = api.homebrewCheckDraft.bind(api);
    let hold = false;
    let released = false;
    const waiting: (() => void)[] = [];
    vi.spyOn(api, "homebrewCheckDraft").mockImplementation(async (args) => {
      if (hold && !released) await new Promise<void>((r) => waiting.push(r));
      return released ? LATE : check(args);
    });
    renderApp(`/homebrew/${pack.id}`, { api });
    const status = () => screen.getByRole("contentinfo", { name: "Status" });
    const line = () => within(status()).getByRole("status");
    const editor = () => screen.getByRole("region", { name: "Element editor" });
    const nameBox = () => screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement;
    await user.click(await screen.findByRole("button", { name: "Ember Bolt" }));
    await waitFor(() => expect(nameBox().value).toBe("Ember Bolt"));
    await user.type(nameBox(), "!");
    await user.clear(nameBox());
    await user.type(nameBox(), "Ember Bolt");
    await waitFor(() => expect(line().textContent).toBe("No problems"));

    // Act: save while the check after it is held back
    hold = true;
    await user.click(within(editor()).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(line().textContent).toBe("Saved 'Ember Bolt' to Ember Isles"));
    const confirmation = line().firstElementChild;
    await waitFor(() => expect(waiting.length).toBeGreaterThan(0));
    const whileChecking = line().textContent;
    released = true;
    waiting.forEach((r) => r());

    // Assert: the late check adds its count after the confirmation and never replaces it
    expect(whileChecking).toBe("Saved 'Ember Bolt' to Ember Isles");
    await waitFor(() => expect(line().textContent).toBe("Saved 'Ember Bolt' to Ember Isles · 2 problems"));
    // Screen readers hear the count only: the confirmation's node is untouched and the region reads changes, not the whole line
    expect(line().getAttribute("aria-atomic")).toBe("false");
    expect(line().firstElementChild).toBe(confirmation);
    expect(line().firstElementChild!.textContent).toBe("Saved 'Ember Bolt' to Ember Isles");

    // An edit after the save: the line goes back to the problems count
    await user.type(nameBox(), "!");
    await waitFor(() => expect(line().textContent).toBe("2 problems"));
  });
});
