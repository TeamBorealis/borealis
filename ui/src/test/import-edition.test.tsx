import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import type { ImportStartRequest } from "../api/contract";
import { renderApp } from "./utils";

const fast = () => new MockApi({ aiChunkDelayMs: 2, importStepMs: 1, aiEnabled: true, rng: () => 0.5 });

async function startWith(edition: "2014" | "2024" | null): Promise<ImportStartRequest> {
  const user = userEvent.setup();
  const api = fast();
  const spy = vi.spyOn(api, "importStart");
  renderApp("/import", { api });
  await user.type(await screen.findByLabelText("Paste text"), "Test Shadow Path: a rogue subclass.");
  if (edition) await user.click(screen.getByRole("button", { name: edition }));
  await user.click(screen.getByRole("button", { name: "Import" }));
  await screen.findByRole("progressbar", { name: "Import progress" });
  return spy.mock.calls[0]![0];
}

describe("import edition override", () => {
  it("sends no edition when left on Auto, so detection decides", async () => {
    const req = await startWith(null);
    expect(req.hint?.edition).toBeUndefined();
  });

  it("sends the chosen edition as the explicit override", async () => {
    expect((await startWith("2014")).hint?.edition).toBe("dnd2014");
    expect((await startWith("2024")).hint?.edition).toBe("dnd2024");
  });
});
