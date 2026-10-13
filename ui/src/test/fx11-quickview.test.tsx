import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

const expanded = (name: string) => screen.getByRole("button", { name: new RegExp(`^${name}$`, "i") }).getAttribute("aria-expanded");

async function openQuickView(patch: { skillsPanelExpanded: boolean; savingThrowsExpanded: boolean }) {
  const user = userEvent.setup();
  const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
  await api.saveSettings({ patch });
  const id = await seededId(api, "Brannoc");
  renderApp("/builder", { api, openCharacterId: id });
  await user.click(await screen.findByRole("button", { name: /Brannoc Ironvale/i }));
  await screen.findByRole("button", { name: /^Skills$/i });
}

describe("Quick view expanders follow the settings", () => {
  it("opens Skills and closes Saving throws by default", async () => {
    await openQuickView({ skillsPanelExpanded: true, savingThrowsExpanded: false });
    await waitFor(() => expect(expanded("Skills")).toBe("true"));
    expect(expanded("Saving throws")).toBe("false");
  });

  it("honours the opposite settings", async () => {
    await openQuickView({ skillsPanelExpanded: false, savingThrowsExpanded: true });
    await waitFor(() => expect(expanded("Saving throws")).toBe("true"));
    expect(expanded("Skills")).toBe("false");
  });
});
