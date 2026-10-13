import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

const fast = () => new MockApi({ aiChunkDelayMs: 1, aiEnabled: true, rng: () => 0.5 });

describe("Play panel", () => {
  it("spends a resource and a long rest restores it", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/play", { api, openCharacterId: await seededId(api, "Seraphine") });
    const count = await screen.findByRole("status", { name: /Rage: \d+ of \d+ uses left/ });
    const max = Number(/of (\d+)/.exec(count.getAttribute("aria-label")!)![1]);
    expect(count.textContent).toBe(`${max}/${max}`);
    await user.click(screen.getByRole("button", { name: "Spend Rage" }));
    await waitFor(() => expect(screen.getByRole("status", { name: /Rage:/ }).textContent).toBe(`${max - 1}/${max}`));
    await user.click(screen.getByRole("button", { name: "Long Rest" }));
    await waitFor(() => expect(screen.getByRole("status", { name: /Rage:/ }).textContent).toBe(`${max}/${max}`));
  });

  it("toggling Rage shows the resistances", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/play", { api, openCharacterId: await seededId(api, "Seraphine") });
    await user.click(await screen.findByRole("switch", { name: "Rage" }));
    await waitFor(() => expect((screen.getByRole("switch", { name: "Rage" }) as HTMLInputElement).checked).toBe(true));
    await user.click(screen.getByRole("tab", { name: "Traits" }));
    const res = await screen.findByRole("list", { name: "Resistances" });
    expect(within(res).getByText("bludgeoning")).toBeTruthy();
    expect(within(res).queryByText(/while rage/)).toBeNull();
  });

  it("steps exhaustion and summarises the effects", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/play", { api, openCharacterId: await seededId(api, "Seraphine") });
    const raise = await screen.findByRole("button", { name: "Raise exhaustion" });
    expect((screen.getByRole("button", { name: "Lower exhaustion" }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(raise);
    await user.click(raise);
    await waitFor(() => expect(screen.getByTestId("exhaustion-summary").textContent).toContain("Level 2: -4 to D20 Tests, -10 ft. speed."));
    await user.click(screen.getByRole("button", { name: "Lower exhaustion" }));
    await waitFor(() => expect(screen.getByTestId("exhaustion-summary").textContent).toContain("Level 1"));
  });

  it("explains a number with its trace and streams the AI explanation", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/play", { api, openCharacterId: await seededId(api, "Seraphine") });
    await user.click(await screen.findByRole("button", { name: /^Explain AC, \d+$/ }));
    const dialog = await screen.findByRole("dialog");
    expect((await within(dialog).findByRole("list", { name: "Contributions" })).textContent).toContain("Mock total");
    await user.click(within(dialog).getByRole("button", { name: "Explain with AI" }));
    expect(await within(dialog).findByText(/Armor Class is 10 plus/)).toBeTruthy();
  });
});

describe("Rules and Gear", () => {
  it("previews Convert to 2024 and Apply opens the new copy", async () => {
    const user = userEvent.setup();
    const api = fast();
    /* Convert to 2024 is a Borealis-only function: it lives in the "Edition & compatibility" flyout of the character-name menu. */
    renderApp("/builder", { api, openCharacterId: await seededId(api, "Brannoc") });
    await user.click(await screen.findByRole("menuitem", { name: /Brannoc/i }));
    await user.click(await screen.findByRole("menuitem", { name: "Edition & compatibility…" }));
    expect(await screen.findByRole("dialog", { name: "Edition & compatibility" })).toBeTruthy();
    expect(await screen.findByText("House rules")).toBeTruthy();
    expect(await screen.findByText("2014 rules")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Convert to 2024…" }));
    const dialog = await screen.findByRole("dialog", { name: /Convert/ });
    expect(await within(dialog).findByRole("region", { name: "Kept" })).toBeTruthy();
    expect(within(dialog).getByRole("table", { name: "Before and after" })).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    expect((await api.listCharacters()).some((c) => c.name === "Brannoc Ironvale (2024)")).toBe(true);
  });

  it("the mastery chooser respects its capacity", async () => {
    const user = userEvent.setup();
    const api = fast();
    const { newCharacterId } = await api.applyConvertTo2024({ characterId: await seededId(api, "Brannoc") });
    renderApp("/equipment", { api, openCharacterId: newCharacterId });
    await user.click(await screen.findByRole("tab", { name: "Additional treasure" }));
    await screen.findByText("0 of 3 chosen");
    for (const w of ["dagger", "handaxe", "mace"]) await user.click(await screen.findByRole("button", { name: new RegExp(`^Master ${w}$`, "i") }));
    await screen.findByText("3 of 3 chosen");
    expect((screen.getByRole("button", { name: /^Master spear$/i }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(screen.getByRole("button", { name: /^Unmaster dagger$/i }));
    await screen.findByText("2 of 3 chosen");
    expect((screen.getByRole("button", { name: /^Master spear$/i }) as HTMLButtonElement).disabled).toBe(false);
  });
});
