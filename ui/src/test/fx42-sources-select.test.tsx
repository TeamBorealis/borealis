import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { renderApp } from "./utils";

const dataRows = async () => {
  const grid = await screen.findByRole("grid", { name: "Sources" });
  const rows = await waitFor(() => {
    const r = within(grid).getAllByRole("row").filter((x) => x.getAttribute("aria-selected") !== null);
    if (r.length < 2) throw new Error("rows not loaded ");
    return r;
  });
  return { grid, rows };
};
const selectedCount = (rows: HTMLElement[]) => rows.filter((r) => r.getAttribute("aria-selected") === "true").length;

describe("Sources extended selection", () => {
  it("shows no heuristic badges", async () => {
    renderApp("/content/sources");
    await dataRows();
    expect(screen.queryByRole("img", { name: "Incomplete source" })).toBeNull();
    expect(screen.queryByRole("img", { name: "Additional information" })).toBeNull();
  });

  it("Ctrl-click adds and removes, Shift-click selects a range, plain click resets", async () => {
    const user = userEvent.setup();
    renderApp("/content/sources");
    const { rows } = await dataRows();
    await user.click(rows[0]!);
    expect(selectedCount(rows)).toBe(1);
    await user.keyboard("{Control>}");
    await user.click(rows[1]!);
    expect(selectedCount(rows)).toBe(2);
    await user.click(rows[1]!);
    await user.keyboard("{/Control}");
    expect(selectedCount(rows)).toBe(1);
    await user.click(rows[0]!);
    await user.keyboard("{Shift>}");
    await user.click(rows[rows.length - 1]!);
    await user.keyboard("{/Shift}");
    expect(selectedCount(rows)).toBe(rows.length);
    await user.click(rows[0]!);
    expect(selectedCount(rows)).toBe(1);
  });

  it("Shift+Arrow extends and Ctrl+A selects all", async () => {
    const user = userEvent.setup();
    renderApp("/content/sources");
    const { grid, rows } = await dataRows();
    await user.click(rows[0]!);
    grid.focus();
    await user.keyboard("{Shift>}{ArrowDown}{/Shift}");
    expect(selectedCount(rows)).toBe(2);
    await user.keyboard("{ArrowUp}");
    expect(selectedCount(rows)).toBe(1);
    await user.keyboard("{Control>}a{/Control}");
    expect(selectedCount(rows)).toBe(rows.length);
  });

  it("Toggle all selected sources flips every selected row", async () => {
    const user = userEvent.setup();
    renderApp("/content/sources");
    const { rows } = await dataRows();
    const box = (r: HTMLElement) => within(r).getByRole("checkbox") as HTMLInputElement;
    const before = rows.map((r) => box(r).checked);
    await user.click(rows[0]!);
    await user.keyboard("{Shift>}");
    await user.click(rows[rows.length - 1]!);
    await user.keyboard("{/Shift}");
    await user.pointer({ keys: "[MouseRight]", target: rows[0]! });
    await user.click(await screen.findByRole("menuitem", { name: "Toggle all selected sources" }));
    expect(rows.map((r) => box(r).checked)).toEqual(before.map((b) => !b));
  });
});
