import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { renderApp, seededId } from "./utils";

// B-222 (decision U17b): a character links to a note of the Obsidian vault from Manage > Character. Link an existing note, create
// one from the character, open it in Start > Notes; a missing note or an unreadable vault says so and offers to link again. The
// backend side (the file field, the note text, name clashes, path checks) is tested in Rust.
type User = ReturnType<typeof userEvent.setup>;

const NOTES: Record<string, string> = {
  "Home.md": "# Home\n\nThe campaign.\n",
  "Places/Harbor Town.md": "# Harbor Town\n\nA fishing port.\n",
  "map.png": "data:image/png;base64,AA==",
};

async function setup(opts: { vault?: boolean; mobile?: boolean } = {}) {
  const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5, mobile: opts.mobile ?? false });
  api.vault.load(NOTES);
  if (opts.vault ?? true) {
    const [pick] = await api.pickPath({ purpose: "vault-dir" });
    await api.setVaultDir({ token: pick!.token });
  }
  const id = await seededId(api, "Brannoc");
  const name = (await api.getSnapshot({ characterId: id })).name;
  return { api, id, name };
}

const section = () => screen.findByRole("region", { name: "Notes" });
const linkedNote = async (api: MockApi, id: string) => (await api.getSnapshot({ characterId: id })).vaultNote;

async function linkHarbor(user: User) {
  await user.click(within(await section()).getByRole("button", { name: /^Link (another )?note\.\.\.$/ }));
  const dialog = await screen.findByRole("dialog", { name: "Link a note to this character" });
  // Notes only: the image is not offered.
  expect(within(dialog).queryByRole("button", { name: "map.png" })).toBeNull();
  await user.type(within(dialog).getByRole("searchbox", { name: "Find a note" }), "harbor");
  await user.click(within(dialog).getByRole("button", { name: "Places/Harbor Town" }));
}

afterEach(() => { setPhoneForTests(false); setPhoneOsForTests(null); });

describe("B-222 a character's vault note", () => {
  it("without a vault says where to set one and offers no link", async () => {
    const { api, id } = await setup({ vault: false });
    renderApp("/manage", { api, openCharacterId: id });
    const notes = await section();
    expect(within(notes).getByText("Set a vault in Settings > Content to link this character to a note.")).toBeTruthy();
    expect(within(notes).getByRole("button", { name: "Open Settings" })).toBeTruthy();
    expect(within(notes).queryByRole("button", { name: "Link note..." })).toBeNull();
    expect(within(notes).queryByRole("button", { name: "Create note" })).toBeNull();
  });

  it("links an existing note from the searchable picker and keeps its vault path on the character", async () => {
    const user = userEvent.setup();
    const { api, id } = await setup();
    renderApp("/manage", { api, openCharacterId: id });
    expect(within(await section()).getByText("No note linked")).toBeTruthy();
    await linkHarbor(user);
    await waitFor(async () => expect(await linkedNote(api, id)).toBe("Places/Harbor Town.md"));
    const notes = await section();
    expect(within(notes).getByText("Places/Harbor Town")).toBeTruthy();
    expect(within(notes).getByRole("button", { name: "Open note" })).toBeTruthy();
    await user.click(within(notes).getByRole("button", { name: "Unlink" }));
    await waitFor(async () => expect(await linkedNote(api, id)).toBeNull());
  });

  it("opens the linked note in Start > Notes", async () => {
    const user = userEvent.setup();
    const { api, id } = await setup();
    await api.setCharacterNote({ characterId: id, path: "Places/Harbor Town.md" });
    renderApp("/manage", { api, openCharacterId: id });
    await user.click(within(await section()).getByRole("button", { name: "Open note" }));
    const reading = await screen.findByRole("article", { name: "Reading view" });
    expect(within(reading).getByText("A fishing port.")).toBeTruthy();
    expect(api.vault.opened).toContain("Places/Harbor Town.md");
  });

  it("creates a note from the character, numbered when the name is taken, and links it", async () => {
    const user = userEvent.setup();
    const { api, id, name } = await setup();
    api.vault.load({ ...NOTES, [`Characters/${name}.md`]: "Someone else's note.\n" });
    renderApp("/manage", { api, openCharacterId: id });
    await user.click(within(await section()).getByRole("button", { name: "Create note" }));
    const path = `Characters/${name} 2.md`;
    await waitFor(async () => expect(await linkedNote(api, id)).toBe(path));
    expect(api.vault.files.get(`Characters/${name}.md`)?.text).toBe("Someone else's note.\n");
    const text = api.vault.files.get(path)!.text;
    expect(text).toMatch(/^---\nname: /);
    expect(text).toContain("tags: [character]");
    expect(text).toContain("## Backstory\n\n## Notes\n\n## Sessions\n");
    expect(within(await section()).getByText(`Characters/${name} 2`)).toBeTruthy();
  });

  it("a missing note says so and offers to link another or create one, without leaving the character", async () => {
    const user = userEvent.setup();
    const { api, id } = await setup();
    await api.setCharacterNote({ characterId: id, path: "Home.md" });
    api.vault.externalWrite("Home.md", null);
    renderApp("/manage", { api, openCharacterId: id });
    const notes = await section();
    await user.click(within(notes).getByRole("button", { name: "Open note" }));
    expect((await within(notes).findByRole("alert")).textContent).toMatch(/The note 'Home' is no longer in your vault/);
    expect(screen.queryByRole("article", { name: "Reading view" })).toBeNull();
    expect(within(notes).getByRole("button", { name: "Create note" })).toBeTruthy();
    await linkHarbor(user);
    await waitFor(async () => expect(await linkedNote(api, id)).toBe("Places/Harbor Town.md"));
    await waitFor(() => expect(within(notes).queryByRole("alert")).toBeNull());
  });

  it("an unavailable vault says so and offers to relink", async () => {
    const user = userEvent.setup();
    const { api, id } = await setup();
    await api.setCharacterNote({ characterId: id, path: "Home.md" });
    api.vault.unavailable = true;
    renderApp("/manage", { api, openCharacterId: id });
    const notes = await section();
    await user.click(within(notes).getByRole("button", { name: "Open note" }));
    expect((await within(notes).findByRole("alert")).textContent).toMatch(/vault folder is unavailable/);
    expect(within(notes).getByRole("button", { name: "Link another note..." })).toBeTruthy();
  });

  it("refuses a link outside the vault or to a file that is not a note", async () => {
    const { api, id } = await setup();
    await expect(api.setCharacterNote({ characterId: id, path: "../secret.md" })).rejects.toMatchObject({ code: "invalid_argument" });
    await expect(api.setCharacterNote({ characterId: id, path: "map.png" })).rejects.toMatchObject({ code: "invalid_argument" });
    expect(await linkedNote(api, id)).toBeNull();
  });

  it("is on Manage > Character on iPhone and iPad, and hidden on Android until it has vault access", async () => {
    setPhoneForTests(true);
    setPhoneOsForTests("ios");
    const { api, id } = await setup({ mobile: true });
    const { unmount } = renderApp("/manage", { api, openCharacterId: id });
    expect(within(await section()).getByRole("button", { name: "Create note" })).toBeTruthy();
    unmount();
    setPhoneOsForTests("android");
    renderApp("/manage", { api, openCharacterId: id });
    await screen.findByLabelText("Character name");
    expect(screen.queryByRole("region", { name: "Notes" })).toBeNull();
  });
});
