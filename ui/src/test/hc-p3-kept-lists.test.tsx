import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { renderApp } from "./utils";

// Homebrew P3 INW5c (1.5.0, HC11, spec 10.1): unsaved SPELL LISTS checks kept when the workspace is left are kept on
// the device like a draft, so they are still there after a restart. A restart is a fresh UI over the same backend: a
// new object (the UI's memory is per backend object) whose state is the mock's.

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };

describe("P3 INW5c: kept SPELL LISTS checks survive a restart", () => {
  it("opens the pack on SPELL LISTS with the checks after a restart, and Save forgets them on the device", async () => {
    // Arrange: two checks on the Wizard list, not saved, kept on the device
    const user = userEvent.setup();
    const api = new MockApi({ creatorEnabled: true });
    const pack = await api.homebrewCreatePack({ request: PACK });
    const keep = vi.spyOn(api, "homebrewKeepSpellListChecks");
    renderApp(`/homebrew/${pack.id}`, { api });
    await user.click(await screen.findByRole("button", { name: "Spell Lists" }));
    let view = await screen.findByRole("region", { name: "Spell lists" });
    await user.selectOptions(within(view).getByRole("combobox", { name: "Spell list" }), "Wizard");
    const grid = await within(view).findByRole("table", { name: "Spells" });
    for (const name of ["Bless", "Guidance"]) await user.click(within(grid).getByRole("checkbox", { name: `Choose ${name}` }));
    await waitFor(() => expect(keep).toHaveBeenLastCalledWith({
      packId: pack.id, kept: { list: "Wizard", pending: [{ id: "bless", on: true }, { id: "guidance", on: true }] },
    }));

    // Act: the app restarts and the pack is opened again
    cleanup();
    const restarted = Object.create(api) as MockApi;
    renderApp(`/homebrew/${pack.id}`, { api: restarted });

    // Assert: SPELL LISTS is open on the Wizard list with the two checks waiting for Save
    view = await screen.findByRole("region", { name: "Spell lists" });
    await waitFor(() => expect((within(view).getByRole("combobox", { name: "Spell list" }) as HTMLSelectElement).value).toBe("Wizard"));
    await waitFor(() => expect(within(view).getByRole("status").textContent).toContain("(2 not saved)"));
    await user.click(within(view).getByRole("button", { name: "Save" }));
    await waitFor(async () => expect(await restarted.homebrewKeptSpellListChecks({ packId: pack.id })).toBeNull());
  });
});
