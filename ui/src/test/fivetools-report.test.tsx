import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { FiveToolsImportReport, FiveToolsSource } from "../api/contract";
import { FiveToolsReport } from "../pages/content/FiveToolsReport";
import { FiveToolsSources } from "../pages/content/FiveToolsSources";
import { groupNotes, isExpectedSkip, reasonKey, setPicked, visibleGroups } from "../pages/content/fiveTools";
import { renderApp } from "./utils";

const src = (abbr: string, ruleset: FiveToolsSource["ruleset"], category: FiveToolsSource["category"]): FiveToolsSource =>
  ({ abbr, name: `Book ${abbr}`, ruleset, category, counts: { feat: 1 } });

const SOURCES = [src("A", "dnd2014", "book"), src("B", "dnd2024", "book"), src("C", "mixed", "adventure"), src("D", "dnd2014", "homebrew"), src("E", "dnd2024", "other")];

describe("5etools source filters", () => {
  it("groups books, adventures and homebrew; edition filter keeps mixed sources", () => {
    expect(visibleGroups(SOURCES, "all", "").map((g) => [g.id, g.sources.length])).toEqual([["book", 2], ["adventure", 1], ["other", 2]]);
    expect(visibleGroups(SOURCES, "dnd2024", "").map((g) => g.sources.map((s) => s.abbr))).toEqual([["B"], ["C"], ["E"]]);
    expect(visibleGroups(SOURCES, "all", "book d").map((g) => g.id)).toEqual(["other"]);
    expect(setPicked(["A"], ["A", "B"], true)).toEqual(["A", "B"]);
    expect(setPicked(["A", "B", "C"], ["A", "B"], false)).toEqual(["C"]);
  });

  it("select-all per group is tri-state and respects the edition filter", async () => {
    const user = userEvent.setup();
    let picked: string[] = ["A"];
    const { rerender } = render(<FiveToolsSources sources={SOURCES} picked={picked} onPicked={(p) => { picked = p; }} />);
    const all = screen.getByRole("checkbox", { name: "Select all books" }) as HTMLInputElement;
    expect(all.indeterminate).toBe(true);
    await user.selectOptions(screen.getByRole("combobox", { name: "Edition" }), "dnd2024");
    expect(screen.queryByRole("checkbox", { name: /Book A/ })).toBeNull();
    await user.click(screen.getByRole("checkbox", { name: "Select all books" }));
    expect(picked).toEqual(["A", "B"]);
    rerender(<FiveToolsSources sources={SOURCES} picked={picked} onPicked={(p) => { picked = p; }} />);
    expect(within(screen.getByRole("list", { name: "Adventures sources" })).getByText("Mixed")).toBeTruthy();
  });
});

describe("5etools import report", () => {
  const report: FiveToolsImportReport = {
    converted: 12, replaced: 2, packs: [],
    partial: [
      { element: "Kit A", kind: "item", reason: "pack content \"censer\" is not an item and was not converted" },
      { element: "Kit B", kind: "item", reason: "pack content \"alms box\" is not an item and was not converted" },
      { element: "Feat C", kind: "feat", reason: "prerequisite not enforced: Renown 25+ with a Dragonmarked House" },
    ],
    skipped: [
      { element: "Common|PHB", kind: "language", reason: "defined by the core pack" },
      { element: "Skill|PHB", kind: "skill", reason: "descriptive catalog entry covered by the core pack" },
      { element: "Sidekick|TCE", kind: "class", reason: "class has no hit die" },
    ],
  };

  it("groups notes by reason with counts and collapses expected core skips", async () => {
    expect(reasonKey("ability score maximum 30 \"x\"")).toBe("ability score maximum N \"…\"");
    const groups = groupNotes(report.partial);
    expect(groups.map((g) => g.notes.length)).toEqual([2, 1]);
    expect(report.skipped.filter(isExpectedSkip).length).toBe(2);
    const user = userEvent.setup();
    render(<FiveToolsReport report={report} />);
    expect(screen.getByText(/Replaced 2 existing pack\(s\)/)).toBeTruthy();
    const partial = screen.getByRole("group", { name: "Partially converted" });
    expect(within(partial).getByText("Partially converted (3)")).toBeTruthy();
    expect(within(partial).queryByText("Kit A")).toBeNull();
    await user.click(within(partial).getByRole("button", { name: /pack content "…" is not an item/ }));
    expect(within(partial).getByText("Kit A")).toBeTruthy();
    expect(screen.getByRole("group", { name: "Skipped" }).textContent).not.toContain("Common");
    expect(screen.getByText(/2 entries already defined by the core rules/)).toBeTruthy();
  });

  it("says how many packs a re-import replaced", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/content");
    for (let round = 0; round < 2; round += 1) {
      await user.click(screen.getByRole("button", { name: "Scan folder…" }));
      await screen.findByRole("list", { name: "Books sources" });
      await user.click(screen.getByRole("button", { name: /^Import \d+ book/ }));
      await screen.findByText(/elements converted/);
    }
    expect(screen.getByText(/Replaced 5 existing pack\(s\)/)).toBeTruthy();
    expect((await api.listPacks()).filter((p) => p.kind === "local-import").length).toBe(5);
  });
});
