import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MAIN_TABS, shellReachableTargets } from "./components/shell/sections";
import { ROUTES } from "./routes";
import { renderApp } from "./test/utils";

describe("App shell", () => {
  it("exposes every route through the shell and lists the seeded character library", async () => {
    renderApp("/characters");
    const nav = screen.getByRole("navigation", { name: "Main" });
    for (const t of MAIN_TABS) expect(within(nav).getByText(t.label)).toBeTruthy();
    const reachable = shellReachableTargets();
    for (const r of ROUTES) expect(reachable.some((p) => p === r.path || p.startsWith(`${r.path}/`))).toBe(true);
    expect(await screen.findByRole("heading", { name: "Characters" })).toBeTruthy();
    expect(await screen.findByText("Brannoc Ironvale")).toBeTruthy();
    expect(screen.getByText("Seraphine Vale")).toBeTruthy();
  });

  it("shows an empty state on character routes when nothing is open", async () => {
    renderApp("/builder");
    expect(await screen.findByText("No character open")).toBeTruthy();
  });
});
