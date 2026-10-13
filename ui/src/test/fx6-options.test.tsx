import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });

describe("Character options flyout (F8)", () => {
  it("is 400 px, shows option cards, and Cancel discards the draft", async () => {
    const user = userEvent.setup();
    const api = fast();
    const id = await seededId(api, "Brannoc");
    renderApp("/builder", { api, openCharacterId: id });
    await screen.findByRole("menuitem", { name: /Brannoc/i });
    await user.keyboard("{F8}");
    const dialog = await screen.findByRole("dialog", { name: "Character options" });
    expect(dialog.style.getPropertyValue("--flyout-w")).toBe("400px");
    expect(dialog.querySelectorAll(".v4-opt").length).toBeGreaterThan(0);
    expect(within(dialog).queryByText("House rules")).toBeNull();
    expect(within(dialog).queryByText(/Convert to 2024/)).toBeNull();
    const before = (await api.getSnapshot({ characterId: id })).options.averageHp;
    await user.click(within(dialog).getByRole("checkbox", { name: /average hit points/i }));
    expect((await api.getSnapshot({ characterId: id })).options.averageHp).toBe(before);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog", { name: "Character options" })).toBeNull();
    expect((await api.getSnapshot({ characterId: id })).options.averageHp).toBe(before);
  });

  it("OK applies the deferred change", async () => {
    const user = userEvent.setup();
    const api = fast();
    const id = await seededId(api, "Brannoc");
    renderApp("/builder", { api, openCharacterId: id });
    await screen.findByRole("menuitem", { name: /Brannoc/i });
    await user.keyboard("{F8}");
    const dialog = await screen.findByRole("dialog", { name: "Character options" });
    const before = (await api.getSnapshot({ characterId: id })).options.averageHp;
    await user.click(within(dialog).getByRole("checkbox", { name: /average hit points/i }));
    await user.click(within(dialog).getByRole("button", { name: "OK" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).options.averageHp).toBe(!before));
  });
});

describe("Edition & compatibility flyout", () => {
  it("opens from the name menu after a separator and holds edition, report and house rules", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/builder", { api, openCharacterId: await seededId(api, "Brannoc") });
    await user.click(await screen.findByRole("menuitem", { name: /Brannoc/i }));
    await user.click(await screen.findByRole("menuitem", { name: "Edition & compatibility…" }));
    const dialog = await screen.findByRole("dialog", { name: "Edition & compatibility" });
    expect(dialog.style.getPropertyValue("--flyout-w")).toBe("400px");
    expect(await within(dialog).findByText("Compatibility report")).toBeTruthy();
    expect(within(dialog).getByText("House rules")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog", { name: "Edition & compatibility" })).toBeNull();
  });
});
