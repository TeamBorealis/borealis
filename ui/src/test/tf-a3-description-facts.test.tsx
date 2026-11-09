import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { AppProviders } from "../AppProviders";
import type { ElementDetail, ElementFact, ElementGrant } from "../api/contract";
import { MockApi } from "../api/mock";
import { DescriptionPane } from "../pages/builder/DescriptionPane";

/**
 * A3 (tester video 2026-11-02, B-114): the description read like raw data. It printed the host's Type, Source and Category
 * facts (so "Source" showed twice) and listed granted traits by name only, so a species card did not say what it does.
 */
const detail = (facts: ElementFact[], grants: ElementGrant[] = []): ElementDetail => ({
  id: "el", name: "Sample Entry", type: "race", source: "SRD 5.2.1", sourceName: "SRD 5.2.1", shortDescription: "",
  prerequisites: null, rulesets: ["dnd2024"], descriptionHtml: "<p>A sample entry.</p>", facts, grants,
} as unknown as ElementDetail);

function renderPane(d: ElementDetail) {
  const api = new MockApi();
  api.presetSettings({ showImportNotice: false });
  api.getElement = async () => d;
  render(<AppProviders api={api}><MemoryRouter><DescriptionPane elementId={d.id} fallback={null} /></MemoryRouter></AppProviders>);
}

const pane = () => screen.getByRole("complementary", { name: "Description" });

describe("the description reads as text, not data (A3)", () => {
  it("builder description omits Type, Source and Category facts", async () => {
    renderPane(detail([
      { label: "Type", value: "Racial traits" }, { label: "Source", value: "SRD 5.2.1" }, { label: "Category", value: "Species Trait Option" },
      { label: "Cost", value: "10 gp" }, { label: "Damage", value: "1d8 slashing" }, { label: "Properties", value: "Versatile (1d10)" }, { label: "Weight", value: "4" },
    ]));
    await screen.findByRole("heading", { name: "Sample Entry" });
    const text = pane().textContent ?? "";
    for (const gone of ["Type.", "Category.", "Racial traits", "Species Trait Option"]) expect(text).not.toContain(gone);
    for (const kept of ["Cost. 10 gp", "Damage. 1d8 slashing", "Properties. Versatile (1d10)", "Weight. 4"]) expect(text).toContain(kept);
    // "Source" once: the footer heading.
    expect(text.match(/Source/g)).toHaveLength(1);
  });

  it("a granted trait shows its text under its own heading; a proficiency stays one line", async () => {
    renderPane(detail([], [
      { name: "Proficiency: Sample Skill", level: 1, descriptionHtml: "" },
      { name: "Fiery Breath", level: 1, descriptionHtml: "<p>Each creature in the cone makes a Dexterity saving throw.</p>" },
    ]));
    await screen.findByRole("heading", { name: "Sample Entry" });
    const trait = screen.getByRole("heading", { name: "Fiery Breath" });
    expect(trait.nextElementSibling?.textContent).toContain("Dexterity saving throw");
    const lines = screen.getByRole("list", { name: "Grants" });
    expect(within(lines).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["Proficiency: Sample Skill"]);
  });
});
