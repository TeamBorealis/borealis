import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { renderApp, seededId } from "./utils";

// B-153 (#18) / B-154 (#19): every character portrait in the builder opens the portraits gallery, on phones too.
const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
const GALLERY = { name: "Portraits gallery" } as const;

/** jsdom has no matchMedia: answer the narrow-shell query (max-width: 899px) as `narrow`. */
function narrowScreen(narrow: boolean): () => void {
  const original = window.matchMedia;
  window.matchMedia = ((q: string) => ({
    matches: q.includes("max-width: 899px") ? narrow : false,
    media: q, onchange: null, addEventListener: () => undefined, removeEventListener: () => undefined,
    addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false,
  })) as typeof window.matchMedia;
  return () => { window.matchMedia = original; };
}

async function openBuilder(): Promise<{ api: MockApi; id: string }> {
  const api = fast();
  const id = await seededId(api, "Seraphine");
  renderApp("/builder", { api, openCharacterId: id });
  return { api, id };
}

describe("desktop builder portraits (#18)", () => {
  it("the large portrait in the character quick view opens the portraits gallery and closes the quick view", async () => {
    const user = userEvent.setup();
    await openBuilder();
    await user.click(await screen.findByRole("button", { name: /^Seraphine/ }));
    const quick = await screen.findByRole("dialog", { name: /Seraphine/ });
    await user.click(within(quick).getByRole("button", { name: "Open portraits" }));
    expect(await screen.findByRole("dialog", GALLERY)).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /Seraphine/ })).toBeNull());
  });

  it("the profile portrait opens the gallery from the keyboard (Enter)", async () => {
    const user = userEvent.setup();
    await openBuilder();
    const portrait = await screen.findByRole("button", { name: "Open portraits" });
    portrait.focus();
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("dialog", GALLERY)).toBeTruthy();
  });

  it("the quick view portrait opens the gallery from the keyboard (Space)", async () => {
    const user = userEvent.setup();
    await openBuilder();
    await user.click(await screen.findByRole("button", { name: /^Seraphine/ }));
    const quick = await screen.findByRole("dialog", { name: /Seraphine/ });
    within(quick).getByRole("button", { name: "Open portraits" }).focus();
    await user.keyboard(" ");
    expect(await screen.findByRole("dialog", GALLERY)).toBeTruthy();
  });
});

describe("narrow shell portraits (#18 narrow window, #19 phone)", () => {
  let restore: () => void = () => undefined;
  beforeEach(() => { restore = narrowScreen(true); });
  afterEach(() => { restore(); setPhoneForTests(false); });

  it("a narrow desktop window shows the portrait in the title bar and a click opens the gallery", async () => {
    const user = userEvent.setup();
    await openBuilder();
    const title = await screen.findByRole("banner");
    await user.click(await within(title).findByRole("button", { name: "Open portraits" }));
    expect(await screen.findByRole("dialog", GALLERY)).toBeTruthy();
  });

  it("on a phone, tapping the title-bar portrait opens the gallery, and Browse adds an image that becomes the portrait", async () => {
    setPhoneForTests(true);
    const user = userEvent.setup();
    const { api, id } = await openBuilder();
    const title = await screen.findByRole("banner");
    await user.click(await within(title).findByRole("button", { name: "Open portraits" }));
    const gallery = await screen.findByRole("dialog", GALLERY);
    expect(within(gallery).getByRole("button", { name: "Browse" })).toBeTruthy();
    const image = new File([new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10])], "hero.png", { type: "image/png" });
    await user.upload(within(gallery).getByLabelText("Add a portrait image"), image);
    expect(await within(gallery).findByRole("button", { name: "Portrait 1" })).toBeTruthy();
    await user.click(within(gallery).getByRole("button", { name: "Select portrait" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).portrait).toMatch(/^data:image\/png;base64,/));
  });

  it("on a phone, the menu's Portraits Gallery entry opens the gallery", async () => {
    setPhoneForTests(true);
    const user = userEvent.setup();
    await openBuilder();
    await within(await screen.findByRole("banner")).findByText(/^Seraphine/);
    await user.click(screen.getByRole("button", { name: "Menu" }));
    await user.click(await screen.findByRole("button", { name: /Portraits Gallery/ }));
    expect(await screen.findByRole("dialog", GALLERY)).toBeTruthy();
  });

  it("with no character open the title bar has no portrait button", async () => {
    setPhoneForTests(true);
    renderApp("/characters", { api: fast() });
    const title = await screen.findByRole("banner");
    await screen.findByRole("button", { name: "Settings" });
    expect(within(title).queryByRole("button", { name: "Open portraits" })).toBeNull();
  });
});
