import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { asCitedAnswer, splitCitations } from "../pages/ai/citations";
import { renderApp } from "./utils";

const STREAM_TIMEOUT_MS = 10_000;

const VALID = {
  answerMarkdown: "Speed 0 [[ID_A]].", citations: ["ID_A"], sources: [{ id: "ID_A", name: "Grappled", type: "Condition" }],
  confidence: "high", notCovered: false, strippedCitations: [], uncitedClaims: [], unverified: false,
};

describe("Rules Q&A citations: parsing", () => {
  it("splits answer text at its [[id]] markers, in order", () => {
    expect(splitCitations("A [[ID_A]] and [[ID_B]].")).toEqual([
      { kind: "text", text: "A " }, { kind: "cite", id: "ID_A" }, { kind: "text", text: " and " }, { kind: "cite", id: "ID_B" }, { kind: "text", text: "." },
    ]);
    expect(splitCitations("no markers")).toEqual([{ kind: "text", text: "no markers" }]);
    expect(splitCitations("[[not an id]]")).toEqual([{ kind: "text", text: "[[not an id]]" }]);
  });

  it("accepts a well-formed rules_qa payload and rejects a malformed one", () => {
    expect(asCitedAnswer(VALID)).toEqual(VALID);
    expect(asCitedAnswer({ ...VALID, sources: [{ id: "ID_A" }] })).toBeNull();
    expect(asCitedAnswer({ ...VALID, confidence: "certain" })).toBeNull();
    expect(asCitedAnswer(null)).toBeNull();
  });
});

describe("Rules Q&A citations: page", () => {
  it("renders inline citations and source chips, and a chip opens the cited entry in the compendium", async () => {
    const user = userEvent.setup();
    renderApp("/ai?mode=rules_qa", { aiEnabled: true });
    await screen.findByText(/claude ready/);
    await user.type(screen.getByLabelText("Your message"), "Does Sentinel stop movement?");
    await user.click(screen.getByRole("button", { name: "Send" }));

    const chips = await screen.findByRole("list", { name: "Citations" }, { timeout: STREAM_TIMEOUT_MS });
    const chip = within(chips).getByRole("button", { name: /Sentinel/ });
    expect(chip.textContent).toMatch(/feat/i);
    const assistant = document.querySelector('.msg[data-role="assistant"]') as HTMLElement;
    expect(assistant.textContent).not.toContain("[[");
    expect(within(assistant).getByRole("button", { name: "Citation: Sentinel" })).toBeTruthy();
    expect(screen.getByText("high confidence")).toBeTruthy();

    await user.click(chip);
    const desc = await screen.findByRole("complementary", { name: "Element description" });
    expect(await within(desc).findByRole("article", { name: "Sentinel" })).toBeTruthy();
    await waitFor(() => expect(within(desc).getByText(/opportunity attacks/)).toBeTruthy());
  });
});
