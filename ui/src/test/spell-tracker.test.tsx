import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { SlotRows } from "../pages/play/SpellSlotsCard";
import { renderApp, seededId } from "./utils";

// GitHub #2 (decision U13): the Play tab spell tracker. Spell slot pips per level (Seraphine, a 2024 Wizard 2: three 1st level
// slots), concentration pinned at the top of Play, and the user's own trackers; the existing Short/Long Rest buttons bring
// them back. Every check runs on the desktop layout and on the phone layout.

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

describe.each(LAYOUTS)("spell slots card (%s)", (_layout, phone) => {
  it("a pip marks a slot used and unused, and a long rest brings every slot back", async () => {
    const { user } = await openPlay("Seraphine", phone);
    const list = await screen.findByRole("list", { name: "Spell slots" });
    expect(slotStatus().textContent).toBe("3/3");
    const pips = within(list).getAllByRole("button", { name: /^Level 1 slot \d of 3, (available|used)$/ });
    expect(pips).toHaveLength(3);
    await user.click(within(list).getByRole("button", { name: "Level 1 slot 1 of 3, available" }));
    await waitFor(() => expect(slotStatus().textContent).toBe("2/3"));
    const used = within(list).getByRole("button", { name: "Level 1 slot 3 of 3, used" });
    expect(used.getAttribute("aria-pressed")).toBe("true");
    // keyboard: Enter on a used pip gives the slot back
    used.focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(slotStatus().textContent).toBe("3/3"));
    await user.click(within(list).getByRole("button", { name: "Level 1 slot 1 of 3, available" }));
    await user.click(await within(list).findByRole("button", { name: "Level 1 slot 1 of 3, available" }));
    await waitFor(() => expect(slotStatus().textContent).toBe("1/3"));
    await user.click(screen.getByRole("button", { name: "Short Rest" }));
    await waitFor(() => expect(slotStatus().textContent).toBe("1/3"));
    await user.click(screen.getByRole("button", { name: "Long Rest" }));
    await waitFor(() => expect(slotStatus().textContent).toBe("3/3"));
  });

  it("a character without spellcasting shows no spell slots card", async () => {
    await openPlay("Brannoc", phone);
    await screen.findByRole("list", { name: "Resources" });
    expect(screen.queryByRole("list", { name: "Spell slots" })).toBeNull();
    expect(screen.queryByRole("combobox", { name: "Concentrate on" })).toBeNull();
  });

  it("concentration is picked, pinned at the top of Play, ended with one tap and cleared by a long rest", async () => {
    const { user } = await openPlay("Seraphine", phone);
    const pick = await screen.findByRole("combobox", { name: "Concentrate on" });
    expect(within(pick).getAllByRole("option").map((o) => o.textContent)).toEqual(["Not concentrating", "Detect Magic (level 1)"]);
    expect(screen.queryByRole("region", { name: "Concentration" })).toBeNull();
    await user.selectOptions(pick, "Detect Magic (level 1)");
    const pinned = await screen.findByRole("region", { name: "Concentration" });
    expect(pinned.textContent).toContain("Detect Magic");
    // pinned first: before the stat tiles of the Play list
    const firstTile = screen.getByRole("button", { name: /^Explain AC, \d+$/ });
    expect(pinned.compareDocumentPosition(firstTile) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await user.click(within(pinned).getByRole("button", { name: "End concentration on Detect Magic" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Concentration" })).toBeNull());
    await user.selectOptions(screen.getByRole("combobox", { name: "Concentrate on" }), "Detect Magic (level 1)");
    await screen.findByRole("region", { name: "Concentration" });
    await user.click(screen.getByRole("button", { name: "Long Rest" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Concentration" })).toBeNull());
    expect((screen.getByRole("combobox", { name: "Concentrate on" }) as HTMLSelectElement).value).toBe("");
  });

  it("a custom tracker is added, tracked, reset by its rule, edited to a large maximum and deleted", async () => {
    const { user } = await openPlay("Brannoc", phone);
    const card = await screen.findByRole("list", { name: "Trackers" });
    expect(within(card).getByText("No trackers")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Add tracker" }));
    let dialog = await screen.findByRole("dialog", { name: "Add tracker" });
    // inline validation: a name is needed
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(within(dialog).getByText("Give the tracker a name.")).toBeTruthy();
    await user.type(within(dialog).getByLabelText("Name"), "Splicer Points");
    const max = within(dialog).getByLabelText("Maximum");
    await user.clear(max);
    await user.type(max, "5");
    await user.selectOptions(within(dialog).getByLabelText("Resets on"), "Long Rest");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    const status = () => screen.getByRole("status", { name: /^Splicer Points: \d+ of \d+ left$/ });
    expect(status().textContent).toBe("5/5");
    await user.click(screen.getByRole("button", { name: "Splicer Points 1 of 5, available" }));
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
    expect(screen.queryByRole("button", { name: /^Splicer Points 1 of 40/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Spend Splicer Points" }));
    await waitFor(() => expect(status().textContent).toBe("39/40"));
    await user.click(screen.getByRole("button", { name: "Restore Splicer Points" }));
    await waitFor(() => expect(status().textContent).toBe("40/40"));

    await user.click(screen.getByRole("button", { name: "Edit Splicer Points" }));
    dialog = await screen.findByRole("dialog", { name: "Edit tracker" });
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByRole("status", { name: /^Splicer Points:/ })).toBeNull());
    expect(within(screen.getByRole("list", { name: "Trackers" })).getByText("No trackers")).toBeTruthy();
  });
});

describe("pact magic row", () => {
  it("shows the pact slots as their own row, recovered on a short rest", async () => {
    const onSet = vi.fn();
    const user = userEvent.setup();
    render(<ul aria-label="Spell slots"><SlotRows slots={[]} pact={{ level: 2, max: 2, used: 1 }} onSet={onSet} /></ul>);
    const row = screen.getByRole("listitem");
    expect(within(row).getByText("Pact Magic")).toBeTruthy();
    expect(within(row).getByText("Level 2 · Short Rest")).toBeTruthy();
    expect(screen.getByRole("status", { name: "Pact Magic slots: 1 of 2 left" }).textContent).toBe("1/2");
    await user.click(screen.getByRole("button", { name: "Pact Magic slot 1 of 2, available" }));
    expect(onSet).toHaveBeenCalledWith({ pact: true, level: 2, used: 2 });
    await user.click(screen.getByRole("button", { name: "Pact Magic slot 2 of 2, used" }));
    expect(onSet).toHaveBeenLastCalledWith({ pact: true, level: 2, used: 0 });
  });
});
