import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { renderApp } from "./utils";

/**
 * A2 (tester video 2026-11-02, B-113) with TV1: on phones the description card sat below every list of the tab, often two
 * pickers away from the tapped row. It now opens inline, directly under the tapped row; desktop keeps the right-hand pane.
 */
afterEach(() => setPhoneForTests(false));

async function openRace(phone: boolean) {
  setPhoneForTests(phone);
  const api = new MockApi({ aiChunkDelayMs: 1 });
  const snap = await api.createCharacter({ request: { name: "Inline", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
  renderApp("/builder/race", { api, openCharacterId: snap.id });
  const list = await screen.findByRole("list", { name: "Species options" });
  await waitFor(() => expect(within(list).getAllByRole("listitem").length).toBeGreaterThan(2));
  const rows = () => within(list).getAllByRole("listitem").filter((li) => li.classList.contains("pick-row"));
  const tap = (li: HTMLElement) => userEvent.setup().click(li.querySelector<HTMLButtonElement>(".pick-main")!);
  return { list, rows, tap };
}

const titleIn = (el: Element | null | undefined) => el?.querySelector(".desc-pane .desc-title")?.textContent ?? null;

describe("the phone description opens under the tapped row (A2)", () => {
  it("phone: the tapped row's description renders directly after it", async () => {
    const { rows, tap } = await openRace(true);
    const second = rows()[1]!;
    const name = second.getAttribute("data-row")!;
    await tap(second);
    await waitFor(() => expect(titleIn(document.querySelector("li.pick-row.hl")?.nextElementSibling)).toBe(name));
    // One card on the screen: the bottom pane does not repeat it.
    expect(document.querySelectorAll(".desc-pane .desc-title")).toHaveLength(1);
  });

  it("tapping another row moves the card; tapping the same row again closes it", async () => {
    const { rows, tap } = await openRace(true);
    const [first, second] = [rows()[0]!, rows()[1]!];
    await tap(first);
    await waitFor(() => expect(titleIn(first.nextElementSibling)).toBe(first.getAttribute("data-row")));
    await tap(second);
    await waitFor(() => expect(titleIn(second.nextElementSibling)).toBe(second.getAttribute("data-row")));
    expect(first.nextElementSibling).toBe(second);
    await tap(second);
    await waitFor(() => expect(second.nextElementSibling?.classList.contains("pick-inline")).not.toBe(true));
  });

  it("desktop keeps the right-hand pane", async () => {
    const { rows, tap, list } = await openRace(false);
    const second = rows()[1]!;
    await tap(second);
    await waitFor(() => expect(titleIn(document.querySelector(".bp-right"))).toBe(second.getAttribute("data-row")));
    expect(document.querySelectorAll(".desc-pane")).toHaveLength(1);
    // A second click on the row keeps it described (desktop never toggles).
    await tap(second);
    expect(titleIn(document.querySelector(".bp-right"))).toBe(second.getAttribute("data-row"));
    expect(list.querySelector(".desc-pane")).toBeNull();
  });
});
