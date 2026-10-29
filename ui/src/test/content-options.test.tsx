import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

// Issue 4: Aurora's New Character and Manage Character Options sliders list the content's own
// `Option` elements after the three built-ins, tagged "custom", checked when their `default`
// setter is true. The mock lists two (Customized Language off, Firearms on by default).
const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
const cardTitles = (root: HTMLElement) =>
  Array.from(root.querySelectorAll(".v4-opt label")).map((l) => (l.textContent ?? "").replace(/\s+/g, " ").trim().toUpperCase());

describe("content options (issue 4)", () => {
  it("the New Character dialog lists them after the built-ins and the choice reaches the character", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters", { api: fast() });
    await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
    const dialog = await screen.findByRole("dialog", { name: "New character" });
    await within(dialog).findByRole("checkbox", { name: /customized language/i });
    expect(cardTitles(dialog)).toEqual([
      "AVERAGE HIT POINTS", "FEATS", "MULTICLASSING", "CUSTOMIZED LANGUAGECUSTOM", "FIREARMSCUSTOM", "RULES EDITION",
    ]);
    const language = within(dialog).getByRole("checkbox", { name: /customized language/i }) as HTMLInputElement;
    const firearms = within(dialog).getByRole("checkbox", { name: /firearms/i }) as HTMLInputElement;
    expect([language.checked, firearms.checked]).toEqual([false, true]);
    await user.click(language);
    await user.click(firearms);
    await user.type(within(dialog).getByLabelText("Name"), "Optioned");
    await user.click(within(dialog).getByRole("button", { name: "Create character" }));
    await waitFor(async () => expect((await api.listCharacters()).some((c) => c.name === "Optioned")).toBe(true));
    const id = (await api.listCharacters()).find((c) => c.name === "Optioned")!.id;
    const snap = await api.getSnapshot({ characterId: id });
    expect(snap.options.custom).toEqual({ ID_MOCK_OPTION_CUSTOMIZED_LANGUAGE: true, ID_MOCK_OPTION_FIREARMS: false });
  });

  it("F8 shows the built-ins (which the backend does not list) and the content options", async () => {
    const user = userEvent.setup();
    const api = fast();
    const id = await seededId(api, "Brannoc");
    renderApp("/builder", { api, openCharacterId: id });
    await screen.findByRole("menuitem", { name: /Brannoc/i });
    await user.keyboard("{F8}");
    const dialog = await screen.findByRole("dialog", { name: "Character options" });
    expect(cardTitles(dialog)).toEqual(["AVERAGE HIT POINTS", "FEATS", "MULTICLASSING", "CUSTOMIZED LANGUAGECUSTOM", "FIREARMSCUSTOM"]);
  });

  it("Create during an edition switch sends no option of the previous edition (review L4)", async () => {
    const user = userEvent.setup();
    const api = fast();
    const only2024 = { id: "ID_MOCK_OPTION_2024_ONLY", name: "Only 2024", description: "A 2024 option.", locked: false, defaultOn: false };
    vi.spyOn(api, "listCharacterOptions").mockImplementation(({ ruleset }) =>
      ruleset === "dnd2024" ? Promise.resolve([only2024]) : new Promise(() => undefined));
    const create = vi.spyOn(api, "createCharacter");
    renderApp("/characters", { api });
    await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
    const dialog = await screen.findByRole("dialog", { name: "New character" });
    await user.selectOptions(within(dialog).getByLabelText("Rules edition"), "dnd2024");
    await user.click(await within(dialog).findByRole("checkbox", { name: /only 2024/i }));
    await user.selectOptions(within(dialog).getByLabelText("Rules edition"), "dnd2014");
    expect(within(dialog).queryByRole("checkbox", { name: /only 2024/i })).toBeNull();
    await user.type(within(dialog).getByLabelText("Name"), "Switcher");
    await user.click(within(dialog).getByRole("button", { name: "Create character" }));
    await waitFor(() => expect(create).toHaveBeenCalledTimes(1));
    const request = create.mock.calls[0]![0].request;
    expect(request.ruleset).toBe("dnd2014");
    expect(Object.keys(request.options.custom ?? {})).not.toContain("ID_MOCK_OPTION_2024_ONLY");
  });
});
