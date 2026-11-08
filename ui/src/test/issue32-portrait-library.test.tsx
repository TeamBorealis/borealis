import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { PortraitLibrary } from "../api/contract";
import { PORTRAIT_LIBRARY_UNAVAILABLE, describeFolders } from "../components/shell/useFolderNotice";
import { libraryNote } from "../pages/manage/PortraitLibraryTiles";
import { renderApp, seededId } from "./utils";

// GitHub #32 (suggested by Tjop Vaoxad): a portrait library folder in Settings > Content, shown in the portraits gallery.
const fast = (opts: { mobile?: boolean } = {}) => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5, ...opts });
const LIBRARY = "/home/me/Pictures/Portraits";
type User = ReturnType<typeof userEvent.setup>;

async function openContentTab(user: User) {
  await user.click(await screen.findByRole("menuitem", { name: "View" }));
  await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
  const dialog = await screen.findByRole("dialog", { name: "Application Settings" });
  await user.click(within(dialog).getByRole("tab", { name: "Content" }));
  return dialog;
}

const libraryBox = (dialog: HTMLElement) => within(dialog).getByLabelText("Portrait library") as HTMLInputElement;

/** Choose the library folder through the settings window (as a user would) and save. */
async function chooseLibrary(api: MockApi) {
  const [pick] = await api.pickPath({ purpose: "portrait-library-dir" });
  await api.setPortraitLibraryDir({ token: pick!.token });
}

async function openGallery(user: User, api: MockApi) {
  const id = await seededId(api, "Seraphine");
  renderApp("/builder", { api, openCharacterId: id });
  await user.click(await screen.findByRole("button", { name: "Open portraits" }));
  return { id, gallery: await screen.findByRole("dialog", { name: "Portraits gallery" }) };
}

afterEach(() => { try { localStorage.removeItem("borealis.portraits"); } catch { /* ignore */ } });

describe("Settings > Content: portrait library row (#32)", () => {
  it("starts empty, Browse holds the folder until Save, and Save stores it", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/characters", { api });
    const dialog = await openContentTab(user);
    expect(libraryBox(dialog).value).toBe("");
    expect(within(dialog).getByRole("button", { name: /Clear portrait library/ })).toHaveProperty("disabled", true);
    await user.click(within(dialog).getByRole("button", { name: "Browse for portrait library" }));
    await waitFor(() => expect(libraryBox(dialog).value).toBe(LIBRARY));
    expect((await api.getSettings()).portraitLibraryDir).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Application Settings" })).toBeNull());
    expect((await api.getSettings()).portraitLibraryDir).toBe(LIBRARY);
    expect((await api.getDataDirs()).portraitLibraryDir).toBe(LIBRARY);
  });

  it("Clear empties the row and Save turns the library off; Cancel keeps it", async () => {
    const user = userEvent.setup();
    const api = fast();
    await chooseLibrary(api);
    renderApp("/characters", { api });
    let dialog = await openContentTab(user);
    await waitFor(() => expect(libraryBox(dialog).value).toBe(LIBRARY));
    await user.click(within(dialog).getByRole("button", { name: /Clear portrait library/ }));
    expect(libraryBox(dialog).value).toBe("");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect((await api.getSettings()).portraitLibraryDir).toBe(LIBRARY);
    dialog = await openContentTab(user);
    await waitFor(() => expect(libraryBox(dialog).value).toBe(LIBRARY));
    await user.click(within(dialog).getByRole("button", { name: /Clear portrait library/ }));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Application Settings" })).toBeNull());
    expect((await api.getSettings()).portraitLibraryDir).toBeNull();
  });

  it("Restore defaults keeps the library folder (folders are user data, B-226)", async () => {
    const user = userEvent.setup();
    const api = fast();
    await chooseLibrary(api);
    await api.saveSettings({ patch: { playerName: "Old" } });
    renderApp("/characters", { api });
    const dialog = await openContentTab(user);
    await waitFor(() => expect(libraryBox(dialog).value).toBe(LIBRARY));
    await user.click(within(dialog).getByRole("button", { name: "Restore defaults" }));
    await waitFor(async () => expect((await api.getSettings()).playerName).toBe("Player One"));
    expect((await api.getSettings()).portraitLibraryDir).toBe(LIBRARY);
    expect((await api.getDataDirs()).portraitLibraryDir).toBe(LIBRARY);
    await waitFor(() => expect(libraryBox(dialog).value).toBe(LIBRARY));
  });

  it("an unavailable folder keeps its path and shows a warning, also in the startup folder notice", async () => {
    const user = userEvent.setup();
    const api = fast();
    await chooseLibrary(api);
    api.portraitLibrary.unavailable = true;
    renderApp("/characters", { api });
    const dialog = await openContentTab(user);
    await waitFor(() => expect(libraryBox(dialog).value).toBe(LIBRARY));
    const notes = within(dialog).getAllByRole("status").map((n) => n.textContent ?? "");
    expect(notes.some((t) => t.includes(PORTRAIT_LIBRARY_UNAVAILABLE))).toBe(true);
    expect(within(dialog).getByRole("button", { name: /Clear portrait library/ })).toHaveProperty("disabled", false);
    expect(describeFolders(await api.getDataDirs())).toBe(PORTRAIT_LIBRARY_UNAVAILABLE);
  });

  it("phones have no folder picker, so the row is hidden there", async () => {
    const user = userEvent.setup();
    renderApp("/more", { api: fast({ mobile: true }) });
    await user.click(await screen.findByRole("button", { name: "Search settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Application Settings" });
    await user.click(within(dialog).getByRole("tab", { name: "Content" }));
    expect(await within(dialog).findByRole("button", { name: "Manage…" })).toBeTruthy();
    expect(within(dialog).queryByLabelText("Portrait library")).toBeNull();
  });
});

describe("Portraits gallery: library images (#32)", () => {
  it("shows the library images after the device images, and picking one sets the portrait from the backend's image", async () => {
    const user = userEvent.setup();
    const api = fast();
    await chooseLibrary(api);
    const { id, gallery } = await openGallery(user, api);
    const tiles = await within(gallery).findAllByRole("button", { name: /^Library portrait / });
    expect(tiles.map((t) => t.getAttribute("aria-label"))).toEqual([
      "Library portrait elf-female.png", "Library portrait human-male.jpg", "Library portrait Dwarves/shield-dwarf.png",
    ]);
    expect(within(gallery).getByText("3 images from your portrait library.")).toBeTruthy();
    const dwarf = within(gallery).getByRole("button", { name: "Library portrait Dwarves/shield-dwarf.png" });
    await waitFor(() => expect(dwarf.querySelector("img")).not.toBeNull());
    await user.click(dwarf);
    expect(dwarf.getAttribute("aria-pressed")).toBe("true");
    await user.click(within(gallery).getByRole("button", { name: "Select portrait" }));
    const expected = await api.portraitLibraryImage({ id: "Dwarves/shield-dwarf.png" });
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).portrait).toBe(expected));
    expect(screen.queryByRole("dialog", { name: "Portraits gallery" })).toBeNull();
  });

  it("a double click on a library image selects it at once", async () => {
    const user = userEvent.setup();
    const api = fast();
    await chooseLibrary(api);
    const { id, gallery } = await openGallery(user, api);
    const elf = await within(gallery).findByRole("button", { name: "Library portrait elf-female.png" });
    await waitFor(() => expect(elf.querySelector("img")).not.toBeNull());
    await user.dblClick(elf);
    const expected = await api.portraitLibraryImage({ id: "elf-female.png" });
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).portrait).toBe(expected));
  });

  it("without a library the gallery is as before", async () => {
    const user = userEvent.setup();
    const { gallery } = await openGallery(user, fast());
    await within(gallery).findByText(/Use Browse to add your own portraits/);
    expect(within(gallery).queryAllByRole("button", { name: /^Library portrait / })).toHaveLength(0);
    expect(within(gallery).queryByRole("status")).toBeNull();
  });

  it("an unavailable library shows the warning and no library images", async () => {
    const user = userEvent.setup();
    const api = fast();
    await chooseLibrary(api);
    api.portraitLibrary.unavailable = true;
    const { gallery } = await openGallery(user, api);
    expect((await within(gallery).findByRole("status")).textContent).toContain(PORTRAIT_LIBRARY_UNAVAILABLE);
    expect(within(gallery).queryAllByRole("button", { name: /^Library portrait / })).toHaveLength(0);
  });

  it("the note counts the images and says what was left out", () => {
    const lib = (patch: Partial<PortraitLibrary>): PortraitLibrary => ({ dir: LIBRARY, unavailable: false, images: [], refused: 0, truncated: false, ...patch });
    const one = { id: "a.png", name: "a.png", folder: null };
    expect(libraryNote(null)).toBeNull();
    expect(libraryNote(lib({ dir: null }))).toBeNull();
    expect(libraryNote(lib({ unavailable: true }))).toBeNull();
    expect(libraryNote(lib({}))).toBe("Your portrait library folder has no images.");
    expect(libraryNote(lib({ images: [one], refused: 2, truncated: true }))).toBe(
      "1 image from your portrait library. Only the first 1 are shown. 2 images could not be used (larger than 2 MiB, or not a PNG, JPEG, GIF or WebP image).",
    );
  });
});
