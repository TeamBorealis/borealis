import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { renderApp, seededId } from "./utils";

// B-148 (tester video C15): on a phone, Save Character Sheet toasted "Exported Sable_Marrow.pdf" with no place and no way to
// open, share or print it. Phones now save through the system save picker: the toast names where the file went (Android
// then opens it in the PDF viewer), and a cancelled picker says nothing. Desktop keeps its toast.

function asPhone(os: "ios" | "android") {
  document.documentElement.classList.add("phone");
  setPhoneForTests(true);
  setPhoneOsForTests(os);
}

afterEach(() => {
  setPhoneForTests(false);
  setPhoneOsForTests(null);
  document.documentElement.classList.remove("phone");
  vi.restoreAllMocks();
});

async function openSheet() {
  const api = new MockApi({ rng: () => 0.5 });
  const id = await seededId(api, "Brannoc");
  renderApp("/sheet", { api, openCharacterId: id });
  await screen.findByRole("button", { name: /^(Generate Preview|Generating\.\.\.)$/ });
  return { api, id, user: userEvent.setup() };
}

const saved = (path: string | null) => ({ fileName: "Brannoc.pdf", mime: "application/pdf", path, dataBase64: null });

describe("B-148 Save Character Sheet on a phone", () => {
  it("names where the file went, and never claims Exported without a place", async () => {
    asPhone("android");
    const { api, id, user } = await openSheet();
    const save = vi.spyOn(api, "exportSheet").mockResolvedValue(saved("Download/Brannoc.pdf"));
    await user.click(screen.getByRole("button", { name: "Save Character Sheet" }));
    expect(await screen.findByText("Saved to Download/Brannoc.pdf")).toBeTruthy();
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ characterId: id, options: expect.objectContaining({ format: "pdf" }) }));
    expect(screen.queryByText(/^Exported /)).toBeNull();
  });

  it("a cancelled save picker says nothing", async () => {
    asPhone("ios");
    const { api, user } = await openSheet();
    const save = vi.spyOn(api, "exportSheet").mockResolvedValue(saved(null));
    await user.click(screen.getByRole("button", { name: "Save Character Sheet" }));
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByText(/^(Exported|Saved to) /)).toBeNull();
  });

  it("Save in the full-screen preview is the same save, and opening the preview never opens the picker", async () => {
    asPhone("ios");
    const { api, user } = await openSheet();
    const save = vi.spyOn(api, "exportSheet").mockResolvedValue(saved("iCloud Drive/Brannoc.pdf"));
    await user.click(screen.getByRole("button", { name: "Generate Preview" }));
    const dialog = await screen.findByRole("dialog", { name: "Character sheet preview" });
    await within(dialog).findByTitle("Character sheet preview");
    await waitFor(() => expect((within(dialog).getByRole("button", { name: "Share" }) as HTMLButtonElement).disabled).toBe(false));
    expect(save).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved to iCloud Drive/Brannoc.pdf")).toBeTruthy();
    expect(save).toHaveBeenCalledTimes(1);
  });
});

describe("B-148 desktop keeps its save toast (U7)", () => {
  it("a desktop save names the path, as before", async () => {
    const { api, user } = await openSheet();
    vi.spyOn(api, "exportSheet").mockResolvedValue(saved("/tmp/sheets/Brannoc.pdf"));
    await user.click(screen.getByRole("button", { name: "Save Character Sheet" }));
    expect(await screen.findByText("Saved to /tmp/sheets/Brannoc.pdf")).toBeTruthy();
  });
});
