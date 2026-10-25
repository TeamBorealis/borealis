import { act, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import type { CharacterSnapshot, LinkStatus } from "../api/contract";
import { announceBundleApplied } from "../pages/transfer/openBundle";
import { renderApp, seededId } from "./utils";

// HW1 S9 (emulator): a newer copy of the open character arrived from another device, but the screen kept
// showing the old copy until the app restarted. The open character is re-read after a receive or an
// accepted bundle (the backend reloads its session from the received file).
const emit = (api: MockApi, status: Partial<LinkStatus>) =>
  (api as unknown as { emit: (e: string, p: unknown) => void }).emit("link:status", status);

async function openSeraphine() {
  const api = new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
  const id = await seededId(api, "Seraphine");
  renderApp("/manage", { api, openCharacterId: id });
  await screen.findAllByText(/Seraphine/);
  const current = await api.getSnapshot({ characterId: id });
  const spy = vi.spyOn(api, "getSnapshot").mockResolvedValue({ ...current, name: "Seraphine Received" } as CharacterSnapshot);
  return { api, id, spy };
}

describe("the open character follows a received copy (HW1 S9)", () => {
  it("re-reads the open character when a receive finishes", async () => {
    const { api, id, spy } = await openSeraphine();
    await act(async () => { emit(api, { role: "receive", phase: "done" }); });
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ characterId: id }));
    expect((await screen.findAllByText(/Seraphine Received/)).length).toBeGreaterThan(0);
  });

  it("re-reads it after an opened bundle is accepted, but not when this device only sent", async () => {
    const { api, spy } = await openSeraphine();
    await act(async () => { emit(api, { role: "send", phase: "done" }); });
    expect(spy).not.toHaveBeenCalled();
    await act(async () => { announceBundleApplied(); });
    await waitFor(() => expect(spy).toHaveBeenCalled());
  });
});
