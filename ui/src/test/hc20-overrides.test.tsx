import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { setPhoneForTests } from "../lib/phone";
import { ORIGINAL_CHANGED, ORIGINAL_MISSING } from "../pages/homebrew/Overrides";
import { renderApp } from "./utils";

// HC20 overrides (homebrew-creator spec 22) on the mock, desktop and phone: "Copy to homebrew" and New element offer
// "Override the original"; the override opens under the original's name with a notice and saves under its id; the list
// notes it; the original shows "Changed by <pack>" with Revert, which brings it back; the notice says when the original
// changed underneath or is not loaded.

afterEach(() => act(() => setPhoneForTests(false)));

const nameBox = () => screen.getByRole("textbox", { name: /^Name/ }) as HTMLInputElement;
const editor = () => screen.getByRole("region", { name: "Element editor" });

async function emberIsles(api: MockApi) {
  return api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
}

/** The Compendium (F1, as on every layout) opened on the sample Fireball. */
async function fireball(user: ReturnType<typeof userEvent.setup>) {
  fireEvent.keyDown(window, { key: "F1" });
  const dlg = await screen.findByRole("dialog", { name: "Compendium" });
  await user.type(within(dlg).getByRole("searchbox", { name: "Search the compendium" }), "fireball");
  await user.click(await within(dlg).findByRole("button", { name: /^Fireball/ }));
  return dlg;
}

describe.each([
  ["desktop", false],
  ["phone", true],
])("HC20 overrides (%s)", (_layout, phone) => {
  it("New element > Override the original... opens the SRD Fireball under its own name with the notice, and saves it", async () => {
    act(() => setPhoneForTests(phone));
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await emberIsles(api);
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click((await screen.findAllByRole("button", { name: /^New( element)?$/ }))[0]!);
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    await user.click(within(flyout).getByRole("radio", { name: /^Spell A spell/ }));
    await user.click(within(flyout).getByRole("radio", { name: /^Override the original/ }));
    expect(within(flyout).queryByRole("checkbox", { name: /Copy its features/ })).toBeNull();
    await user.click(within(flyout).getByRole("button", { name: "Choose..." }));
    const picker = await screen.findByRole("dialog", { name: /Spell to override/ });
    const rows = await within(picker).findAllByRole("button", { name: /Fireball/ });
    await user.click(rows.find((r) => /SRD 5\.2\.1/.test(r.closest("tr, li, [role=row]")?.textContent ?? r.textContent ?? "")) ?? rows[0]!);
    await user.click(within(picker).getByRole("button", { name: "Choose" }));
    await waitFor(() => expect(nameBox().value).toBe("Fireball"));
    const notice = within(editor()).getByRole("note", { name: "Override" });
    expect(notice.textContent).toMatch(/Saving changes it everywhere; characters that use it get the change\./);
    expect(within(notice).queryByRole("button", { name: "Revert" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(async () => {
      const rows = await api.homebrewListElements({ packId: pack.id });
      expect(rows.map((r) => [r.name, r.override?.srd])).toEqual([["Fireball", true]]);
    });
    // saved, the notice offers Revert
    expect(await within(within(editor()).getByRole("note", { name: "Override" })).findByRole("button", { name: "Revert" })).toBeTruthy();
  });

  it("Copy to homebrew > Override the original changes the Fireball; the Compendium shows Changed by with Revert", async () => {
    act(() => setPhoneForTests(phone));
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await emberIsles(api);
    renderApp("/characters", { api });
    await screen.findAllByRole("button", { name: /new character/i });
    let dlg = await fireball(user);
    await user.click(await within(dlg).findByRole("link", { name: "Copy to homebrew" }));
    const copy = await screen.findByRole("dialog", { name: "Copy to homebrew" });
    expect((within(copy).getByRole("radio", { name: /Make a variant \(copy\)/ }) as HTMLInputElement).checked).toBe(true);
    await user.click(within(copy).getByRole("radio", { name: /Override the original/ }));
    await user.click(within(copy).getByRole("button", { name: "Override" }));
    await waitFor(() => expect(nameBox().value).toBe("Fireball"));
    expect(within(editor()).getByRole("note", { name: "Override" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(async () => expect((await api.homebrewListElements({ packId: pack.id })).map((r) => r.name)).toEqual(["Fireball"]));
    // the element list notes what it changes (on phones in the row's detail line, once the list shows)
    if (!phone) expect(await screen.findByText(/^Changes /)).toBeTruthy();

    // the original now says who changed it, with Revert
    dlg = await fireball(user);
    const line = await within(dlg).findByRole("note", { name: "Changed by" });
    expect(line.textContent).toMatch(/Changed by\. Ember Isles/);
    await user.click(within(line).getByRole("button", { name: /Revert/ }));
    await waitFor(() => expect(within(dlg).queryByRole("note", { name: "Changed by" })).toBeNull());
    expect(await api.homebrewListElements({ packId: pack.id })).toEqual([]);
    expect(await screen.findByText("Reverted. The original is back.")).toBeTruthy();
  });

  it.each([
    ["changed", ORIGINAL_CHANGED, "Original changed"],
    ["missing", ORIGINAL_MISSING, "Original not loaded"],
  ] as const)("the notice says when the original is %s", async (state, sentence, note) => {
    act(() => setPhoneForTests(phone));
    const api = new MockApi({ creatorEnabled: true });
    const pack = await emberIsles(api);
    const fresh = await api.homebrewNewOverride({ packId: pack.id, elementId: "ID_2024_SPELL_FIREBALL" });
    await api.homebrewSaveDraft({ draftId: fresh.draftId });
    const flags = { changed: state === "changed", missing: state === "missing" };
    const list = api.homebrewListElements.bind(api);
    vi.spyOn(api, "homebrewListElements").mockImplementation(async (a) =>
      (await list(a)).map((r) => (r.override ? { ...r, override: { ...r.override, ...flags } } : r)));
    const open = api.homebrewOpenDraft.bind(api);
    vi.spyOn(api, "homebrewOpenDraft").mockImplementation(async (a) => {
      const v = await open(a);
      return v.override ? { ...v, override: { ...v.override, ...flags } } : v;
    });
    renderApp(`/homebrew/${pack.id}?element=ID_2024_SPELL_FIREBALL`, { api });
    const alert = await within(await screen.findByRole("note", { name: "Override" })).findByRole("alert");
    expect(alert.textContent).toBe(sentence);
    if (!phone) expect(await screen.findByText(note)).toBeTruthy();
  });
});

describe("HC20 refusals (mock)", () => {
  it("an element another pack already changes is refused with a plain sentence", async () => {
    const api = new MockApi({ creatorEnabled: true });
    const a = await emberIsles(api);
    const b = await api.homebrewCreatePack({ request: { name: "Frost Reach", code: "FRR", edition: "dnd2024" } });
    const v = await api.homebrewNewOverride({ packId: a.id, elementId: "ID_2024_SPELL_FIREBALL" });
    await api.homebrewSaveDraft({ draftId: v.draftId });
    await expect(api.homebrewNewOverride({ packId: b.id, elementId: "ID_2024_SPELL_FIREBALL" }))
      .rejects.toMatchObject({ message: "Ember Isles already changes this element. Change it there, or revert that change first." });
    expect((await api.getElement({ elementId: "ID_2024_SPELL_FIREBALL" })).changedBy).toEqual({ packId: a.id, packName: "Ember Isles" });
  });
});
