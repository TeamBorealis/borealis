import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

async function setup(route: string, rowHeight?: "small" | "medium" | "large") {
  const api = new MockApi({ aiChunkDelayMs: 2 });
  if (rowHeight) await api.saveSettings({ patch: { rowHeight } });
  const snap = await api.createCharacter({ request: { name: "Fx10", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
  renderApp(route, { api, openCharacterId: snap.id });
}

describe("two-pane common", () => {
  it("marks the first open picker group so the focus bar lights on mount", async () => {
    await setup("/builder/class");
    await screen.findByRole("list", { name: "Class options" });
    const open = document.querySelectorAll(".pick.is-open");
    expect(open.length).toBeGreaterThan(0);
    expect(document.querySelector(".pick-stack .pick.is-open")).toBe(open[0]);
  });

  it.each([["small", "17px"], ["medium", "21px"], ["large", "25px"]] as const)("applies %s picker row height", async (size, px) => {
    await setup("/builder/class", size);
    await screen.findByRole("list", { name: "Class options" });
    await waitFor(() => expect((document.querySelector(".pick") as HTMLElement).style.getPropertyValue("--pick-row")).toBe(px));
  });
});
