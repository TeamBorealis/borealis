import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { SlotRows } from "../pages/play/SpellSlotsCard";
import { renderApp, seededId } from "./utils";

// GitHub #2 (decision U13): the Play tab spell tracker. Spell slot pips per level (Seraphine, a 2024 Wizard 2: three 1st level
// slots), concentration pinned at the top of Play, the user's own trackers, and the page's rest bar. Presses are relative and
// run in order (review M1). Every check runs on the desktop layout and on the phone layout.

const fast = () => new MockApi({ aiChunkDelayMs: 1, aiEnabled: true, rng: () => 0.5 });
const LAYOUTS = [["desktop", false], ["phone", true]] as const;

afterEach(() => { setPhoneForTests(false); setPhoneOsForTests(null); });

async function openPlay(name = "Seraphine", phone = false) {
  if (phone) { setPhoneForTests(true); setPhoneOsForTests("ios"); }
  const api = fast();
  const characterId = await seededId(api, name);
  renderApp("/play", { api, openCharacterId: characterId });
  return { api, characterId, user: userEvent.setup() };
}

const slotStatus = () => screen.getByRole("status", { name: /^Level 1 slots: \d+ of 3 left$/ });
const usePip = (list: HTMLElement) => within(list).getAllByRole("button", { name: /^Use a level 1 slot, / })[0]!;
const restorePip = (list: HTMLElement) => within(list).getAllByRole("button", { name: /^Restore a level 1 slot, / })[0]!;

describe.each(LAYOUTS)("spell slots card (%s)", (_layout, phone) => {
  it("a pip spends or gives back one slot, the keyboard works, and a long rest brings every slot back", async () => {
    const { user } = await openPlay("Seraphine", phone);
    const list = await screen.findByRole("list", { name: "Spell slots" });
    expect(slotStatus().textContent).toBe("3/3");
    const pips = within(list).getAllByRole("button", { name: "Use a level 1 slot, 3 of 3 left" });
    expect(pips).toHaveLength(3);
    // one tab stop per row; the arrow keys move along it
    expect(pips.map((p) => p.tabIndex)).toEqual([0, -1, -1]);
    await user.click(pips[0]!);
    await waitFor(() => expect(slotStatus().textContent).toBe("2/3"));
    expect(within(list).getAllByRole("button", { name: "Use a level 1 slot, 2 of 3 left" })).toHaveLength(2);
    const used = restorePip(list);
    expect(used.getAttribute("aria-label")).toBe("Restore a level 1 slot, 2 of 3 left");
    // keyboard: arrow to the used pip, Enter gives the slot back
    usePip(list).focus();
    await user.keyboard("{End}");
    expect(document.activeElement).toBe(used);
    await user.keyboard("{Enter}");
    await waitFor(() => expect(slotStatus().textContent).toBe("3/3"));
    await user.click(usePip(list));
    await waitFor(() => expect(slotStatus().textContent).toBe("2/3"));
    await user.click(screen.getByRole("button", { name: "Short Rest" }));
    await waitFor(() => expect(slotStatus().textContent).toBe("2/3"));
    await user.click(screen.getByRole("button", { name: "Long Rest" }));
    await waitFor(() => expect(slotStatus().textContent).toBe("3/3"));
  });

  it("two quick presses spend two slots, and a press right after a long rest is not lost and does not undo it", async () => {
    await openPlay("Seraphine", phone);
    const list = await screen.findByRole("list", { name: "Spell slots" });
    // both presses read the same screen and go out before either answer comes back
    const pip = usePip(list);
    fireEvent.click(pip);
    fireEvent.click(pip);
    await waitFor(() => expect(slotStatus().textContent).toBe("1/3"));
    // rest, then at once press a stale "restore" pip and a stale "use" pip: the rest comes first
    const stale = restorePip(list);
    const staleUse = usePip(list);
    fireEvent.click(screen.getByRole("button", { name: "Long Rest" }));
    fireEvent.click(stale);
    fireEvent.click(staleUse);
    await waitFor(() => expect(slotStatus().textContent).toBe("2/3"));
    await new Promise((r) => setTimeout(r, 50));
    expect(slotStatus().textContent).toBe("2/3");
  });

  it("a character without spellcasting shows no spell slots card", async () => {
    await openPlay("Brannoc", phone);
    await screen.findByRole("list", { name: "Resources" });
    expect(screen.queryByRole("list", { name: "Spell slots" })).toBeNull();
    expect(screen.queryByRole("combobox", { name: "Concentrate on" })).toBeNull();
  });

  it("the rests sit in a bar at the top of Play, not in the Resources header", async () => {
    await openPlay("Seraphine", phone);
    const rests = await screen.findByRole("group", { name: "Rests" });
    expect(within(rests).getAllByRole("button").map((b) => b.textContent)).toEqual(["Short Rest", "Long Rest"]);
    const firstTile = screen.getByRole("button", { name: /^Explain AC, \d+$/ });
    expect(rests.compareDocumentPosition(firstTile) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const resources = screen.getByRole("list", { name: "Resources" }).closest("section")!;
    expect(within(resources).queryByRole("button", { name: /Rest$/ })).toBeNull();
  });

  it("concentration is picked, pinned at the top of Play, ended with one tap and cleared by a long rest", async () => {
    const { user } = await openPlay("Seraphine", phone);
    const pick = await screen.findByRole("combobox", { name: "Concentrate on" });
    expect(within(pick).getAllByRole("option").map((o) => o.textContent)).toEqual(["Not concentrating", "Detect Magic (level 1)"]);
    expect(screen.queryByRole("region", { name: "Concentration" })).toBeNull();
    await user.selectOptions(pick, "Detect Magic (level 1)");
    const pinned = await screen.findByRole("region", { name: "Concentration" });
    expect(pinned.textContent).toContain("Detect Magic");
    // pinned before the stat tiles of the Play list, after the rest bar
    const firstTile = screen.getByRole("button", { name: /^Explain AC, \d+$/ });
    expect(pinned.compareDocumentPosition(firstTile) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByRole("group", { name: "Rests" }).compareDocumentPosition(pinned) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await user.click(within(pinned).getByRole("button", { name: "End concentration on Detect Magic" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Concentration" })).toBeNull());
    await user.selectOptions(screen.getByRole("combobox", { name: "Concentrate on" }), "Detect Magic (level 1)");
    await screen.findByRole("region", { name: "Concentration" });
    await user.click(screen.getByRole("button", { name: "Long Rest" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Concentration" })).toBeNull());
    expect((screen.getByRole("combobox", { name: "Concentrate on" }) as HTMLSelectElement).value).toBe("");
  });

  it("a custom tracker is added, tracked, reset by its rule, edited to a large maximum and deleted in two presses", async () => {
    const { user } = await openPlay("Brannoc", phone);
    const card = await screen.findByRole("list", { name: "Trackers" });
    expect(within(card).getByText("No trackers")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Add tracker" }));
    let dialog = await screen.findByRole("dialog", { name: "Add tracker" });
    // inline validation: a name is needed (hidden characters do not count)
    await user.type(within(dialog).getByLabelText("Name"), "​");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(within(dialog).getByText("Give the tracker a name.")).toBeTruthy();
    await user.clear(within(dialog).getByLabelText("Name"));
    await user.type(within(dialog).getByLabelText("Name"), "Splicer Points");
    const max = within(dialog).getByLabelText("Maximum");
    await user.clear(max);
    await user.type(max, "5");
    await user.selectOptions(within(dialog).getByLabelText("Resets on"), "Long Rest");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    const status = () => screen.getByRole("status", { name: /^Splicer Points: \d+ of \d+ left$/ });
    expect(status().textContent).toBe("5/5");
    await user.click(screen.getAllByRole("button", { name: "Use one Splicer Points, 5 of 5 left" })[0]!);
    await waitFor(() => expect(status().textContent).toBe("4/5"));
    await user.click(screen.getByRole("button", { name: "Short Rest" }));
    await waitFor(() => expect(status().textContent).toBe("4/5"));
    await user.click(screen.getByRole("button", { name: "Long Rest" }));
    await waitFor(() => expect(status().textContent).toBe("5/5"));

    // a large maximum tracks with - and + instead of pips
    await user.click(screen.getByRole("button", { name: "Edit Splicer Points" }));
    dialog = await screen.findByRole("dialog", { name: "Edit tracker" });
    expect((within(dialog).getByLabelText("Name") as HTMLInputElement).value).toBe("Splicer Points");
    const editMax = within(dialog).getByLabelText("Maximum");
    await user.clear(editMax);
    await user.type(editMax, "40");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(status().textContent).toBe("40/40"));
    expect(screen.queryByRole("button", { name: /^Use one Splicer Points/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Spend Splicer Points" }));
    await waitFor(() => expect(status().textContent).toBe("39/40"));
    await user.click(screen.getByRole("button", { name: "Restore Splicer Points" }));
    await waitFor(() => expect(status().textContent).toBe("40/40"));

    // Delete needs a second press
    await user.click(screen.getByRole("button", { name: "Edit Splicer Points" }));
    dialog = await screen.findByRole("dialog", { name: "Edit tracker" });
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect(screen.getByRole("status", { name: /^Splicer Points:/ })).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Confirm delete" }));
    await waitFor(() => expect(screen.queryByRole("status", { name: /^Splicer Points:/ })).toBeNull());
    expect(within(screen.getByRole("list", { name: "Trackers" })).getByText("No trackers")).toBeTruthy();
  });

  it("a tracker that counts up shows what was gained, filled from the left", async () => {
    const { user } = await openPlay("Brannoc", phone);
    await user.click(await screen.findByRole("button", { name: "Add tracker" }));
    const dialog = await screen.findByRole("dialog", { name: "Add tracker" });
    await user.type(within(dialog).getByLabelText("Name"), "Infection");
    const max = within(dialog).getByLabelText("Maximum");
    await user.clear(max);
    await user.type(max, "6");
    await user.selectOptions(within(dialog).getByLabelText("Resets on"), "Never");
    await user.click(within(dialog).getByLabelText(/Counts up/));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    const status = () => screen.getByRole("status", { name: /^Infection: \d+ of 6$/ });
    await waitFor(() => expect(status().textContent).toBe("0/6"));
    expect(screen.getByText("No reset · counts up")).toBeTruthy();
    await user.click(screen.getAllByRole("button", { name: "Add one to Infection, 0 of 6" })[0]!);
    await waitFor(() => expect(status().textContent).toBe("1/6"));
    const group = screen.getByRole("group", { name: "Infection" });
    const pips = within(group).getAllByRole("button");
    expect(pips[0]!.getAttribute("aria-label")).toBe("Remove one from Infection, 1 of 6");
    expect(pips[0]!.querySelector(".v5-pip.full")).not.toBeNull();
    expect(pips[1]!.querySelector(".v5-pip.full")).toBeNull();
  });
});

describe("pact magic row", () => {
  it("shows the pact slots as their own row, recovered on a short rest", async () => {
    const onPress = vi.fn();
    const user = userEvent.setup();
    render(<ul aria-label="Spell slots"><SlotRows slots={[]} pact={{ level: 2, max: 2, used: 1 }} onPress={onPress} /></ul>);
    const row = screen.getByRole("listitem");
    expect(within(row).getByText("Pact Magic")).toBeTruthy();
    expect(within(row).getByText("Level 2 · Short Rest")).toBeTruthy();
    expect(screen.getByRole("status", { name: "Pact Magic slots: 1 of 2 left" }).textContent).toBe("1/2");
    await user.click(screen.getByRole("button", { name: "Use a Pact Magic slot, 1 of 2 left" }));
    expect(onPress).toHaveBeenCalledWith({ pact: true, level: 2, spend: true });
    await user.click(screen.getByRole("button", { name: "Restore a Pact Magic slot, 1 of 2 left" }));
    expect(onPress).toHaveBeenLastCalledWith({ pact: true, level: 2, spend: false });
  });
});
