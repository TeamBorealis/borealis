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

  // B-140 (TV1): entering Rage is the use being counted; ending it gives nothing back; at 0 the switch refuses and stays off.
  it("the Rage switch spends a use, ending it restores nothing, and at 0 it refuses", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/play", { api, openCharacterId: await seededId(api, "Seraphine") });
    const count = await screen.findByRole("status", { name: /Rage: \d+ of \d+ uses left/ });
    const max = Number(/of (\d+)/.exec(count.getAttribute("aria-label")!)![1]);
    const rage = () => screen.getByRole("status", { name: /Rage:/ }).textContent;
    const toggle = () => screen.getByRole("switch", { name: "Rage" }) as HTMLInputElement;
    await user.click(toggle());
    await waitFor(() => expect(toggle().checked).toBe(true));
    expect(rage()).toBe(`${max - 1}/${max}`);
    await user.click(toggle());
    await waitFor(() => expect(toggle().checked).toBe(false));
    expect(rage()).toBe(`${max - 1}/${max}`);
    for (let left = max - 1; left > 0; left--) {
      await user.click(screen.getByRole("button", { name: "Spend Rage" }));
      await waitFor(() => expect(rage()).toBe(`${left - 1}/${max}`));
    }
    await user.click(toggle());
    expect(await screen.findByText(`Rage has 0 of ${max} uses left`)).toBeTruthy();
    expect(toggle().checked).toBe(false);
    expect(rage()).toBe(`0/${max}`);
  });

  // B-141 (tester video C7): the detail said "Resets on Short Rest / Long Rest", but a Short Rest gives back one Rage use.
  it("the resource row and its detail say how many uses each rest gives back", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/play", { api, openCharacterId: await seededId(api, "Seraphine") });
    const list = await screen.findByRole("list", { name: "Resources" });
    const row = within(list).getByRole("button", { name: /^Rage/ });
    expect(row.textContent).toContain("1 back on a Short Rest, all on a Long Rest");
    await user.click(row);
    expect(await screen.findByText(/\d+ of \d+ left\. 1 back on a Short Rest, all on a Long Rest\./)).toBeTruthy();
    expect(screen.queryByText(/Resets on/)).toBeNull();
  });

  // B-142 (tester video C9): a rest gave no visible answer, so six taps on Short Rest looked like nothing happened.
  it("Short Rest reports what came back, and says so when there is nothing to regain", async () => {
    const user = userEvent.setup();
    const api = fast();
    const id = await seededId(api, "Seraphine");
    await api.spendResource({ characterId: id, resourceId: "rage" });
    await api.spendResource({ characterId: id, resourceId: "rage" });
    renderApp("/play", { api, openCharacterId: id });
    const count = await screen.findByRole("status", { name: /Rage: \d+ of \d+ uses left/ });
    const max = Number(/of (\d+)/.exec(count.getAttribute("aria-label")!)![1]);
    expect(count.textContent).toBe(`${max - 2}/${max}`);
    await user.click(screen.getByRole("button", { name: "Short Rest" }));
    expect(await screen.findByText("Short Rest: Rage +1.")).toBeTruthy();
    expect(screen.getByRole("status", { name: /Rage:/ }).textContent).toBe(`${max - 1}/${max}`);
    await user.click(screen.getByRole("button", { name: "Long Rest" }));
    expect(await screen.findByText(/^Long Rest: Rage \+1/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Short Rest" }));
    expect(await screen.findByText("Short Rest: nothing to regain.")).toBeTruthy();
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
