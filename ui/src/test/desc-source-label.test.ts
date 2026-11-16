import { describe, expect, it } from "vitest";
import { sourceLabel } from "../pages/builder/DescriptionPane";

// The description pane's SOURCE line names the book and, for an element of one edition, its year. A 5etools import's
// book name already ends with it, and the line read "Player's Handbook (2024) (2024)" (the macOS real-window test d8 on
// GitHub's runner, lane O).
describe("description pane SOURCE line", () => {
  it("adds the edition's year to a book name that does not say it", () => {
    expect(sourceLabel("SRD 5.2.1", ["dnd2024"])).toBe("SRD 5.2.1 (2024)");
    expect(sourceLabel("SRD 5.1", ["dnd2014"])).toBe("SRD 5.1 (2014)");
  });

  it("does not repeat a year the book name already ends with", () => {
    expect(sourceLabel("Player's Handbook (2024)", ["dnd2024"])).toBe("Player's Handbook (2024)");
  });

  it("names the book alone for an element of both editions", () => {
    expect(sourceLabel("Ember Isles", ["dnd2014", "dnd2024"])).toBe("Ember Isles");
  });
});
