import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { EmptyPane, NestedPane } from "../pages/builder/DescriptionPane";
import { numberedNames } from "../pages/builder/PickerGroup";

// H-ajc (WB-26 section 17, axe landmark rules; the web e2e `hc-next-landmarks.e2e.ts` runs axe itself): pickers of one
// name are told apart, and a description pane drawn inside another landmark is a region, not an aside.

describe("H-ajc landmarks", () => {
  it("numbers the names used more than once and leaves the others", () => {
    expect(numberedNames(["Fighter skill", "Language", "Fighter skill", "Weapon Mastery"]))
      .toEqual(["Fighter skill 1 of 2", "Language", "Fighter skill 2 of 2", "Weapon Mastery"]);
    expect(numberedNames([])).toEqual([]);
  });

  it("the description pane is an aside at the top level and a region when nested", () => {
    const { unmount } = render(<EmptyPane />);
    expect(screen.getByRole("complementary", { name: "Description" })).toBeTruthy();
    unmount();
    render(<NestedPane.Provider value><EmptyPane /></NestedPane.Provider>);
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(screen.getByRole("region", { name: "Description" })).toBeTruthy();
  });
});
