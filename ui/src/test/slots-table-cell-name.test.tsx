import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

describe("Selection slots: a slot's name is one text", () => {
  it("writes \"<title> (n of m)\" as one text node, so every WebKit names the cell by all of it", async () => {
    // macOS 26's WebKit named the cell by its first text node only ("Cantrip (Wizard 1)" without "(1 of 3)"), so the
    // real-window tests d6 and d12 found no cell "Cantrip (Wizard 1) (1 of 3)" on GitHub's macOS 26 runner
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    const snap = await api.getSnapshot({ characterId: id });
    const counted = snap.slots.find((s) => s.tab === "spells" && s.of > 1);
    expect(counted, "the seed has a slot of a several-pick rule").toBeTruthy();
    renderApp("/spells", { api, openCharacterId: id });
    const table = await screen.findByRole("table", { name: "Selection slots" });
    const name = `${counted!.title} (${counted!.ordinal} of ${counted!.of})`;
    const cell = within(table).getAllByRole("cell").find((c) => c.classList.contains("sp-slot-title") && c.textContent?.startsWith(name));
    expect(cell, `a cell named ${name}`).toBeTruthy();
    const first = cell!.firstChild;
    expect(first?.nodeType).toBe(Node.TEXT_NODE);
    expect(first?.textContent).toBe(name);
  });
});
