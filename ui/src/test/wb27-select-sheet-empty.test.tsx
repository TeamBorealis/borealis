import { describe, expect, it } from "vitest";
import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { NO_CHOICE, SelectSheet } from "../components/controls/SelectSheet";

function Harness() {
  const [value, setValue] = useState("");
  return <SelectSheet aria-label="School" value={value} onChange={setValue}
    options={[{ value: "", label: "" }, { value: "Evocation", label: "Evocation" }]} />;
}

describe("phone select sheet: an option with no words (WB-27 axe, button-name)", () => {
  it("is a named row, None, and choosing it is still the empty value", () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /School/ }));
    const rows = screen.getAllByRole("option");
    expect(rows.map((r) => r.textContent)).toEqual([NO_CHOICE, "Evocation"]);
    expect(screen.getByRole("option", { name: NO_CHOICE }).getAttribute("aria-selected")).toBe("true");
  });
});
