import { act, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

// B-24: files the OS opens with Borealis ("Open with", Files > Share, Android VIEW intents).
const mock = (extra: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ rng: () => 0.5, ...extra });
const hero = (name: string) => ({ fileName: `${name.split(" ")[0]!.toLowerCase()}.dnd5e`, contentBase64: btoa(JSON.stringify({ name })) });

describe("Open with: character files", () => {
  it("lists a character opened while the Characters page is showing (a cold start lands there)", async () => {
    const api = mock({ mobile: true });
    renderApp("/characters", { api });
    await screen.findByRole("button", { name: /Import characters/ });
    expect(screen.queryByText(/Orla Brightwater/)).toBeNull();
    api.openedFiles = [hero("Orla Brightwater")];
    act(() => { window.dispatchEvent(new CustomEvent("borealis-open-file")); });
    expect(await screen.findByText("Imported Orla Brightwater.")).toBeTruthy();
    // The toast names the character too; the list entry is the second match.
    await waitFor(() => expect(screen.getAllByText(/Orla Brightwater/).length).toBeGreaterThan(1));
  });

  it("drains files parked before the page mounted, once", async () => {
    const api = mock({ mobile: true });
    const take = vi.spyOn(api, "takeOpenedFiles");
    const importCharacter = vi.spyOn(api, "importCharacter");
    api.openedFiles = [hero("Tamsin Reed")];
    renderApp("/characters", { api });
    expect(await screen.findByText("Imported Tamsin Reed.")).toBeTruthy();
    await waitFor(() => expect(take).toHaveBeenCalled());
    act(() => { window.dispatchEvent(new CustomEvent("borealis-open-file")); });
    await waitFor(() => expect(take.mock.calls.length).toBeGreaterThan(1));
    expect(importCharacter).toHaveBeenCalledTimes(1);
  });
});
