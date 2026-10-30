import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import type { CharacterSnapshot } from "../api/contract";
import { renderApp, seededId } from "./utils";

// GitHub #11 (C1): content problems no longer make a wall of banners on every Build page. The backend reports a broken
// expression only for content the character holds or is offered, and puts it on the affected choice (`problem`); whatever
// notices remain are one compact notice that expands. The full list stays in the import report.
const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
const NOTICES = [
  "ID_GTPG25_CLASS has unparsable expressions in: rule 0, rule 1, rule 2",
  "ID_GTPG25_CF_ULTIMATE has unparsable expressions in: requirements",
  "A user content pack could not be loaded: broken.json",
];

async function openWith(patch: (s: CharacterSnapshot) => CharacterSnapshot) {
  const api = fast();
  const id = await seededId(api, "Brannoc");
  const load = api.loadCharacter.bind(api);
  const snapshot = api.getSnapshot.bind(api);
  vi.spyOn(api, "loadCharacter").mockImplementation(async (a) => patch(await load(a)));
  vi.spyOn(api, "getSnapshot").mockImplementation(async (a) => patch(await snapshot(a)));
  renderApp("/builder/class", { api, openCharacterId: id });
  await screen.findByRole("menuitem", { name: /Brannoc/i });
  return api;
}

describe("C1 content notices", () => {
  it("several notices are one collapsed notice that expands", async () => {
    const user = userEvent.setup();
    await openWith((s) => ({ ...s, notices: NOTICES }));
    const region = await screen.findByRole("region", { name: "Content notices" });
    const toggle = within(region).getByRole("button", { name: "3 content notices" });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(within(region).queryByText(NOTICES[0]!)).toBeNull();
    expect(document.querySelectorAll(".bp-left .alert").length).toBe(1);
    await user.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    for (const n of NOTICES) expect(within(region).getByText(n)).toBeTruthy();
    await user.click(toggle);
    expect(within(region).queryByText(NOTICES[0]!)).toBeNull();
  });

  it("one notice is shown as it is, and none shows nothing", async () => {
    await openWith((s) => ({ ...s, notices: [NOTICES[2]!] }));
    const region = await screen.findByRole("region", { name: "Content notices" });
    expect(region.textContent).toBe(NOTICES[2]);
    expect(within(region).queryByRole("button")).toBeNull();
  });

  it("a choice whose content cannot be read says so in place of the empty-list text", async () => {
    const problem = "This choice's filter, \"Companion 4: Ability Boost (\", can't be read, so it offers nothing.";
    const api = await openWith((s) => ({
      ...s, notices: [],
      slots: s.slots.map((sl) => (sl.title === "Fighting style" ? { ...sl, chosen: null, problem } : sl)),
    }));
    vi.spyOn(api, "getSlotOptions").mockResolvedValue({ total: 0, items: [] });
    expect(screen.queryByRole("region", { name: "Content notices" })).toBeNull();
    const user = userEvent.setup();
    const head = await screen.findByRole("button", { name: /FIGHTING STYLE/i });
    if (head.getAttribute("aria-expanded") !== "true") await user.click(head);
    const note = await screen.findByRole("note", { name: "Content problem" });
    expect(note.textContent).toBe(problem);
    expect(screen.queryByText(/Check your source restrictions/)).toBeNull();
    expect(screen.queryByRole("note", { name: "Why this list is empty" })).toBeNull();
  });
});
