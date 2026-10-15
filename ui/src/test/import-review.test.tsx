import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { apiError } from "../api/mock/errors";
import type { ImportElement } from "../api/contract";
import { ElementRow } from "../pages/import/ElementCard";
import { renderApp } from "./utils";

type User = ReturnType<typeof userEvent.setup>;
const mock = (aiEnabled = true) => new MockApi({ aiChunkDelayMs: 2, importStepMs: 1, aiEnabled, rng: () => 0.5 });

async function reviewClass(user: User, api = mock()) {
  const view = renderApp("/import", { api });
  await user.type(await screen.findByLabelText("Paste text"), "Runebinder: a half-caster class that etches runes.");
  await user.click(screen.getByRole("button", { name: "Import" }));
  await screen.findByText("Level 1–20 test passed ✓", {}, { timeout: 3000 });
  return view;
}
const regions = () => screen.getAllByRole("region").filter((r) => r.closest("ul[aria-label='Imported elements']")).map((r) => r.getAttribute("aria-label"));
const region = (name: string) => screen.getByRole("region", { name });
const pick = (user: User, name: string) => user.click(within(region(name)).getByRole("button", { name: new RegExp(name) }));
const passageLines = () => [...document.querySelectorAll("[data-passage]")].map((l) => Number(l.getAttribute("data-line")));

describe("import review screen (HB2)", () => {
  it("lists flagged elements first and filters with counts", async () => {
    const user = userEvent.setup();
    await reviewClass(user);
    expect(regions().slice(0, 4)).toEqual(["Master Runebinder", "Rockling", "Hearth Rite", "Cairn Staff"]);
    expect(screen.getByText(/4 of 13 elements need a look/)).toBeTruthy();
    for (const label of ["All (13)", "Check this (3)", "Not imported (1)", "OK (9)"]) expect(screen.getByRole("button", { name: label })).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Not imported (1)" }));
    expect(regions()).toEqual(["Cairn Staff"]);
    expect(within(region("Cairn Staff")).getByText("Not imported")).toBeTruthy();
    expect(within(region("Cairn Staff")).getByText(/generation failed/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "OK (9)" }));
    expect(regions()).toHaveLength(9);
    expect(screen.getByRole("button", { name: "OK (9)" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("says plainly when nothing is flagged", async () => {
    const user = userEvent.setup();
    renderApp("/import", { api: mock() });
    await user.type(await screen.findByLabelText("Describe it"), "a feat that lets you jump");
    await user.click(screen.getByRole("button", { name: "Feat" }));
    await user.click(screen.getByRole("button", { name: "Import" }));
    expect((await screen.findByText(/Nothing needs your attention: all 1 element imported cleanly/, {}, { timeout: 3000 })).textContent).toBeTruthy();
  });

  it("shows the reason beside a badge and scrolls to and highlights the element's source passage", async () => {
    const user = userEvent.setup();
    await reviewClass(user);
    const capstone = region("Master Runebinder");
    expect(within(capstone).getByText("Check this")).toBeTruthy();
    expect(within(capstone).getByText(/capstone has no cost/)).toBeTruthy();
    expect(passageLines()).toEqual([16, 17]);
    expect(within(capstone).getByRole("button", { name: "Line 16, in this element's passage" })).toBeTruthy();

    await pick(user, "Rockling");
    expect(passageLines()).toEqual([19, 20, 21, 22, 23, 24]);
    expect(region("Rockling").querySelector(".v5-sub")?.textContent).toMatch(/flying speed of 30 feet/);
  });

  it("collapses the rest of an element's notes behind a details disclosure", async () => {
    const user = userEvent.setup();
    await reviewClass(user);
    const details = within(region("Master Runebinder")).getByText(/^Details \(/).closest("details") as HTMLDetailsElement;
    expect(details.open).toBe(false);
    await user.click(within(details).getByText(/^Details \(/));
    expect(details.open).toBe(true);
    expect(within(details).getAllByText(/Spell level cap missing/)).toHaveLength(1);
  });

  it("relocates an element to the lines picked in the source", async () => {
    const user = userEvent.setup();
    const api = mock();
    const calls: unknown[] = [];
    const original = api.importRelocateElement.bind(api);
    api.importRelocateElement = (a) => { calls.push(a); return original(a); };
    await reviewClass(user, api);
    await pick(user, "Hearth Rite");
    expect(passageLines()).toEqual([]);
    const pane = within(region("Hearth Rite"));
    expect((pane.getByRole("button", { name: /Use this passage/ }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(pane.getByRole("button", { name: "Line 7" }));
    await user.click(pane.getByRole("button", { name: "Line 8" }));
    await user.click(pane.getByRole("button", { name: "Use this passage (lines 7 to 8)" }));
    expect(calls).toEqual([expect.objectContaining({ elementId: "hearth-rite", startLine: 7, endLine: 8 })]);
    await waitFor(() => expect(passageLines()).toEqual([7, 8]));
    await waitFor(() => expect(within(region("Hearth Rite")).getByText("OK")).toBeTruthy());
  });

  it("regenerates from the current passage", async () => {
    const user = userEvent.setup();
    const api = mock();
    const calls: unknown[] = [];
    const relocated: unknown[] = [];
    const original = api.importRegenerateElement.bind(api);
    api.importRegenerateElement = (a) => { calls.push(a); return original(a); };
    api.importRelocateElement = (a) => { relocated.push(a); return Promise.resolve(); };
    await reviewClass(user, api);
    await user.click(within(region("Master Runebinder")).getByRole("button", { name: "Regenerate" }));
    expect(calls).toEqual([expect.objectContaining({ elementId: "master-runebinder" })]);
    expect(relocated).toEqual([]);
    await waitFor(() => expect(within(region("Master Runebinder")).getByText("OK")).toBeTruthy());
  });

  it("lists the headings that were not imported in a banner above the elements", async () => {
    const user = userEvent.setup();
    await reviewClass(user);
    expect(screen.getByText(/These headings weren't imported: Camp Rules, Road Lore\./)).toBeTruthy();
  });

  it("accepts a flagged element with Looks right and updates the counts", async () => {
    const user = userEvent.setup();
    await reviewClass(user);
    await user.click(within(region("Master Runebinder")).getByRole("button", { name: "Looks right" }));
    await waitFor(() => expect(within(region("Master Runebinder")).getByText("OK")).toBeTruthy());
    expect(screen.getByRole("button", { name: "Check this (2)" })).toBeTruthy();
    expect(within(region("Master Runebinder")).getByText(/marked it as fine/)).toBeTruthy();
    await user.click(within(region("Master Runebinder")).getByRole("button", { name: /Undo/ }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Check this (3)" })).toBeTruthy());
  });

  it("offers Fix with AI per element only when AI is on, and everything else works without it", () => {
    const element: ImportElement = {
      id: "e1", type: "feat", name: "Brisk Walker", status: "needs_review", sourceExcerpt: "x", json: { name: "Brisk Walker" }, balanceFlags: [], validationIssues: [],
      sourcePassage: { startLine: 1, endLine: 1, text: "x" },
      review: { state: "check", code: "untraced", reason: "A number is not in the source", details: [], textOnly: false, textOnlyReason: "", textOnlyOverride: null, accepted: false },
    };
    const calls: string[] = [];
    const props = { element, selected: true, busy: false, sourceLines: ["x"], error: null, onSelect: () => undefined, onSaveJson: async () => true,
      onAccept: () => calls.push("accept"), onRelocate: () => calls.push("relocate"), onRegenerate: () => calls.push("regenerate"), onRevise: () => calls.push("revise"), onTextOnly: () => calls.push("textonly") };
    const off = render(<ElementRow {...props} aiEnabled={false} />);
    expect(screen.queryByLabelText(/with AI/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Fix with AI" })).toBeNull();
    for (const name of ["Looks right", "Regenerate"]) expect((screen.getByRole("button", { name }) as HTMLButtonElement).disabled).toBe(false);
    expect(screen.getByRole("radio", { name: "Text only" })).toBeTruthy();
    off.unmount();
    render(<ElementRow {...props} aiEnabled />);
    expect(screen.getByLabelText("Fix Brisk Walker with AI")).toBeTruthy();
  });

  it("marks text-only elements and applies the manual override", async () => {
    const user = userEvent.setup();
    const api = mock();
    const calls: unknown[] = [];
    const original = api.importSetTextOnly.bind(api);
    api.importSetTextOnly = (a) => { calls.push(a); return original(a); };
    await reviewClass(user, api);
    await pick(user, "Pebble Charm");
    const charm = within(region("Pebble Charm"));
    expect(charm.getByText("Text only", { selector: ".v5-tag" })).toBeTruthy();
    expect(charm.getByRole("note").textContent).toContain("The sheet won't calculate this; it's shown as text");
    expect((charm.getByRole("radio", { name: "Automatic" }) as HTMLInputElement).checked).toBe(true);

    await user.click(charm.getByRole("radio", { name: "Has working rules" }));
    expect(calls).toEqual([expect.objectContaining({ elementId: "pebble-charm", textOnly: false })]);
    await waitFor(() => expect(within(region("Pebble Charm")).queryByRole("note")).toBeNull());
    expect((within(region("Pebble Charm")).getByRole("radio", { name: "Has working rules" }) as HTMLInputElement).checked).toBe(true);

    await user.click(within(region("Pebble Charm")).getByRole("radio", { name: "Text only" }));
    await waitFor(() => expect(within(region("Pebble Charm")).getByRole("note")).toBeTruthy());
  });

  it("keeps the draft and says what went wrong when an action fails", async () => {
    const user = userEvent.setup();
    const api = mock();
    api.importAcceptElement = () => Promise.reject(apiError("internal", "The disk is full."));
    api.importRegenerateElement = () => Promise.reject(apiError("internal", "The model is not reachable."));
    await reviewClass(user, api);
    await user.click(within(region("Master Runebinder")).getByRole("button", { name: "Looks right" }));
    const alert = await within(region("Master Runebinder")).findByRole("alert");
    expect(alert.textContent).toMatch(/Could not mark this as fine/);
    expect(alert.textContent).toMatch(/Your draft is unchanged/);
    expect(regions()).toHaveLength(13);

    await user.click(within(region("Master Runebinder")).getByRole("button", { name: "Regenerate" }));
    expect((await within(region("Master Runebinder")).findByRole("alert")).textContent).toMatch(/model is not reachable/);
    expect(regions()).toHaveLength(13);
    expect((screen.getByRole("button", { name: "Add to Library" }) as HTMLButtonElement).disabled).toBe(false);
  });
});
