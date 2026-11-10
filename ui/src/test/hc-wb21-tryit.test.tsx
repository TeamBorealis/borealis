import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setPhoneForTests } from "../lib/phone";
import { mockApi } from "./hc-wb10-harness";
import { renderApp } from "./utils";

// Homebrew WB-21 (spec 9, 13.9, 16) in the creator on the mock: Try it from a feat draft, its setup, a choice made with the
// Build picker, the level ladder, the summary with the draft's changes marked, Start over, Close (the trial ends and the
// open character is never touched), the problems row, and the phone sheet.

afterEach(() => act(() => setPhoneForTests(false)));

const editor = () => screen.getByRole("region", { name: "Element editor" });
const nameBox = () => screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement;
const tryIt = () => screen.getByRole("dialog", { name: "Try it" });

/** A new pack with a feat "Fleet" (General) whose rule adds 5 to walking speed, made through the mock's API. */
async function openFeat(user: ReturnType<typeof userEvent.setup>, opts: { name?: string } = {}) {
  const api = mockApi();
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  const d = await api.homebrewNewDraft({ packId: pack.id, kind: "feat" });
  await api.homebrewUpdateDraft({
    draftId: d.draftId,
    form: { ...d.form, fields: { name: opts.name ?? "Fleet", category: "General" }, blocks: [{ key: "b1", kind: "stat", sentence: "", fields: { name: "speed", value: "5" } }] },
  });
  const saved = await api.homebrewSaveDraft({ draftId: d.draftId });
  const spies = {
    start: vi.spyOn(api, "homebrewTrialStart"),
    update: vi.spyOn(api, "homebrewTrialUpdate"),
    end: vi.spyOn(api, "homebrewTrialEnd"),
    character: vi.spyOn(api, "saveCharacter"),
  };
  renderApp(`/homebrew/${pack.id}`, { api });
  await user.click(await screen.findByRole("button", { name: "Fleet" }));
  await waitFor(() => expect(nameBox().value).toBe("Fleet"));
  return { api, spies, elementId: saved.elementIds[0]! };
}

async function openTryIt(user: ReturnType<typeof userEvent.setup>) {
  await user.click(within(editor()).getByRole("button", { name: "Try it" }));
  const dialog = await screen.findByRole("dialog", { name: "Try it" });
  await within(dialog).findByText(/Your test character is level 1/);
  return dialog;
}

describe("WB-21 Try it (mock)", () => {
  it("opens from the draft: the feat in its slot, level 1, the ladder shows it at 4; nothing of the open character changes", async () => {
    const user = userEvent.setup();
    const { api, spies } = await openFeat(user);
    const before = await api.listCharacters();
    const dialog = await openTryIt(user);
    expect(spies.start).toHaveBeenCalledWith({ draftId: expect.any(String), setup: {} });
    expect(within(dialog).getByText("Your feat")).toBeTruthy();
    expect(within(dialog).getByText("Fleet", { selector: ".hb-try-draft-name" })).toBeTruthy();
    expect(within(dialog).getByText(/first feat slot/)).toBeTruthy();
    const ladder = within(dialog).getByRole("table", { name: "Level ladder" });
    expect(within(ladder).getAllByRole("row")).toHaveLength(21);
    expect(ladder.querySelector('tr[data-level="4"]')?.textContent).toContain("Fleet");
    expect(ladder.querySelector('tr[data-level="1"]')?.className).toContain("sel");
    expect(within(dialog).getByRole("group", { name: "Ability scores" }).textContent).toContain("15");
    expect(await api.listCharacters()).toEqual(before);
    expect(spies.character).not.toHaveBeenCalled();
  });

  it("level: the spinner and a ladder row step the test character; the feat and its speed arrive at 4, marked", async () => {
    const user = userEvent.setup();
    const { spies } = await openFeat(user);
    const dialog = await openTryIt(user);
    const summary = within(dialog).getByRole("list", { name: "Combat numbers" });
    expect(summary.textContent).toContain("30 ft.");
    await user.click(within(dialog).getByRole("button", { name: "Increase Level" }));
    await within(dialog).findByText(/Your test character is level 2/);
    expect(spies.update).toHaveBeenLastCalledWith({ trialId: expect.any(String), change: { level: 2 } });
    const ladder = within(dialog).getByRole("table", { name: "Level ladder" });
    await user.click(within(ladder).getByRole("button", { name: "Level 4" }));
    await within(dialog).findByText(/Your test character is level 4/);
    await waitFor(() => expect(ladder.querySelector('tr[data-level="4"]')?.className).toContain("sel"));
    const speed = within(summary).getByText("35 ft.");
    expect(speed.closest(".hb-try-changed")?.textContent).toContain("(changed by this element)");
    const features = within(dialog).getByRole("region", { name: "Features" });
    expect(within(features).getByText("Fleet").closest(".hb-try-changed")).toBeTruthy();
    expect(within(dialog).getByText(/Taken at level 4/)).toBeTruthy();
    expect((within(dialog).getByRole("spinbutton", { name: "Level" }) as HTMLInputElement).value).toBe("4");
  });

  it("choices: a Build picker on the test character takes a pick through the trial", async () => {
    const user = userEvent.setup();
    const { spies } = await openFeat(user);
    const dialog = await openTryIt(user);
    const choices = within(dialog).getByRole("button", { name: /^Choices \(\d+\)$/ });
    const count = Number(/\((\d+)\)/.exec(choices.textContent ?? "")![1]);
    expect(count).toBeGreaterThan(0);
    // Fighter skill 1 of 2 and 2 of 2: one Build picker each.
    const [list] = await within(dialog).findAllByRole("list", { name: "Fighter skill options" });
    if (!list) throw new Error("no skill picker");
    await user.dblClick(await within(list).findByRole("button", { name: /^Athletics/ }));
    await waitFor(() => expect(spies.update).toHaveBeenCalledWith({ trialId: expect.any(String), change: { slotId: expect.any(String), elementId: expect.any(String) } }));
    await waitFor(() => expect(within(dialog).getByRole("button", { name: /^Choices \(\d+\)$/ }).textContent).toBe(`Choices (${count - 1})`));
    const skills = within(dialog).getByRole("region", { name: "Skills" });
    await waitFor(() => expect(skills.textContent).toContain("Athletics"));
  });

  it("setup: edition, a swap of two ability scores; Start over makes a new test character; Close ends it", async () => {
    const user = userEvent.setup();
    const { spies } = await openFeat(user);
    const dialog = await openTryIt(user);
    const scores = within(dialog).getByRole("group", { name: "Ability scores" });
    await user.click(within(scores).getByRole("button", { name: /^STR 15/ }));
    await user.click(within(scores).getByRole("button", { name: /^CHA 8/ }));
    await waitFor(() => expect(spies.update).toHaveBeenLastCalledWith({ trialId: expect.any(String), change: { setup: { abilityScores: [8, 14, 13, 10, 12, 15] } } }));
    await waitFor(() => expect(within(scores).getByRole("button", { name: /^STR 8/ })).toBeTruthy());
    fireEvent.change(within(dialog).getByLabelText("Edition"), { target: { value: "dnd2014" } });
    await waitFor(() => expect(spies.update).toHaveBeenLastCalledWith({ trialId: expect.any(String), change: { setup: { edition: "dnd2014" } } }));
    await user.click(within(dialog).getByRole("button", { name: "Start over" }));
    await waitFor(() => expect(spies.start).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(spies.end).toHaveBeenCalledTimes(1));
    await within(dialog).findByText(/Your test character is level 1/);
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Try it" })).toBeNull());
    await waitFor(() => expect(spies.end).toHaveBeenCalledTimes(2));
  });

  it("Ctrl+T opens Try it; a problem row closes it and goes to the field", async () => {
    const user = userEvent.setup();
    await openFeat(user, { name: "Fleet" });
    // An empty name is a problem of the draft: the mock puts the draft's problems on the level the feat is taken at.
    await user.clear(nameBox());
    await waitFor(() => expect(screen.getByRole("button", { name: /Problems \([1-9]/ })).toBeTruthy());
    fireEvent.keyDown(window, { key: "t", ctrlKey: true });
    const dialog = await screen.findByRole("dialog", { name: "Try it" });
    await within(dialog).findByText(/Your test character is level 1/);
    const problems = within(dialog).getByRole("button", { name: /^Problems \([1-9]/ });
    if (problems.getAttribute("aria-expanded") !== "true") await user.click(problems);
    await user.click(within(within(dialog).getByRole("list", { name: "Problems" })).getAllByRole("button")[0]!);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Try it" })).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(nameBox()));
  });

  it("phones: Try it is a full-height sheet, the ladder rows carry one detail line", async () => {
    act(() => setPhoneForTests(true));
    const user = userEvent.setup();
    await openFeat(user);
    await openTryIt(user);
    expect(tryIt().closest(".flyout-overlay")).toBeTruthy();
    expect(tryIt().querySelector(".sheet-grabber")).toBeTruthy();
    const row = within(tryIt()).getByRole("table", { name: "Level ladder" }).querySelector('tr[data-level="4"]')!;
    expect(row.querySelector(".ph-row-detail")?.textContent).toContain("Fleet");
    await user.click(within(tryIt()).getAllByRole("button", { name: "Close" }).at(-1)!);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Try it" })).toBeNull());
  });
});
