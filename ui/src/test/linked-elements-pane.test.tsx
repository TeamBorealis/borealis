import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { AppProviders } from "../AppProviders";
import type { ElementDetail, ElementLink } from "../api/contract";
import { MockApi } from "../api/mock";
import { DescriptionPane } from "../pages/builder/DescriptionPane";

/**
 * Linked Elements (opt-in): an option that grants, creates or names other elements shows each one's card, with its text,
 * below its own description; a long list starts folded, and a card's link opens the element in the compendium.
 */
const link = (id: string, name: string, contentType: string, via: ElementLink["via"] = "grant"): ElementLink => ({
  id, name, type: "option", source: "SRD 5.2.1", sourceName: "SRD 5.2.1", shortDescription: "", prerequisites: null, rulesets: ["dnd2024"],
  contentType, via, descriptionHtml: `<p>${name} text.</p>`,
});

const detail = (links: ElementLink[]): ElementDetail => ({
  id: "opt", name: "Sample Option", type: "option", source: "SRD 5.2.1", sourceName: "SRD 5.2.1", shortDescription: "",
  prerequisites: null, rulesets: ["dnd2024"], descriptionHtml: "<p>You create a sample item.</p>", facts: [], grants: [], links,
});

function renderPane(d: ElementDetail, linkedElements: boolean) {
  const api = new MockApi();
  api.presetSettings({ showImportNotice: false, linkedElements });
  api.getElement = async ({ elementId }) => (elementId === d.id ? d : { ...d, id: elementId, name: "Opened", links: [] });
  render(<AppProviders api={api}><MemoryRouter><DescriptionPane elementId={d.id} fallback={null} /></MemoryRouter></AppProviders>);
}

describe("the description pane's linked elements", () => {
  it("shows each linked element's card with its text when the setting is on", async () => {
    renderPane(detail([link("bag", "Sample Bag", "Magic Item"), link("cond", "Sample Condition", "Condition", "rule")]), true);
    const linked = await screen.findByRole("region", { name: "Linked elements" });
    const cards = within(linked).getAllByRole("group");
    expect(cards.map((c) => (c as HTMLDetailsElement).open)).toEqual([true, true]);
    expect(within(linked).getByText("Sample Bag text.")).toBeTruthy();
    expect(within(linked).getByText("Magic Item · Granted")).toBeTruthy();
    expect(within(linked).getByText("Condition · Used")).toBeTruthy();
    expect(within(linked).getByRole("button", { name: "Open Sample Bag in the compendium" })).toBeTruthy();
  });

  it("folds a long list, one name per row", async () => {
    const many = Array.from({ length: 5 }, (_, i) => link(`s${i}`, `Sample Spell ${i}`, "Spell"));
    renderPane(detail(many), true);
    const linked = await screen.findByRole("region", { name: "Linked elements" });
    const cards = within(linked).getAllByRole("group") as HTMLDetailsElement[];
    expect(cards.map((c) => c.open)).toEqual([false, false, false, false, false]);
    await userEvent.setup().click(within(linked).getByText("Sample Spell 2"));
    expect(cards[2]!.open).toBe(true);
  });

  it("shows nothing extra while the setting is off", async () => {
    renderPane(detail([link("bag", "Sample Bag", "Magic Item")]), false);
    await screen.findByRole("heading", { name: "Sample Option" });
    expect(screen.queryByRole("region", { name: "Linked elements" })).toBeNull();
    expect(screen.queryByText("Sample Bag text.")).toBeNull();
  });
});
