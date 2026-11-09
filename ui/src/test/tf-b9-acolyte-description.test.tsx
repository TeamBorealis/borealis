import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { AppProviders } from "../AppProviders";
import type { ElementDetail } from "../api/contract";
import { MockApi } from "../api/mock";
import { DescriptionPane, repeatsBody } from "../pages/builder/DescriptionPane";

/**
 * B9 (tester video 2026-11-02, B-134): the host's short description is the body's first 160 characters plus "...", so the
 * duplicate check missed it and the card printed the same text twice (italic, then as the body).
 */
const BODY = "<p><b>Ability Scores:</b> Intelligence, Wisdom, Charisma</p><p><b>Feat:</b> Sample Feat (Cleric)</p>"
  + "<p><b>Skill Proficiencies:</b> Insight and Religion</p><p><b>Tool Proficiency:</b> Calligrapher&rsquo;s Supplies</p>"
  + "<p><b>Equipment:</b> Choose A or B: (A) a sample kit; or (B) 50 GP</p>";
const SHORT = "Ability Scores: Intelligence, Wisdom, Charisma Feat: Sample Feat (Cleric) Skill Proficiencies: Insight and Religion Tool Proficiency: Calligrapher’s Supplies...";

const detail = (shortDescription: string): ElementDetail => ({
  id: "bg-sample", name: "Sample Background", type: "background", source: "SRD 5.2.1", sourceName: "SRD 5.2.1", shortDescription,
  prerequisites: null, rulesets: ["dnd2024"], descriptionHtml: BODY, facts: [], grants: [],
} as unknown as ElementDetail);

function renderPane(d: ElementDetail) {
  const api = new MockApi();
  api.presetSettings({ showImportNotice: false });
  api.getElement = async () => d;
  render(<AppProviders api={api}><MemoryRouter><DescriptionPane elementId={d.id} fallback={null} /></MemoryRouter></AppProviders>);
}

describe("the description's summary line (B9)", () => {
  it("a truncated short description is not repeated above the body", async () => {
    renderPane(detail(SHORT));
    await screen.findByRole("heading", { name: "Sample Background" });
    expect(document.querySelector(".desc-intro")).toBeNull();
  });

  it("a short description the body does not hold still shows", async () => {
    renderPane(detail("A summary written on its own."));
    await screen.findByRole("heading", { name: "Sample Background" });
    expect(document.querySelector(".desc-intro")?.textContent).toBe("A summary written on its own.");
  });

  it("compares text, ignoring tags, entities, spacing and the trailing ellipsis", () => {
    expect(repeatsBody(SHORT, BODY)).toBe(true);
    expect(repeatsBody("Ability Scores: Intelligence, Wisdom…", BODY)).toBe(true);
    expect(repeatsBody("Wisdom, Strength...", BODY)).toBe(false);
    expect(repeatsBody("", BODY)).toBe(true);
  });
});
