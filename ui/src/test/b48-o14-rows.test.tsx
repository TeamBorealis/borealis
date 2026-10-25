import { act, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { renderApp, seededId } from "./utils";

// B-48 O14 B (U9a): on phones the data tables (spells, selection slots, the equipment catalogue, Sources) become one-line
// rows; the other columns show as a detail line on the row, with the same information. Desktop keeps its columns (U7).

afterEach(() => { setPhoneForTests(false); });

const detailOf = (row: HTMLElement) => row.querySelector(".ph-row-detail")?.textContent ?? null;
/** Text of a table cell as the desktop shows it; a tick mark reads as its label's first letter (V, S, M). */
const cellText = (td: Element) => {
  const mark = td.querySelector(".sp-mark");
  if (mark) return (mark.getAttribute("aria-label") ?? "").charAt(0);
  return (td.textContent ?? "").trim();
};

async function open(route: string, phone: boolean) {
  if (phone) setPhoneForTests(true);
  const api = new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5 });
  const id = await seededId(api, "Seraphine");
  renderApp(route, { api, openCharacterId: id });
  return api;
}

describe("spell rows (O14)", () => {
  it("on a phone every spell row carries level and school, components and source as its detail", async () => {
    await open("/spells", true);
    const table = await screen.findByRole("table", { name: "Wizard spells" });
    const rows = await waitFor(() => { const r = [...table.querySelectorAll<HTMLElement>("tbody tr[data-spell-id]")]; expect(r.length).toBeGreaterThan(3); return r; });
    for (const row of rows) {
      const detail = detailOf(row);
      expect(detail, row.textContent ?? "").not.toBeNull();
      const level = cellText(row.querySelector("td.sp-lvl")!);
      const marks = [...row.querySelectorAll("td.sp-vsm")].map(cellText).filter(Boolean);
      const source = cellText(row.querySelector("td.sp-src")!);
      expect(detail).toContain(level);
      if (marks.length) expect(detail).toContain(marks.join(", "));
      if (source) expect(detail).toContain(source);
    }
  });

  it("selection slot rows keep the gained-on level, replaced spell and spellcasting source", async () => {
    await open("/spells", true);
    const table = await screen.findByRole("table", { name: "Selection slots" });
    const rows = await waitFor(() => { const r = [...table.querySelectorAll<HTMLElement>("tbody tr")].filter((r) => !r.classList.contains("ap-empty-row")); expect(r.length).toBeGreaterThan(0); return r; });
    for (const row of rows) {
      const cells = [...row.querySelectorAll("td")];
      const detail = detailOf(row) ?? "";
      expect(detail).toContain(`Gained on level ${cellText(cells[3]!)}`);
      if (cellText(cells[4]!)) expect(detail).toContain(cellText(cells[4]!));
      if (cellText(cells[5]!)) expect(detail).toContain(cellText(cells[5]!));
    }
  });

  it("the desktop keeps its columns and shows no detail line", async () => {
    await open("/spells", false);
    const table = await screen.findByRole("table", { name: "Wizard spells" });
    await waitFor(() => expect(table.querySelectorAll("tbody tr[data-spell-id]").length).toBeGreaterThan(3));
    expect(document.querySelector(".ph-row-detail")).toBeNull();
    expect(within(table).getByRole("columnheader", { name: "Level & school" })).toBeTruthy();
  });
});

describe("Magic section tabs", () => {
  it("name the class in sentence case on a phone and in Aurora's capitals on desktop", async () => {
    await open("/spells", true);
    expect(await screen.findByRole("tab", { name: "Wizard" })).toBeTruthy();
    act(() => setPhoneForTests(false));
    expect(await screen.findByRole("tab", { name: "WIZARD" })).toBeTruthy();
  });
});

describe("equipment catalogue rows (O14)", () => {
  it("on a phone each item row carries price, weight and source", async () => {
    await open("/equipment", true);
    const table = await screen.findByRole("table", { name: "Item catalog" });
    const rows = await waitFor(() => { const r = [...table.querySelectorAll<HTMLElement>("tbody tr")].filter((r) => !r.classList.contains("ap-empty-row")); expect(r.length).toBeGreaterThan(0); return r; });
    for (const row of rows) {
      const detail = detailOf(row) ?? "";
      for (const td of [...row.querySelectorAll("td")].slice(1)) if (cellText(td)) expect(detail).toContain(cellText(td));
    }
  });

  it("desktop rows have no detail line", async () => {
    await open("/equipment", false);
    await screen.findByRole("table", { name: "Item catalog" });
    expect(document.querySelector(".ph-row-detail")).toBeNull();
  });
});

describe("Sources rows (O14)", () => {
  it("on a phone each source row carries its group, author and version", async () => {
    await open("/content/sources", true);
    const grid = await screen.findByRole("grid", { name: "Sources" });
    const rows = await waitFor(() => { const r = [...grid.querySelectorAll<HTMLElement>(".v4-grid-row:not(.v4-grid-group)")]; expect(r.length).toBeGreaterThan(0); return r; });
    for (const row of rows) {
      const cells = [...row.querySelectorAll('[role="gridcell"]')].map(cellText);
      const detail = detailOf(row) ?? "";
      expect(detail).toContain(cells[2]);
      if (cells[3]) expect(detail).toContain(cells[3]);
      if (cells[4]) expect(detail).toContain(cells[4]);
    }
  });

  it("desktop Sources rows have no detail line", async () => {
    await open("/content/sources", false);
    const grid = await screen.findByRole("grid", { name: "Sources" });
    await waitFor(() => expect(grid.querySelectorAll(".v4-grid-row").length).toBeGreaterThan(1));
    expect(document.querySelector(".ph-row-detail")).toBeNull();
  });
});
