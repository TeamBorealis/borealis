import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { AppProviders } from "../AppProviders";
import type { ElementDetail, RulesetProfile } from "../api/contract";
import { MockApi } from "../api/mock";
import { DescriptionPane } from "../pages/builder/DescriptionPane";
import { renderApp } from "./utils";

/**
 * B14 (tester video 2026-11-02, B-137): coming back to Build, the first tab read "Race" and the pane "Loading..." for about a
 * second, because the ruleset's wording and the described element loaded again on every mount.
 */
const never = <T,>(): Promise<T> => new Promise<T>(() => {});

describe("Build's first paint (B14)", () => {
  it("Build tab label is Species on first paint for a 2024 character", async () => {
    const api = new MockApi();
    const created = await api.createCharacter({ request: { name: "Sable", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
    // The ruleset profile never arrives: the label must not wait for it.
    api.getRulesetProfile = () => never<RulesetProfile>();
    renderApp("/builder/race", { api, openCharacterId: created.id });
    expect(await screen.findByRole("tab", { name: "Species" })).toBeTruthy();
    expect(screen.queryByRole("tab", { name: "Race" })).toBeNull();
  });

  it("a remounted pane keeps the element it showed while it reloads, without a Loading card", async () => {
    const api = new MockApi();
    api.presetSettings({ showImportNotice: false });
    const pane = () => (
      <AppProviders api={api}>
        <MemoryRouter><DescriptionPane elementId="human" fallback={null} /></MemoryRouter>
      </AppProviders>
    );
    const first = render(pane());
    expect(await screen.findByRole("heading", { name: "Human" }, { timeout: 5000 })).toBeTruthy();
    first.unmount();

    api.getElement = () => never<ElementDetail>();
    render(pane());
    expect(screen.getByRole("heading", { name: "Human" })).toBeTruthy();
    expect(screen.queryByText("Loading...")).toBeNull();
  });
});
