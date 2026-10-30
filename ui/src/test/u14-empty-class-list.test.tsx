import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { ELEMENTS } from "../api/mock/content";
import type { SelectionSlot, SlotEmptyReason } from "../api/contract";
import { EmptyListNote } from "../pages/builder/EmptyListNote";
import { renderApp } from "./utils";

// GitHub #10 (decision U14): R4 stands (classes never cross editions), but a Class list that is empty because of the edition
// says so with counts and offers "Start over as 2014", which makes a NEW character and keeps the old one. "Check your source
// restrictions" is said only when restrictions really hide options. The New Character dialog warns when the chosen edition
// has no classes. The mock's `classlessEdition` takes the 2024 classes away (every 2024 book switched off).
const CLASS_COUNT = ELEMENTS.filter((e) => e.type === "class").length;
const classless = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5, classlessEdition: "dnd2024" });

async function newCharacter(user: ReturnType<typeof userEvent.setup>, name: string, ruleset: "dnd2014" | "dnd2024") {
  await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
  const dialog = await screen.findByRole("dialog", { name: "New character" });
  await user.type(within(dialog).getByLabelText("Name"), name);
  await user.selectOptions(within(dialog).getByLabelText("Rules edition"), ruleset);
  return dialog;
}

describe("U14 the New Character dialog", () => {
  it("warns when the chosen edition has no classes and names the other edition's", async () => {
    const user = userEvent.setup();
    renderApp("/characters", { api: classless() });
    const dialog = await newCharacter(user, "Ayla", "dnd2024");
    const warning = await within(dialog).findByRole("alert");
    expect(warning.textContent).toBe(
      `No 2024 classes are available: no 2024 book with classes is switched on. Your 2014 books have ${CLASS_COUNT} classes, so choose the 2014 rules, or switch 2024 content on in Additional Content.`,
    );
    await user.selectOptions(within(dialog).getByLabelText("Rules edition"), "dnd2014");
    await waitFor(() => expect(within(dialog).queryByRole("alert")).toBeNull());
  });

  it("says nothing when the edition has classes", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
    const counts = vi.spyOn(api, "classCountsByEdition");
    renderApp("/characters", { api });
    const dialog = await newCharacter(user, "Ayla", "dnd2024");
    await waitFor(() => expect(counts).toHaveBeenCalled());
    expect(within(dialog).queryByRole("alert")).toBeNull();
  });
});

describe("U14 the empty Class list", () => {
  it("names the edition with counts and Start over as 2014 makes a new character, keeping the old one", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters", { api: classless() });
    const dialog = await newCharacter(user, "Ayla", "dnd2024");
    await user.click(within(dialog).getByRole("button", { name: "Create character" }));
    await user.click(await screen.findByRole("tab", { name: "Class" }));
    const note = await screen.findByRole("note", { name: "Why this list is empty" });
    expect(note.textContent).toContain(`This character uses the 2024 rules. ${CLASS_COUNT} classes from 2014 books need a 2014 character.`);
    expect(note.textContent).not.toMatch(/source restrictions/i);
    const oldId = (await api.listCharacters()).find((c) => c.name === "Ayla")!.id;

    await user.click(within(note).getByRole("button", { name: "Start over as 2014" }));
    const confirm = await screen.findByRole("dialog", { name: "Start over as 2014?" });
    expect(confirm.textContent).toContain("Your 2024 character Ayla stays in your collection, unchanged.");
    // a new character is unsaved in the mock: the user chooses whether its changes are saved first
    expect(confirm.textContent).toContain("Ayla has unsaved changes.");
    const save = vi.spyOn(api, "saveCharacter");
    await user.click(within(confirm).getByRole("button", { name: "Save and start over" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith({ characterId: oldId }));

    await waitFor(async () => expect((await api.listCharacters()).filter((c) => c.name === "Ayla")).toHaveLength(2));
    const both = (await api.listCharacters()).filter((c) => c.name === "Ayla");
    expect(both.map((c) => c.ruleset).sort()).toEqual(["dnd2014", "dnd2024"]);
    expect(both.find((c) => c.ruleset === "dnd2024")!.id).toBe(oldId);
    // the new 2014 character is open and its Class list has rows
    await waitFor(() => expect(screen.queryByRole("note", { name: "Why this list is empty" })).toBeNull());
    expect(await screen.findByRole("button", { name: /^Fighter/ })).toBeTruthy();
  });

  it("Start over without saving leaves the saved copy alone", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters", { api: classless() });
    const save = vi.spyOn(api, "saveCharacter");
    const dialog = await newCharacter(user, "Cora", "dnd2024");
    await user.click(within(dialog).getByRole("button", { name: "Create character" }));
    await user.click(await screen.findByRole("tab", { name: "Class" }));
    const note = await screen.findByRole("note", { name: "Why this list is empty" });
    await user.click(within(note).getByRole("button", { name: "Start over as 2014" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Start over as 2014?" })).getByRole("button", { name: "Start over without saving" }));
    await waitFor(async () => expect((await api.listCharacters()).filter((c) => c.name === "Cora")).toHaveLength(2));
    expect(save).not.toHaveBeenCalled();
  });

  it("Cancel keeps everything as it was", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters", { api: classless() });
    const start = vi.spyOn(api, "startOverInEdition");
    const dialog = await newCharacter(user, "Bryn", "dnd2024");
    await user.click(within(dialog).getByRole("button", { name: "Create character" }));
    await user.click(await screen.findByRole("tab", { name: "Class" }));
    const note = await screen.findByRole("note", { name: "Why this list is empty" });
    await user.click(within(note).getByRole("button", { name: "Start over as 2014" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Start over as 2014?" })).getByRole("button", { name: "Cancel" }));
    expect(start).not.toHaveBeenCalled();
    expect((await api.listCharacters()).filter((c) => c.name === "Bryn")).toHaveLength(1);
  });
});

const slot = (emptyReason: SlotEmptyReason | null, elementType: SelectionSlot["elementType"] = "class"): SelectionSlot => ({
  id: "s1", tab: "class", groupId: null, groupLabel: null, classSlotId: null, title: elementType === "class" ? "Class" : "Gift",
  elementType, ordinal: 1, of: 1, required: true, locked: false, chosen: null, requiredLevel: null, allowReplace: false, hint: null,
  gainedOnLevel: 1, replaced: [], emptyReason, problem: null,
});

function note(s: SelectionSlot, handlers: Partial<Parameters<typeof EmptyListNote>[0]> = {}) {
  render(<EmptyListNote slot={s} ruleset="dnd2024" onManageSources={() => undefined} onStartOver={() => undefined} onEditionOptions={() => undefined} {...handlers} />);
  return screen.getByRole("note", { name: "Why this list is empty" });
}

describe("U14 EmptyListNote", () => {
  it("blames source restrictions only when they hide options", () => {
    const onManageSources = vi.fn();
    const n = note(slot({ otherEdition: null, restricted: 3 }), { onManageSources });
    expect(n.textContent).toBe("Nothing matches. 3 classes are hidden by your source restrictions.Manage Sources");
    within(n).getByRole("button", { name: "Manage Sources" }).click();
    expect(onManageSources).toHaveBeenCalled();
    expect(within(n).queryByRole("button", { name: /Start over/ })).toBeNull();
  });

  it("with no hidden options it does not mention restrictions", () => {
    const n = note(slot(null));
    expect(n.textContent).toBe("Nothing matches.");
  });

  it("names both causes, with one action each", () => {
    const n = note(slot({ otherEdition: { edition: "dnd2014", rejected: 14, notAllowed: 0 }, restricted: 24 }));
    expect(n.textContent).toBe(
      "This character uses the 2024 rules. 14 classes from 2014 books need a 2014 character.Start over as 2014 24 classes are hidden by your source restrictions.Manage Sources",
    );
  });

  it("other lists: a strict character is pointed at its edition options, and only the Class list offers Start over", () => {
    const onEditionOptions = vi.fn();
    const n = note(slot({ otherEdition: { edition: "dnd2014", rejected: 1, notAllowed: 2 }, restricted: 0 }, "feat"), { onEditionOptions });
    expect(n.textContent).toBe(
      "This character uses the 2024 rules. 1 feat from 2014 books needs a 2014 character. 2 feats from 2014 books are hidden: this character does not allow the other edition.Edition & compatibility",
    );
    expect(within(n).queryByRole("button", { name: /Start over/ })).toBeNull();
    within(n).getByRole("button", { name: "Edition & compatibility" }).click();
    expect(onEditionOptions).toHaveBeenCalled();
  });
});
