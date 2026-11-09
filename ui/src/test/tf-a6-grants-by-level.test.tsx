import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { AppProviders } from "../AppProviders";
import type { ElementDetail, ElementGrant } from "../api/contract";
import { MockApi } from "../api/mock";
import { DescriptionPane } from "../pages/builder/DescriptionPane";

/**
 * A6 (tester video 2026-11-02, B-117): a class card listed every feature up to level 20 in one flat list. It now shows what
 * comes at level 1 and folds the rest, in level order, under "Later levels".
 */
const g = (name: string, level = 1): ElementGrant => ({ name, level, descriptionHtml: "" });

const detail = (grants: ElementGrant[]): ElementDetail => ({
  id: "cls", name: "Sample Class", type: "class", source: "SRD 5.2.1", sourceName: "SRD 5.2.1", shortDescription: "",
  prerequisites: null, rulesets: ["dnd2024"], descriptionHtml: "<p>A sample class.</p>", facts: [], grants,
} as unknown as ElementDetail);

function renderPane(d: ElementDetail) {
  const api = new MockApi();
  api.presetSettings({ showImportNotice: false });
  api.getElement = async () => d;
  render(<AppProviders api={api}><MemoryRouter><DescriptionPane elementId={d.id} fallback={null} /></MemoryRouter></AppProviders>);
}

describe("the Grants list by level (A6)", () => {
  it("later-level grants are collapsed", async () => {
    renderPane(detail([g("Shield Training"), g("Battle Fury"), g("Keen Senses", 2), g("Grand Finale", 20)]));
    await screen.findByRole("heading", { name: "Sample Class" });
    const first = screen.getByRole("list", { name: "At level 1" });
    expect(within(first).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["Shield Training", "Battle Fury"]);
    const later = screen.getByText("Later levels").closest("details")!;
    expect(later.open).toBe(false);
    await userEvent.setup().click(screen.getByText("Later levels"));
    expect(later.open).toBe(true);
    // Level by level, in level order.
    expect(within(later).getAllByRole("list").map((ul) => ul.getAttribute("aria-label"))).toEqual(["Level 2", "Level 20"]);
    expect(within(within(later).getByRole("list", { name: "Level 2" })).getByRole("listitem").textContent).toBe("Keen Senses");
    expect(within(within(later).getByRole("list", { name: "Level 20" })).getByRole("listitem").textContent).toBe("Grand Finale");
  });

  it("a list with only level 1 grants has no level labels", async () => {
    renderPane(detail([g("Shield Training"), g("Battle Fury")]));
    await screen.findByRole("heading", { name: "Sample Class" });
    expect(screen.getByRole("list", { name: "Grants" })).toBeTruthy();
    expect(screen.queryByText("Later levels")).toBeNull();
    expect(screen.queryByText("At level 1")).toBeNull();
  });
});
