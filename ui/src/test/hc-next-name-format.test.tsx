import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import type { HomebrewFieldSpec } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { CreatorEnvValue } from "../pages/homebrew/env";
import { FieldView } from "../pages/homebrew/FieldView";
import { ApiProvider } from "../state/ApiContext";

// "Name when made" with value chips (board HmGzM, spec 7.4): an item's `name-format` is plain text with the base item's
// name and the enhancement bonus inserted as values, shown by their words and never as `{{parent}}`; braces can't be typed.

const field = (): HomebrewFieldSpec => MOCK_KIND_DESCRIPTORS.find((k) => k.id === "magicItem")!.fields.find((f) => f.key === "nameFormat")!;

let last = "";
function Harness({ start }: { start: string }) {
  const [value, setValue] = useState(start);
  return (
    <ApiProvider api={new MockApi()}>
      <CreatorEnvValue value={{}}>
        <FieldView field={field()} value={value} problems={[]} kindId="magicItem" onChange={(v) => { last = String(v); setValue(String(v)); }} />
      </CreatorEnvValue>
    </ApiProvider>
  );
}

describe("Name when made", () => {
  it("is the name-format control", () => {
    expect(field().control).toBe("nameFormat");
  });

  it("shows the values by their words and inserts them from buttons", async () => {
    const user = userEvent.setup();
    render(<Harness start="{{parent}} +{{enhancement}}" />);
    const group = screen.getByRole("group", { name: "Name when made" });
    const chips = [...group.querySelectorAll(".hb-value-chip .hb-ref-name")].map((c) => c.textContent);
    expect(chips).toEqual(["Base item's name", "Enhancement bonus"]);
    expect(group.textContent).not.toContain("{{");
    await user.click(within(group).getByRole("button", { name: "Remove the value Base item's name" }));
    expect(last).toBe(" +{{enhancement}}");
    await user.click(within(group).getByRole("button", { name: "Insert the base item's name" }));
    expect(last).toBe(" +{{enhancement}}{{parent}}");
  });

  it("refuses typed braces", async () => {
    const user = userEvent.setup();
    render(<Harness start="" />);
    await user.type(screen.getByRole("textbox", { name: "Name when made" }), "Vicious {{x}}");
    expect(last).toBe("Vicious x");
    await user.click(screen.getByRole("button", { name: "Insert the enhancement bonus" }));
    expect(last).toBe("Vicious x{{enhancement}}");
  });
});
